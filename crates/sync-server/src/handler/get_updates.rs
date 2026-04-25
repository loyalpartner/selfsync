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
    let mut response_has_keystore_nigori = false;

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
            let proto = db_entity_to_proto(entity);
            tracing::debug!(
                user_id = user.id,
                dtype = data_type_id,
                version = proto.version.unwrap_or(0),
                server_tag = proto.server_defined_unique_tag.as_deref().unwrap_or(""),
                id = proto.id_string.as_deref().unwrap_or(""),
                "get_updates returning entity"
            );
            if is_keystore_nigori(&proto) {
                response_has_keystore_nigori = true;
            }
            all_entries.push(proto);
        }
    }

    // Send keystore keys whenever the client is sync'ing the NIGORI data type.
    // Chromium loopback_server only sends them when the response includes a
    // KEYSTORE Nigori entity, but that leaves clients stuck if they fetched the
    // Nigori in an earlier round and never got the keys (e.g. Edge resuming with
    // a GU_TRIGGER instead of NEW_CLIENT, which never sets need_encryption_key).
    // The keys are tiny; sending them on every NIGORI-bearing request is the
    // simplest fix that unblocks `cryptographer_has_pending_keys=true`.
    let client_subscribes_nigori = msg
        .from_progress_marker
        .iter()
        .any(|m| m.data_type_id == Some(DATA_TYPE_NIGORI));
    let need_key = msg.need_encryption_key.unwrap_or(false);
    let encryption_keys = if response_has_keystore_nigori || need_key || client_subscribes_nigori {
        vec![user.encryption_key.as_bytes().to_vec()]
    } else {
        vec![]
    };

    tracing::debug!(
        user_id = user.id,
        response_has_keystore_nigori,
        need_key,
        client_subscribes_nigori,
        keys_sent = encryption_keys.len(),
        key_byte_len = encryption_keys.first().map(|k| k.len()).unwrap_or(0),
        "get_updates encryption_keys decision"
    );

    Ok(sync_pb::GetUpdatesResponse {
        entries: all_entries,
        new_progress_marker: new_markers,
        changes_remaining: Some(total_remaining),
        encryption_keys,
        ..Default::default()
    })
}

fn is_keystore_nigori(entity: &sync_pb::SyncEntity) -> bool {
    let Some(specifics) = entity.specifics.as_ref() else {
        return false;
    };
    let Some(sync_pb::entity_specifics::SpecificsVariant::Nigori(nigori)) =
        specifics.specifics_variant.as_ref()
    else {
        return false;
    };
    nigori.passphrase_type
        == Some(sync_pb::nigori_specifics::PassphraseType::KeystorePassphrase as i32)
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
