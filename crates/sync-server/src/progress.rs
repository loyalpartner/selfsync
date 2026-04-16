use crate::util::BASE64;
use base64::Engine;

/// Progress token tracks the last-seen version for a data type.
///
/// Format: `v1,{data_type_id},{version}` base64-encoded.
pub struct Progress {
    pub data_type_id: i32,
    pub version: i64,
}

impl Progress {
    pub fn from_token(token: &[u8], data_type_id: i32) -> Self {
        if token.is_empty() {
            return Self {
                data_type_id,
                version: 0,
            };
        }

        let Ok(decoded) = BASE64.decode(token) else {
            tracing::debug!("invalid base64 in progress token");
            return Self {
                data_type_id,
                version: 0,
            };
        };
        let Ok(s) = String::from_utf8(decoded) else {
            tracing::debug!("invalid UTF-8 in progress token");
            return Self {
                data_type_id,
                version: 0,
            };
        };

        if let Some(rest) = s.strip_prefix("v1,") {
            let mut parts = rest.splitn(3, ',');
            let dtype = parts
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(data_type_id);
            let version = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            return Self {
                data_type_id: dtype,
                version,
            };
        }

        Self {
            data_type_id,
            version: 0,
        }
    }

    pub fn to_token(&self) -> Vec<u8> {
        let payload = format!("v1,{},{}", self.data_type_id, self.version);
        BASE64.encode(payload.as_bytes()).into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let p = Progress {
            data_type_id: 47745,
            version: 42,
        };
        let token = p.to_token();
        let p2 = Progress::from_token(&token, 0);
        assert_eq!(p2.data_type_id, 47745);
        assert_eq!(p2.version, 42);
    }

    #[test]
    fn empty_token_defaults_to_zero() {
        let p = Progress::from_token(&[], 32904);
        assert_eq!(p.data_type_id, 32904);
        assert_eq!(p.version, 0);
    }
}
