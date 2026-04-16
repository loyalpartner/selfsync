use base64::Engine;
pub(crate) use base64::engine::general_purpose::STANDARD as BASE64;

/// Generate a server-assigned entity ID: base64(prefix + uuid).
pub(crate) fn gen_id(prefix: Option<&str>) -> String {
    let uuid = uuid::Uuid::new_v4().to_string();
    let raw = match prefix {
        Some(p) if p.len() >= 8 => format!("{}{}", &p[..8], uuid),
        Some(p) if !p.is_empty() => format!("{p}{uuid}"),
        _ => uuid,
    };
    BASE64.encode(raw.as_bytes())
}

/// Current time in milliseconds since Unix epoch.
pub(crate) fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_millis() as i64
}

/// Generate a random encryption key (32 bytes of randomness, base64-encoded).
pub(crate) fn gen_encryption_key() -> String {
    let mut bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rng(), &mut bytes);
    BASE64.encode(bytes)
}
