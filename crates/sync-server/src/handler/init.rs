use anyhow::Result;
use prost::Message;
use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};

use crate::auth::BrowserKind;
use crate::db::entity::{sync_entity, user};
use crate::proto::sync_pb;
use crate::util::{BASE64, now_millis};

const DATA_TYPE_NIGORI: i32 = 47745;
const DATA_TYPE_BOOKMARK: i32 = 32904;

/// Permanent bookmark folders, in stable order. Edge gets one extra entry
/// (`workspace_bookmarks`); its BookmarkDataTypeProcessor enumerates that
/// tag at initial-merge time and treats its absence as
/// `kBookmarksInitialMergePermanentEntitiesMissing` (the Type 64 trap).
/// Chromium ignores unknown server-defined permanent tags, so the extra
/// folder is harmless even if a non-Edge browser somehow lands here — but
/// we still gate it on `BrowserKind::Edge` to keep each browser's DB clean.
const STANDARD_BOOKMARK_FOLDERS: &[(&str, &str)] = &[
    ("bookmark_bar", "Bookmark Bar"),
    ("other_bookmarks", "Other Bookmarks"),
    ("synced_bookmarks", "Synced Bookmarks"),
];
const EDGE_EXTRA_BOOKMARK_FOLDER: (&str, &str) = ("workspace_bookmarks", "Workspaces");

/// Seed permanent entities for a freshly-created user.
pub async fn initialize_user_data(
    db: &DatabaseConnection,
    user: &user::Model,
    encryption_key: &str,
    version: &mut i64,
) -> Result<()> {
    let browser = user.browser();
    create_nigori_node(db, user.id, version, encryption_key, browser).await?;
    create_bookmark_permanent_folders(db, user.id, version, browser).await?;
    tracing::info!(
        user_id = user.id,
        %browser,
        "initialized user data (nigori + bookmarks)"
    );
    Ok(())
}

async fn create_bookmark_permanent_folders(
    db: &DatabaseConnection,
    user_id: i32,
    version: &mut i64,
    browser: BrowserKind,
) -> Result<()> {
    let root_id = create_permanent_bookmark(
        db,
        user_id,
        version,
        "google_chrome_bookmarks",
        "Bookmarks",
        "0",
    )
    .await?;
    for (tag, name) in STANDARD_BOOKMARK_FOLDERS {
        create_permanent_bookmark(db, user_id, version, tag, name, &root_id).await?;
    }
    if browser == BrowserKind::Edge {
        let (tag, name) = EDGE_EXTRA_BOOKMARK_FOLDER;
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
    // Chromium loopback's `PersistentPermanentEntity` id format:
    //   "{datatype_field_number}|{server_tag}"
    // e.g. "32904|google_chrome_bookmarks", "32904|bookmark_bar".
    let id_string = format!("{DATA_TYPE_BOOKMARK}|{tag}");
    let now = now_millis();

    let specifics = sync_pb::EntitySpecifics {
        specifics_variant: Some(sync_pb::entity_specifics::SpecificsVariant::Bookmark(
            sync_pb::BookmarkSpecifics::default(),
        )),
        ..Default::default()
    };

    sync_entity::ActiveModel {
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
    }
    .insert(db)
    .await?;
    *version += 1;
    Ok(id_string)
}

async fn create_nigori_node(
    db: &DatabaseConnection,
    user_id: i32,
    version: &mut i64,
    encryption_key: &str,
    browser: BrowserKind,
) -> Result<()> {
    // Loopback-style id: "{datatype_field_number}|google_chrome_nigori".
    let id_string = format!("{DATA_TYPE_NIGORI}|google_chrome_nigori");
    let now = now_millis();

    let specifics = sync_pb::EntitySpecifics {
        specifics_variant: Some(sync_pb::entity_specifics::SpecificsVariant::Nigori(
            build_nigori_for(browser, encryption_key, now)?,
        )),
        ..Default::default()
    };

    sync_entity::ActiveModel {
        user_id: Set(user_id),
        id_string: Set(id_string),
        // Chromium loopback uses empty string for top-level Nigori parent.
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
    }
    .insert(db)
    .await?;
    *version += 1;
    Ok(())
}

/// Pick the Nigori shape that the browser's cryptographer will accept.
///
/// - Edge uses MSA-managed keystore keys and rejects any server-built
///   keybag (cryptographer stays pending forever). Emit an empty
///   IMPLICIT_PASSPHRASE Nigori so Edge enters Chromium's
///   client-initialization fall-through in
///   `nigori_sync_bridge_impl.cc::MergeFullSyncData` and supplies its own
///   keybag derived from MSA keys.
/// - Chromium-family browsers accept the standard KEYSTORE_PASSPHRASE flow
///   with the server-issued encryption key.
fn build_nigori_for(
    browser: BrowserKind,
    encryption_key: &str,
    now: i64,
) -> Result<sync_pb::NigoriSpecifics> {
    match browser {
        BrowserKind::Edge => Ok(empty_implicit_nigori()),
        BrowserKind::Chromium => keystore_nigori(encryption_key, now),
    }
}

/// Empty IMPLICIT_PASSPHRASE NigoriSpecifics. Triggers Chromium's client-side
/// init path: the client uses its own keystore_keys to derive a fresh keybag
/// and commits it back. See `MergeFullSyncData` fall-through.
fn empty_implicit_nigori() -> sync_pb::NigoriSpecifics {
    sync_pb::NigoriSpecifics {
        encryption_keybag: Some(sync_pb::EncryptedData {
            key_name: Some(String::new()),
            blob: Some(String::new()),
        }),
        passphrase_type: Some(sync_pb::nigori_specifics::PassphraseType::ImplicitPassphrase as i32),
        ..Default::default()
    }
}

/// NigoriSpecifics with keystore passphrase encryption derived from the
/// server-issued key.
fn keystore_nigori(encryption_key: &str, now: i64) -> Result<sync_pb::NigoriSpecifics> {
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
        // CRITICAL: encrypt_everything=false. Real MSA wire uses false, and
        // selfsync sends plaintext BookmarkSpecifics — setting true here is a
        // protocol contradiction (says everything is encrypted, but bookmark
        // entities are sent in plaintext) that breaks BookmarkDataTypeProcessor
        // initial merge in some clients.
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
