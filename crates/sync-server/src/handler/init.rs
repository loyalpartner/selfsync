use anyhow::Result;
use prost::Message;
use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};

use crate::db::entity::sync_entity;
use crate::proto::sync_pb;
use crate::util::{BASE64, now_millis};

const DATA_TYPE_NIGORI: i32 = 47745;
const DATA_TYPE_BOOKMARK: i32 = 32904;

/// Initialize permanent entities for a new user: Nigori + bookmark permanent
/// folders.
pub async fn initialize_user_data(
    db: &DatabaseConnection,
    user_id: i32,
    encryption_key: &str,
    version: &mut i64,
) -> Result<()> {
    create_nigori_node(db, user_id, version, encryption_key).await?;
    create_bookmark_permanent_folders(db, user_id, version).await?;
    tracing::info!(user_id, "initialized user data (nigori + bookmarks)");
    Ok(())
}

async fn create_bookmark_permanent_folders(
    db: &DatabaseConnection,
    user_id: i32,
    version: &mut i64,
) -> Result<()> {
    // Chromium loopback id format: "{datatype_field_number}|{server_tag}".
    let root_id = create_permanent_bookmark(
        db,
        user_id,
        version,
        "google_chrome_bookmarks",
        "Bookmarks",
        "0",
    )
    .await?;
    // Standard Chromium permanent bookmark folders. Edge additionally expects a
    // 5th permanent folder `workspace_bookmarks` ("Workspaces") under the root —
    // its BookmarkDataTypeProcessor enumerates that tag at initial-merge time
    // and treats its absence as
    // `kBookmarksInitialMergePermanentEntitiesMissing` (Type 64). Always
    // including it is safe: Chromium ignores unknown server-defined permanent
    // tags rather than rejecting them.
    for (tag, name) in &[
        ("bookmark_bar", "Bookmark Bar"),
        ("other_bookmarks", "Other Bookmarks"),
        ("synced_bookmarks", "Synced Bookmarks"),
        ("workspace_bookmarks", "Workspaces"),
    ] {
        create_permanent_bookmark(db, user_id, version, tag, name, &root_id).await?;
    }
    Ok(())
}

async fn create_permanent_bookmark(
    db: &DatabaseConnection,
    user_id: i32,
    version: &mut i64,
    tag: &str,
    name: &str,
    parent_id: &str,
) -> Result<String> {
    let id_string = format!("{DATA_TYPE_BOOKMARK}|{tag}");
    let now = now_millis();

    let specifics = sync_pb::EntitySpecifics {
        specifics_variant: Some(sync_pb::entity_specifics::SpecificsVariant::Bookmark(
            sync_pb::BookmarkSpecifics::default(),
        )),
        ..Default::default()
    };

    let entity = sync_entity::ActiveModel {
        user_id: Set(user_id),
        id_string: Set(id_string.clone()),
        parent_id_string: Set(Some(parent_id.to_string())),
        data_type_id: Set(DATA_TYPE_BOOKMARK),
        version: Set(*version),
        ctime: Set(Some(now)),
        mtime: Set(Some(now)),
        name: Set(Some(name.to_string())),
        server_tag: Set(Some(tag.to_string())),
        specifics: Set(Some(specifics.encode_to_vec())),
        folder: Set(true),
        deleted: Set(false),
        ..Default::default()
    };
    entity.insert(db).await?;
    *version += 1;
    Ok(id_string)
}

async fn create_nigori_node(
    db: &DatabaseConnection,
    user_id: i32,
    version: &mut i64,
    encryption_key: &str,
) -> Result<()> {
    let id_string = format!("{DATA_TYPE_NIGORI}|google_chrome_nigori");
    let now = now_millis();

    let nigori_specifics = build_nigori_specifics(encryption_key, now)?;
    let specifics = sync_pb::EntitySpecifics {
        specifics_variant: Some(sync_pb::entity_specifics::SpecificsVariant::Nigori(
            nigori_specifics,
        )),
        ..Default::default()
    };

    let entity = sync_entity::ActiveModel {
        user_id: Set(user_id),
        id_string: Set(id_string),
        parent_id_string: Set(Some(String::new())),
        data_type_id: Set(DATA_TYPE_NIGORI),
        version: Set(*version),
        ctime: Set(Some(now)),
        mtime: Set(Some(now)),
        name: Set(Some("Nigori".to_string())),
        server_tag: Set(Some("google_chrome_nigori".to_string())),
        specifics: Set(Some(specifics.encode_to_vec())),
        folder: Set(true),
        deleted: Set(false),
        ..Default::default()
    };
    entity.insert(db).await?;
    *version += 1;
    Ok(())
}

/// Build NigoriSpecifics with keystore passphrase encryption.
fn build_nigori_specifics(encryption_key: &str, now: i64) -> Result<sync_pb::NigoriSpecifics> {
    use base64::Engine;
    use selfsync_nigori::{KeyDerivationParams, Nigori};

    let passphrase = BASE64.encode(encryption_key.as_bytes());
    let nigori = Nigori::create_by_derivation(&KeyDerivationParams::pbkdf2(), &passphrase)?;

    let key_name = nigori.get_key_name();
    let (user_key_opt, enc_key, mac_key) = nigori.export_keys();

    #[allow(deprecated)]
    let nigori_key = sync_pb::NigoriKey {
        deprecated_name: Some(key_name.clone()),
        deprecated_user_key: user_key_opt.map(|k| k.to_vec()),
        encryption_key: Some(enc_key.to_vec()),
        mac_key: Some(mac_key.to_vec()),
    };

    let keybag = sync_pb::NigoriKeyBag {
        key: vec![nigori_key.clone()],
    };
    let encrypted_keybag = nigori.encrypt(&keybag.encode_to_vec());

    #[allow(deprecated)]
    let mut decryptor_key = nigori_key.clone();
    #[allow(deprecated)]
    {
        decryptor_key.deprecated_name = None;
    }
    let encrypted_decryptor = nigori.encrypt(&decryptor_key.encode_to_vec());

    Ok(sync_pb::NigoriSpecifics {
        encryption_keybag: Some(sync_pb::EncryptedData {
            key_name: Some(key_name.clone()),
            blob: Some(encrypted_keybag),
        }),
        keybag_is_frozen: Some(true),
        // CRITICAL: encrypt_everything=false. selfsync sends plaintext
        // BookmarkSpecifics; setting true here is a protocol contradiction
        // (claims everything is encrypted while bookmarks ship as plaintext)
        // that breaks BookmarkDataTypeProcessor initial merge in some clients
        // (Edge in particular).
        encrypt_everything: Some(false),
        passphrase_type: Some(sync_pb::nigori_specifics::PassphraseType::KeystorePassphrase as i32),
        keystore_decryptor_token: Some(sync_pb::EncryptedData {
            key_name: Some(key_name),
            blob: Some(encrypted_decryptor),
        }),
        keystore_migration_time: Some(now),
        ..Default::default()
    })
}
