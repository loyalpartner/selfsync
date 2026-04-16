use std::collections::HashMap;

use anyhow::Result;
use prost::Message;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
    TransactionTrait,
};

use crate::db::entity::{sync_entity, user};
use crate::proto::sync_pb;
use crate::util::gen_id;

/// Handle a COMMIT message: create or update sync entities.
pub async fn handle(
    db: &DatabaseConnection,
    user: &user::Model,
    msg: sync_pb::CommitMessage,
) -> Result<sync_pb::CommitResponse> {
    let cache_guid = msg.cache_guid.as_deref().unwrap_or_default();
    let mut entry_responses = Vec::with_capacity(msg.entries.len());
    let mut id_map: HashMap<String, String> = HashMap::new();

    let txn = db.begin().await?;
    let mut current_version = user.next_version;

    for entry in &msg.entries {
        let version = entry.version.unwrap_or(0);
        let client_id = entry.id_string.clone().unwrap_or_default();

        let resp = if version == 0 {
            // CREATE new entity
            let server_id = gen_id(entry.name.as_deref());
            let parent_id = resolve_parent(&entry.parent_id_string, &id_map);

            let saved = build_new_entity(
                user.id,
                &server_id,
                parent_id,
                entry,
                cache_guid,
                &client_id,
                current_version,
            )
            .insert(&txn)
            .await?;

            id_map.insert(client_id, server_id.clone());
            current_version += 1;
            success_response(&server_id, saved.version)
        } else {
            // UPDATE existing entity
            let id_string = id_map
                .get(&client_id)
                .cloned()
                .unwrap_or_else(|| client_id.clone());

            let existing = sync_entity::Entity::find()
                .filter(sync_entity::Column::UserId.eq(user.id))
                .filter(sync_entity::Column::IdString.eq(&id_string))
                .one(&txn)
                .await?;

            match existing {
                Some(entity) if entity.version != version => {
                    // Version conflict
                    conflict_response(&id_string, entity.version)
                }
                Some(entity) => {
                    let parent_id = resolve_parent(&entry.parent_id_string, &id_map);
                    let mut active: sync_entity::ActiveModel = entity.into();
                    active.parent_id_string = Set(parent_id);
                    active.version = Set(current_version);
                    if entry.ctime.is_some() {
                        active.ctime = Set(entry.ctime);
                    }
                    active.mtime = Set(entry.mtime);
                    active.name = Set(entry.name.clone());
                    active.deleted = Set(entry.deleted.unwrap_or(false));
                    if entry.specifics.is_some() {
                        active.specifics = Set(entry.specifics.as_ref().map(|s| s.encode_to_vec()));
                    }
                    if entry.unique_position.is_some() {
                        active.unique_position =
                            Set(entry.unique_position.as_ref().map(|p| p.encode_to_vec()));
                    }

                    let saved = active.update(&txn).await?;
                    current_version += 1;
                    success_response(&saved.id_string, saved.version)
                }
                None => {
                    // Entity not found — treat as new
                    let server_id = gen_id(entry.name.as_deref());

                    let saved = build_new_entity(
                        user.id,
                        &server_id,
                        entry.parent_id_string.clone(),
                        entry,
                        cache_guid,
                        &id_string,
                        current_version,
                    )
                    .insert(&txn)
                    .await?;

                    id_map.insert(client_id, server_id.clone());
                    current_version += 1;
                    success_response(&server_id, saved.version)
                }
            }
        };

        entry_responses.push(resp);
    }

    // Update user's next_version counter
    let mut active_user: user::ActiveModel = user.clone().into();
    active_user.next_version = Set(current_version);
    active_user.update(&txn).await?;

    txn.commit().await?;

    Ok(sync_pb::CommitResponse {
        entryresponse: entry_responses,
    })
}

fn resolve_parent(parent_id: &Option<String>, id_map: &HashMap<String, String>) -> Option<String> {
    parent_id
        .as_ref()
        .map(|pid| id_map.get(pid).cloned().unwrap_or_else(|| pid.clone()))
}

