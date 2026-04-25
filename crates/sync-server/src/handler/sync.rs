use axum::{Extension, body::Bytes, http::StatusCode, response::IntoResponse};
use prost::Message;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};

use crate::auth::DEFAULT_EMAIL;
use crate::db::entity::user;
use crate::proto::sync_pb;
use crate::util::gen_encryption_key;

use super::{commit, get_updates};

/// POST /command/ — handles all Chrome sync protocol messages.
pub async fn handle_command(
    Extension(db): Extension<DatabaseConnection>,
    body: Bytes,
) -> impl IntoResponse {
    let msg = match sync_pb::ClientToServerMessage::decode(body.as_ref()) {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("failed to decode ClientToServerMessage: {e}");
            return (StatusCode::BAD_REQUEST, Vec::new());
        }
    };

    // Email from protobuf `share` field. Chrome sends the signed-in account
    // email there, but Edge sends a base64 cache_guid (e.g.
    // "YgtTggJVuyTHat..."). When `share` is not an email, fall back to
    // DEFAULT_EMAIL so multiple Edge devices share one user record instead of
    // each minting a fresh Nigori per device GUID.
    let email = if msg.share.is_empty() || !is_email(&msg.share) {
        DEFAULT_EMAIL.to_string()
    } else {
        msg.share.clone()
    };

    let msg_type = message_type_name(msg.message_contents);
    tracing::info!(email, msg_type, body_len = body.len(), "sync request");

    let user = match find_or_create_user(&db, &email).await {
        Ok(u) => u,
        Err(e) => {
            tracing::error!(email, "user lookup failed: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, Vec::new());
        }
    };

    if !msg
        .store_birthday
        .as_ref()
        .is_none_or(|b| b.is_empty() || b == &user.store_birthday)
    {
        let resp = error_response(
            &user.store_birthday,
            sync_pb::sync_enums::ErrorType::NotMyBirthday,
        );
        return (StatusCode::OK, resp.encode_to_vec());
    }

    // Convoy / real Edge MSA populate ClientCommand and NewBagOfChips on every
    // response. Chromium-based clients use them for sync scheduling and as a
    // sanity check on server health.
    let client_command = sync_pb::ClientCommand {
        set_sync_poll_interval: Some(14400),
        max_commit_batch_size: Some(100),
        ..Default::default()
    };
    let new_bag_of_chips = sync_pb::ChipBag {
        server_chips: Some(b"selfsync".to_vec()),
    };

    let mut resp = sync_pb::ClientToServerResponse {
        error_code: Some(sync_pb::sync_enums::ErrorType::Success as i32),
        store_birthday: Some(user.store_birthday.clone()),
        client_command: Some(client_command),
        new_bag_of_chips: Some(new_bag_of_chips),
        ..Default::default()
    };

    let contents = msg.message_contents;

    if contents == sync_pb::client_to_server_message::Contents::Commit as i32 {
        if let Some(commit_msg) = msg.commit {
            let entry_count = commit_msg.entries.len();
            match commit::handle(&db, &user, commit_msg).await {
                Ok(commit_resp) => {
                    tracing::info!(email, entry_count, "commit succeeded");
                    resp.commit = Some(commit_resp);
                }
                Err(e) => {
                    tracing::error!(email, entry_count, "commit failed: {e}");
                    resp.error_code = Some(sync_pb::sync_enums::ErrorType::TransientError as i32);
                }
            }
        }
    } else if contents == sync_pb::client_to_server_message::Contents::GetUpdates as i32 {
        if let Some(get_updates_msg) = msg.get_updates {
            let type_count = get_updates_msg.from_progress_marker.len();
            match get_updates::handle(&db, &user, get_updates_msg).await {
                Ok(gu_resp) => {
                    let entries = gu_resp.entries.len();
                    tracing::info!(email, type_count, entries, "get_updates succeeded");
                    resp.get_updates = Some(gu_resp);
                }
                Err(e) => {
                    tracing::error!(email, type_count, "get_updates failed: {e}");
                    resp.error_code = Some(sync_pb::sync_enums::ErrorType::TransientError as i32);
                }
            }
        }
    } else {
        tracing::warn!(email, contents, "unhandled message type");
    }

    let resp_bytes = resp.encode_to_vec();
    tracing::debug!(
        email,
        msg_type,
        resp_len = resp_bytes.len(),
        "response sent"
    );
    (StatusCode::OK, resp_bytes)
}

async fn find_or_create_user(db: &DatabaseConnection, email: &str) -> anyhow::Result<user::Model> {
    if let Some(u) = user::Entity::find()
        .filter(user::Column::Email.eq(email))
        .one(db)
        .await?
    {
        return Ok(u);
    }

    // Match Microsoft Edge MSA sync server's wire format: it uses a fixed
    // sentinel string `"ProductionEnvironmentDefinition"` as the store
    // birthday for ALL users. Edge's BookmarkDataTypeProcessor may validate
    // this exact value as part of its initial-merge sanity checks. Chromium
    // clients accept any non-empty birthday so this is safe across both.
    let birthday = "ProductionEnvironmentDefinition".to_string();
    let enc_key = gen_encryption_key();
    let mut next_version = 1i64;

    let new_user = user::ActiveModel {
        email: Set(email.to_string()),
        store_birthday: Set(birthday),
        encryption_key: Set(enc_key.clone()),
        next_version: Set(next_version),
        ..Default::default()
    };
    let u = new_user.insert(db).await?;
    tracing::info!(email, "created new user");

    super::init::initialize_user_data(db, u.id, &enc_key, &mut next_version).await?;

    let mut active: user::ActiveModel = u.into();
    active.next_version = Set(next_version);
    let u = active.update(db).await?;

    Ok(u)
}

/// Returns true if `s` looks like an email: a single `@` with non-empty local
/// and domain parts, and a `.` in the domain. Cheap heuristic, no RFC.
fn is_email(s: &str) -> bool {
    let bytes = s.as_bytes();
    let Some(at) = bytes.iter().position(|&b| b == b'@') else {
        return false;
    };
    if at == 0 || at == bytes.len() - 1 {
        return false;
    }
    if bytes.iter().filter(|&&b| b == b'@').count() != 1 {
        return false;
    }
    bytes[at + 1..].contains(&b'.')
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
