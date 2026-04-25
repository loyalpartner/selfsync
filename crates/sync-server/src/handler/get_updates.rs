use anyhow::Result;
use prost::Message;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, Order, QueryFilter, QueryOrder, QuerySelect,
};

use crate::db::entity::{sync_entity, user};
use crate::progress::Progress;
use crate::proto::sync_pb;

const PAGE_SIZE: u64 = 100;
const DATA_TYPE_NIGORI: i32 = 47745;

/// Handle a GET_UPDATES message: return entities newer than the client's progress tokens.
pub async fn handle(
    db: &DatabaseConnection,
    user: &user::Model,
    msg: sync_pb::GetUpdatesMessage,
) -> Result<sync_pb::GetUpdatesResponse> {
    let mut all_entries = Vec::new();
    let mut new_markers = Vec::new();
    let mut total_remaining: i64 = 0;

    for marker in &msg.from_progress_marker {
        let data_type_id = marker.data_type_id.unwrap_or(0);
        let token = marker.token.as_deref().unwrap_or(&[]);
        let progress = Progress::from_token(token, data_type_id);

        // Fetch batch_size+1 to detect if there are more
        let entities = sync_entity::Entity::find()
            .filter(sync_entity::Column::UserId.eq(user.id))
            .filter(sync_entity::Column::DataTypeId.eq(data_type_id))
            .filter(sync_entity::Column::Version.gt(progress.version))
            .order_by(sync_entity::Column::Version, Order::Asc)
            .limit(PAGE_SIZE + 1)
            .all(db)
            .await?;

        let has_more = entities.len() as u64 > PAGE_SIZE;
        let count = if has_more {
            PAGE_SIZE as usize
        } else {
            entities.len()
        };

        if has_more {
            total_remaining += 1;
        }

        let new_version = entities
            .get(count.saturating_sub(1))
            .map(|e| e.version)
            .unwrap_or(progress.version);

        new_markers.push(sync_pb::DataTypeProgressMarker {
            data_type_id: Some(data_type_id),
            token: Some(
                Progress {
                    data_type_id,
                    version: new_version,
                }
                .to_token(),
            ),
            ..Default::default()
        });

        for entity in entities.into_iter().take(count) {
            all_entries.push(db_entity_to_proto(entity));
        }
    }

    // Send keystore keys whenever the client subscribes to NIGORI. Chromium
    // loopback only sends them on NEW_CLIENT + need_encryption_key, but that
    // leaves clients stuck if they fetched the Nigori in an earlier round and
    // never got the keys (e.g. a GU_TRIGGER resume that never sets
    // need_encryption_key). The keys are tiny — sending on every NIGORI-bearing
    // request is the simplest fix that unblocks `cryptographer_has_pending_keys`.
    let client_subscribes_nigori = msg
        .from_progress_marker
        .iter()
        .any(|m| m.data_type_id == Some(DATA_TYPE_NIGORI));
    let need_key = msg.need_encryption_key.unwrap_or(false);
    let encryption_keys = if client_subscribes_nigori || need_key {
        vec![user.encryption_key.as_bytes().to_vec()]
    } else {
        vec![]
    };

    Ok(sync_pb::GetUpdatesResponse {
        entries: all_entries,
        new_progress_marker: new_markers,
        changes_remaining: Some(total_remaining),
        encryption_keys,
        ..Default::default()
    })
}

fn db_entity_to_proto(entity: sync_entity::Model) -> sync_pb::SyncEntity {
    let specifics = entity
        .specifics
        .as_ref()
        .and_then(|bytes| sync_pb::EntitySpecifics::decode(bytes.as_slice()).ok());

    let unique_position = entity
        .unique_position
        .as_ref()
        .and_then(|bytes| sync_pb::UniquePosition::decode(bytes.as_slice()).ok());

    sync_pb::SyncEntity {
        id_string: Some(entity.id_string),
        parent_id_string: entity.parent_id_string,
        version: Some(entity.version),
        ctime: entity.ctime,
        mtime: entity.mtime,
        name: entity.name,
        client_tag_hash: entity.client_tag,
        server_defined_unique_tag: entity.server_tag,
        specifics,
        unique_position,
        originator_cache_guid: entity.originator_cache_guid,
        originator_client_item_id: entity.originator_client_item_id,
        folder: Some(entity.folder),
        deleted: Some(entity.deleted),
        ..Default::default()
    }
}
