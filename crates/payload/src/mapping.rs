use std::collections::HashMap;
use std::fs;
use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// cache_guid → email 映射表
#[derive(Debug, Default)]
pub struct AccountMapping {
    map: HashMap<String, String>,
}

#[derive(Deserialize)]
struct Preferences {
    account_info: Option<Vec<AccountInfoEntry>>,
    google: Option<GoogleServices>,
    sync: Option<SyncData>,
}

#[derive(Deserialize)]
struct AccountInfoEntry {
    email: Option<String>,
    gaia: Option<String>,
}

#[derive(Deserialize)]
struct GoogleServices {
    services: Option<ServicesData>,
}

#[derive(Deserialize)]
struct ServicesData {
    last_username: Option<String>,
    last_signed_in_username: Option<String>,
    account_id: Option<String>,
}

#[derive(Deserialize)]
struct SyncData {
    transport_data_per_account: Option<HashMap<String, TransportData>>,
}

#[derive(Deserialize)]
struct TransportData {
    #[serde(rename = "sync.cache_guid")]
    cache_guid: Option<String>,
}

impl AccountMapping {
    /// 扫描所有 profile 目录，构建 cache_guid → email 映射
    pub fn build(user_data_dir: &str) -> Self {
        let mut mapping = AccountMapping::default();
        let base = Path::new(user_data_dir);

        // 扫描 Default + Profile N 目录
        let mut profile_dirs = vec![base.join("Default")];
        if let Ok(entries) = fs::read_dir(base) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with("Profile ") {
                    profile_dirs.push(entry.path());
                }
            }
        }

        for dir in &profile_dirs {
            if let Err(e) = mapping.load_profile(dir) {
                tracing::debug!("skipping {}: {e}", dir.display());
            }
        }

        mapping
    }

    fn load_profile(&mut self, profile_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let prefs_path = profile_dir.join("Preferences");
        let content = fs::read_to_string(&prefs_path)?;
        let prefs: Preferences = serde_json::from_str(&content)?;

        // 构建 gaia_id → email 映射
        let mut gaia_to_email: HashMap<String, String> = HashMap::new();

        if let Some(accounts) = &prefs.account_info {
            for acc in accounts {
                if let (Some(gaia), Some(email)) = (&acc.gaia, &acc.email) {
                    gaia_to_email.insert(gaia.clone(), email.clone());
                }
            }
        }

        // 从 google.services 补充（如果 account_info 没有对应的 email）
        if let Some(google) = &prefs.google
            && let Some(services) = &google.services
            && let Some(account_id) = &services.account_id
        {
            let email = services
                .last_username
                .as_deref()
                .or(services.last_signed_in_username.as_deref());
            if let Some(email) = email {
                gaia_to_email
                    .entry(account_id.clone())
                    .or_insert_with(|| email.to_string());
            }
        }

        // 遍历 transport_data_per_account，通过 gaia_id_hash 关联 cache_guid 和 email
        if let Some(sync) = &prefs.sync
            && let Some(transport) = &sync.transport_data_per_account
        {
            for (gaia_id_hash, data) in transport {
                if let Some(cache_guid) = &data.cache_guid {
                    let email = gaia_to_email.iter().find_map(|(gaia_id, email)| {
                        let hash = gaia_id_to_hash(gaia_id);
                        if &hash == gaia_id_hash {
                            Some(email.clone())
                        } else {
                            None
                        }
                    });

                    if let Some(email) = email {
                        let profile = profile_dir
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        tracing::info!(profile, cache_guid, email, "mapped account");
                        self.map.insert(cache_guid.clone(), email);
                    }
                }
            }
        }

        Ok(())
    }

    /// 通过 client_id (cache_guid) 查找 email
    pub fn lookup(&self, client_id: &str) -> Option<&str> {
        self.map.get(client_id).map(String::as_str)
    }
}

/// gaia_id → base64(sha256(gaia_id))
fn gaia_id_to_hash(gaia_id: &str) -> String {
    let hash = Sha256::digest(gaia_id.as_bytes());
    BASE64.encode(hash)
}