fn build_new_entity(
    user_id: i32,
    server_id: &str,
    parent_id: Option<String>,
    entry: &sync_pb::SyncEntity,
    cache_guid: &str,
    originator_client_item_id: &str,
    version: i64,
) -> sync_entity::ActiveModel {
    sync_entity::ActiveModel {
        user_id: Set(user_id),
        id_string: Set(server_id.to_string()),
        parent_id_string: Set(parent_id),
        data_type_id: Set(extract_data_type_id(entry)),
        version: Set(version),
        ctime: Set(entry.ctime),
        mtime: Set(entry.mtime),
        name: Set(entry.name.clone()),
        client_tag: Set(entry.client_tag_hash.clone()),
        server_tag: Set(entry.server_defined_unique_tag.clone()),
        specifics: Set(entry.specifics.as_ref().map(|s| s.encode_to_vec())),
        unique_position: Set(entry.unique_position.as_ref().map(|p| p.encode_to_vec())),
        originator_cache_guid: Set(Some(cache_guid.to_string())),
        originator_client_item_id: Set(Some(originator_client_item_id.to_string())),
        folder: Set(entry.folder.unwrap_or(false)),
        deleted: Set(entry.deleted.unwrap_or(false)),
        ..Default::default()
    }
}

fn success_response(id_string: &str, version: i64) -> sync_pb::commit_response::EntryResponse {
    sync_pb::commit_response::EntryResponse {
        response_type: sync_pb::commit_response::ResponseType::Success as i32,
        id_string: Some(id_string.to_string()),
        version: Some(version),
        ..Default::default()
    }
}

fn conflict_response(id_string: &str, version: i64) -> sync_pb::commit_response::EntryResponse {
    sync_pb::commit_response::EntryResponse {
        response_type: sync_pb::commit_response::ResponseType::Conflict as i32,
        id_string: Some(id_string.to_string()),
        version: Some(version),
        ..Default::default()
    }
}

/// Extract data type ID from EntitySpecifics using the oneof variant's tag number.
fn extract_data_type_id(entry: &sync_pb::SyncEntity) -> i32 {
    let Some(specifics) = &entry.specifics else {
        return 0;
    };
    let Some(ref variant) = specifics.specifics_variant else {
        return 0;
    };

    use sync_pb::entity_specifics::SpecificsVariant;
    match variant {
        SpecificsVariant::Autofill(_) => 31729,
        SpecificsVariant::Bookmark(_) => 32904,
        SpecificsVariant::Preference(_) => 37702,
        SpecificsVariant::ThemeAndroid(_) => 1587331,
        SpecificsVariant::ThemeIos(_) => 1577986,
        SpecificsVariant::Theme(_) => 41210,
        SpecificsVariant::Password(_) => 45873,
        SpecificsVariant::Nigori(_) => 47745,
        SpecificsVariant::Extension(_) => 48119,
        SpecificsVariant::App(_) => 48364,
        SpecificsVariant::Session(_) => 50119,
        SpecificsVariant::AutofillProfile(_) => 63951,
        SpecificsVariant::SearchEngine(_) => 88610,
        SpecificsVariant::ExtensionSetting(_) => 96159,
        SpecificsVariant::AppSetting(_) => 103656,
        SpecificsVariant::HistoryDeleteDirective(_) => 150251,
        SpecificsVariant::DeviceInfo(_) => 154522,
        SpecificsVariant::PriorityPreference(_) => 163425,
        SpecificsVariant::Dictionary(_) => 170540,
        SpecificsVariant::ManagedUserSetting(_) => 186662,
        SpecificsVariant::AppList(_) => 229170,
        SpecificsVariant::AutofillWallet(_) => 306270,
        SpecificsVariant::WalletMetadata(_) => 330441,
        SpecificsVariant::ArcPackage(_) => 340906,
        SpecificsVariant::Printer(_) => 410745,
        SpecificsVariant::ReadingList(_) => 411028,
        SpecificsVariant::UserEvent(_) => 455206,
        SpecificsVariant::UserConsent(_) => 556014,
        SpecificsVariant::SendTabToSelf(_) => 601980,
        SpecificsVariant::SecurityEvent(_) => 600372,
        SpecificsVariant::WebApk(_) => 1117170,
        SpecificsVariant::WebApp(_) => 673225,
        SpecificsVariant::WifiConfiguration(_) => 662827,
        SpecificsVariant::OsPreference(_) => 702141,
        SpecificsVariant::OsPriorityPreference(_) => 703915,
        SpecificsVariant::SharingMessage(_) => 728866,
        SpecificsVariant::AutofillOffer(_) => 774329,
        SpecificsVariant::WorkspaceDesk(_) => 874841,
        SpecificsVariant::WebauthnCredential(_) => 895275,
        SpecificsVariant::History(_) => 963985,
        _ => 0,
    }
}
