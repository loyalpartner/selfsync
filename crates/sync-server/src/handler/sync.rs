use axum::{
    Extension,
    body::Bytes,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use prost::Message;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};

use crate::auth::ClientIdentity;
use crate::db::entity::user;
use crate::proto::sync_pb;
use crate::util::gen_encryption_key;

use super::{commit, get_updates};

/// POST /command/ — handles all Chrome sync protocol messages.
pub async fn handle_command(
    Extension(db): Extension<DatabaseConnection>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let msg = match sync_pb::ClientToServerMessage::decode(body.as_ref()) {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("failed to decode ClientToServerMessage: {e}");
            return (StatusCode::BAD_REQUEST, Vec::new());
        }
    };

    let identity = ClientIdentity::from_request(&headers, &msg);
    let msg_type = message_type_name(msg.message_contents);
    tracing::info!(
        email = %identity.email,
        browser = %identity.browser,
        msg_type,
        body_len = body.len(),
        share = %msg.share,
        cache_guid = request_cache_guid(&msg),
        client_birthday = msg.store_birthday.as_deref().unwrap_or("<none>"),
        "sync request"
    );

    let user = match find_or_create_user(&db, &identity).await {
        Ok(u) => u,
        Err(e) => {
            tracing::error!(email = %identity.email, "user lookup failed: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, Vec::new());
        }
    };

    if birthday_mismatch(&msg, &user) {
        let resp = error_response(
            &user.store_birthday,
            sync_pb::sync_enums::ErrorType::NotMyBirthday,
        );
        return (StatusCode::OK, resp.encode_to_vec());
    }

    let mut resp = sync_pb::ClientToServerResponse {
        error_code: Some(sync_pb::sync_enums::ErrorType::Success as i32),
        store_birthday: Some(user.store_birthday.clone()),
        client_command: Some(default_client_command()),
        new_bag_of_chips: Some(default_chip_bag()),
        ..Default::default()
    };

    dispatch(&db, &user, msg, &mut resp).await;

    let resp_bytes = resp.encode_to_vec();
    tracing::debug!(
        email = %identity.email,
        msg_type,
        resp_len = resp_bytes.len(),
        "response sent"
    );
    (StatusCode::OK, resp_bytes)
}

/// Cache GUID from either the COMMIT envelope (preferred) or the deprecated
/// `invalidator_client_id` field. Used as a per-device identifier in logs.
fn request_cache_guid(msg: &sync_pb::ClientToServerMessage) -> &str {
    let from_commit = msg.commit.as_ref().and_then(|c| c.cache_guid.as_deref());
    #[allow(deprecated)]
    let from_invalidator = msg.invalidator_client_id.as_deref();
    from_commit.or(from_invalidator).unwrap_or("<none>")
}

async fn dispatch(
    db: &DatabaseConnection,
    user: &user::Model,
    msg: sync_pb::ClientToServerMessage,
    resp: &mut sync_pb::ClientToServerResponse,
) {
    use sync_pb::client_to_server_message::Contents;

    let contents = msg.message_contents;
    if contents == Contents::Commit as i32 {
        let Some(commit_msg) = msg.commit else {
            return;
        };
        let entry_count = commit_msg.entries.len();
        match commit::handle(db, user, commit_msg).await {
            Ok(commit_resp) => {
                tracing::info!(email = %user.email, entry_count, "commit succeeded");
                resp.commit = Some(commit_resp);
            }
            Err(e) => {
                tracing::error!(email = %user.email, entry_count, "commit failed: {e}");
                resp.error_code = Some(sync_pb::sync_enums::ErrorType::TransientError as i32);
            }
        }
    } else if contents == Contents::GetUpdates as i32 {
        let Some(get_updates_msg) = msg.get_updates else {
            return;
        };
        let type_count = get_updates_msg.from_progress_marker.len();
        match get_updates::handle(db, user, get_updates_msg).await {
            Ok(gu_resp) => {
                let entries = gu_resp.entries.len();
                tracing::info!(email = %user.email, type_count, entries, "get_updates succeeded");
                resp.get_updates = Some(gu_resp);
            }
            Err(e) => {
                tracing::error!(email = %user.email, type_count, "get_updates failed: {e}");
                resp.error_code = Some(sync_pb::sync_enums::ErrorType::TransientError as i32);
            }
        }
    } else {
        tracing::warn!(email = %user.email, contents, "unhandled message type");
    }
}

/// `ClientCommand` and `NewBagOfChips` are populated on every response. The
/// reference convoy server and real Edge MSA both do this; Chromium-based
/// clients use them for sync scheduling and as a server-health signal.
fn default_client_command() -> sync_pb::ClientCommand {
    #[allow(deprecated)]
    sync_pb::ClientCommand {
        set_sync_poll_interval: Some(14400),
        set_sync_long_poll_interval: Some(21600),
        max_commit_batch_size: Some(100),
        ..Default::default()
    }
}

fn default_chip_bag() -> sync_pb::ChipBag {
    sync_pb::ChipBag {
        server_chips: Some(b"selfsync".to_vec()),
    }
}

fn birthday_mismatch(msg: &sync_pb::ClientToServerMessage, user: &user::Model) -> bool {
    msg.store_birthday
        .as_ref()
        .is_some_and(|b| !b.is_empty() && b != &user.store_birthday)
}

async fn find_or_create_user(
    db: &DatabaseConnection,
    identity: &ClientIdentity,
) -> anyhow::Result<user::Model> {
    if let Some(u) = user::Entity::find()
        .filter(user::Column::Email.eq(&identity.email))
        .filter(user::Column::BrowserKind.eq(identity.browser.as_db_str()))
        .one(db)
        .await?
    {
        return Ok(u);
    }

    // Match Microsoft Edge MSA sync server's wire format: it uses a fixed
    // sentinel string `"ProductionEnvironmentDefinition"` as the store
    // birthday for ALL users, not a per-user UUID. Edge clients (especially
    // the BookmarkDataTypeProcessor in Edge's MSA-aware fork) may validate
    // this exact value as part of their initial-merge sanity checks.
    // Chromium accepts any non-empty birthday so this is safe across both.
    let birthday = "ProductionEnvironmentDefinition".to_string();
    let enc_key = gen_encryption_key();
    let mut next_version = 1i64;

    let new_user = user::ActiveModel {
        email: Set(identity.email.clone()),
        browser_kind: Set(identity.browser.as_db_str().to_string()),
        store_birthday: Set(birthday),
        encryption_key: Set(enc_key.clone()),
        next_version: Set(next_version),
        ..Default::default()
    }
    .insert(db)
    .await?;
    tracing::info!(
        email = %identity.email,
        browser = %identity.browser,
        "created new user"
    );

    super::init::initialize_user_data(db, &new_user, &enc_key, &mut next_version).await?;

    let mut active: user::ActiveModel = new_user.into();
    active.next_version = Set(next_version);
    Ok(active.update(db).await?)
}

fn message_type_name(contents: i32) -> &'static str {
    use sync_pb::client_to_server_message::Contents;
    match Contents::try_from(contents) {
        Ok(Contents::Commit) => "COMMIT",
        Ok(Contents::GetUpdates) => "GET_UPDATES",
        Ok(Contents::ClearServerData) => "CLEAR_SERVER_DATA",
        _ => "UNKNOWN",
    }
}

fn error_response(
    birthday: &str,
    code: sync_pb::sync_enums::ErrorType,
) -> sync_pb::ClientToServerResponse {
    sync_pb::ClientToServerResponse {
        error_code: Some(code as i32),
        store_birthday: Some(birthday.to_string()),
        ..Default::default()
    }
}
