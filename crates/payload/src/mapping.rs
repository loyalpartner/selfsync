use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// 懒加载的 cache_guid → email 映射。
/// 收到请求时按需扫描 Preferences，命中后缓存。
pub struct AccountMapping {
    user_data_dir: PathBuf,
    cache: Mutex<HashMap<String, String>>,
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
    pub fn new(user_data_dir: &str) -> Self {
        Self {
            user_data_dir: PathBuf::from(user_data_dir),
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// 通过 client_id (cache_guid) 查找 email。
    /// 先查缓存，miss 时扫描所有 profile 的 Preferences 文件。
    pub fn lookup(&self, client_id: &str) -> Option<String> {
        // 1. 查缓存
        if let Some(email) = self.cache.lock().unwrap().get(client_id) {
            return Some(email.clone());
        }

        // 2. cache miss — 扫描所有 profile
        let found = self.scan_profiles(client_id);

        // 3. 命中则缓存
        if let Some(ref email) = found {
            self.cache
                .lock()
                .unwrap()
                .insert(client_id.to_string(), email.clone());
            tracing::info!(client_id, email, "resolved and cached account mapping");
        }

        found
    }

    fn scan_profiles(&self, target_cache_guid: &str) -> Option<String> {
        let profile_dirs = self.list_profile_dirs();

        for dir in &profile_dirs {
            if let Some(email) = self.find_email_in_profile(dir, target_cache_guid) {
                return Some(email);
            }
        }
        None
    }

    fn list_profile_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = vec![self.user_data_dir.join("Default")];
        if let Ok(entries) = fs::read_dir(&self.user_data_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if name.to_string_lossy().starts_with("Profile ") {
                    dirs.push(entry.path());
                }
            }
        }
        dirs
    }

    fn find_email_in_profile(&self, profile_dir: &Path, target_cache_guid: &str) -> Option<String> {
        let content = fs::read_to_string(profile_dir.join("Preferences")).ok()?;
        let prefs: Preferences = serde_json::from_str(&content).ok()?;

        // gaia_id → email
        let mut gaia_to_email: HashMap<String, String> = HashMap::new();

        if let Some(accounts) = &prefs.account_info {
            for acc in accounts {
                if let (Some(gaia), Some(email)) = (&acc.gaia, &acc.email) {
                    gaia_to_email.insert(gaia.clone(), email.clone());
                }
            }
        }

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

        // 在 transport_data_per_account 中找 target_cache_guid
        let transport = prefs.sync?.transport_data_per_account?;
        for (gaia_id_hash, data) in &transport {
            if data.cache_guid.as_deref() == Some(target_cache_guid) {
                // 反查 gaia_id_hash → gaia_id → email
                let email = gaia_to_email.iter().find_map(|(gaia_id, email)| {
                    if gaia_id_to_hash(gaia_id) == *gaia_id_hash {
                        Some(email.clone())
                    } else {
                        None
                    }
                });
                return email;
            }
        }

        None
    }
}

impl std::fmt::Debug for AccountMapping {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let cached = self.cache.lock().unwrap().len();
        f.debug_struct("AccountMapping")
            .field("user_data_dir", &self.user_data_dir)
            .field("cached_entries", &cached)
            .finish()
    }
}

fn gaia_id_to_hash(gaia_id: &str) -> String {
    let hash = Sha256::digest(gaia_id.as_bytes());
    BASE64.encode(hash)
}
