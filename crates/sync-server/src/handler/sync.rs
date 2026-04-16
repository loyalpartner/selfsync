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

    // Email from protobuf `share` field (Chrome sends the signed-in account email).
    let email = if msg.share.is_empty() {
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

    // Validate store_birthday
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

    let mut resp = sync_pb::ClientToServerResponse {
        error_code: Some(sync_pb::sync_enums::ErrorType::Success as i32),
        store_birthday: Some(user.store_birthday.clone()),
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

    let prefix = (b'a' + uuid::Uuid::new_v4().as_bytes()[0] % 26) as char;
    let birthday = format!("{prefix}{}", uuid::Uuid::new_v4());
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

    // Initialize permanent entities (nigori, bookmarks)
    super::init::initialize_user_data(db, u.id, &enc_key, &mut next_version).await?;

    // Update next_version after entity initialization
    let mut active: user::ActiveModel = u.into();
    active.next_version = Set(next_version);
    let u = active.update(db).await?;

    Ok(u)
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
