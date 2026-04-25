//! Identity resolution for incoming sync requests.
//!
//! Each request is mapped to a `(email, browser)` pair. The pair — not email
//! alone — uniquely identifies a server-side user, because Edge and Chromium
//! cannot share a sync account: their cryptographers are incompatible (Edge
//! wraps Nigori with MSA-managed keys, Chromium uses server-issued keystore
//! keys) and they ship different permanent bookmark folders. Letting both
//! browsers land on the same row would corrupt sync state on each commit.

use axum::http::HeaderMap;

use crate::proto::sync_pb;

pub const DEFAULT_EMAIL: &str = "anonymous@localhost";

/// Browser variant inferred from request metadata. Drives per-user choices
/// during initialization (Nigori passphrase mode, permanent folder set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserKind {
    Chromium,
    Edge,
}

impl BrowserKind {
    /// Stable string used for the `users.browser_kind` column and as the
    /// `Display` representation. Changing these values is a breaking schema
    /// change.
    pub const fn as_db_str(self) -> &'static str {
        match self {
            BrowserKind::Chromium => "chromium",
            BrowserKind::Edge => "edge",
        }
    }

    /// Inverse of [`as_db_str`]. Unknown values fall back to `Chromium`
    /// since vanilla Chromium behavior is the safer default for any client
    /// we haven't explicitly profiled.
    pub fn from_db(s: &str) -> Self {
        match s {
            "edge" => BrowserKind::Edge,
            _ => BrowserKind::Chromium,
        }
    }
}

impl std::fmt::Display for BrowserKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_db_str())
    }
}

/// Identity derived from a single sync request: who the user is, and which
/// browser sent the request.
#[derive(Debug, Clone)]
pub struct ClientIdentity {
    pub email: String,
    pub browser: BrowserKind,
}

impl ClientIdentity {
    pub fn from_request(headers: &HeaderMap, msg: &sync_pb::ClientToServerMessage) -> Self {
        let browser = detect_browser(headers);
        let email = extract_email(&msg.share, browser);
        Self { email, browser }
    }
}

/// Edge sets `X-AFS-ClientInfo: app=Microsoft Edge; ver=...; ...`. Vanilla
/// Chromium and its derivatives (Brave, Vivaldi, Arc) omit the header.
fn detect_browser(headers: &HeaderMap) -> BrowserKind {
    let raw = headers
        .get("x-afs-clientinfo")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let app = raw.split(';').find_map(|part| {
        part.trim()
            .strip_prefix("app=")
            .map(|v| v.trim())
    });
    match app {
        Some(name) if name.eq_ignore_ascii_case("Microsoft Edge") => BrowserKind::Edge,
        _ => BrowserKind::Chromium,
    }
}

/// Pull a usable email out of `ClientToServerMessage.share`. Chromium puts
/// the signed-in account email there; Edge puts a base64 cache_guid which
/// would route every device to a fresh user record per install. Fall back to
/// `DEFAULT_EMAIL` so all Edge devices for one user collapse to one row.
fn extract_email(share: &str, browser: BrowserKind) -> String {
    if browser == BrowserKind::Edge || share.is_empty() || !looks_like_email(share) {
        DEFAULT_EMAIL.to_string()
    } else {
        share.to_string()
    }
}

/// Cheap structural check, not RFC validation: a single `@` with non-empty
/// local + domain parts and a `.` in the domain.
fn looks_like_email(s: &str) -> bool {
    let bytes = s.as_bytes();
    let Some(at) = bytes.iter().position(|&b| b == b'@') else {
        return false;
    };
    if at == 0 || at == bytes.len() - 1 {
        return false;
    }
    if bytes.iter().filter(|&&b| b == b'@').count() != 1 {
        return false;
    }
    bytes[at + 1..].contains(&b'.')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers_with_clientinfo(value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("x-afs-clientinfo", value.parse().unwrap());
        h
    }

    #[test]
    fn detects_edge_from_app_field() {
        let h = headers_with_clientinfo("app=Microsoft Edge; ver=140.0.0; cputype=x64");
        assert_eq!(detect_browser(&h), BrowserKind::Edge);
    }

    #[test]
    fn missing_header_is_chromium() {
        assert_eq!(detect_browser(&HeaderMap::new()), BrowserKind::Chromium);
    }

    #[test]
    fn unknown_app_is_chromium() {
        let h = headers_with_clientinfo("app=Brave; ver=1.0");
        assert_eq!(detect_browser(&h), BrowserKind::Chromium);
    }

    #[test]
    fn edge_share_falls_back_to_default() {
        // Edge's share is a base64 cache_guid, never an email.
        let email = extract_email("YgtTggJVuyTHat==", BrowserKind::Edge);
        assert_eq!(email, DEFAULT_EMAIL);
    }

    #[test]
    fn chromium_share_keeps_email() {
        let email = extract_email("alice@example.com", BrowserKind::Chromium);
        assert_eq!(email, "alice@example.com");
    }

    #[test]
    fn empty_share_falls_back_to_default() {
        assert_eq!(extract_email("", BrowserKind::Chromium), DEFAULT_EMAIL);
    }

    #[test]
    fn looks_like_email_rejects_obvious_non_emails() {
        assert!(looks_like_email("a@b.com"));
        assert!(!looks_like_email("nope"));
        assert!(!looks_like_email("@b.com"));
        assert!(!looks_like_email("a@b"));
        assert!(!looks_like_email("a@b@c.com"));
    }

    #[test]
    fn browser_kind_round_trips_through_db_column() {
        for k in [BrowserKind::Chromium, BrowserKind::Edge] {
            assert_eq!(BrowserKind::from_db(k.as_db_str()), k);
        }
        assert_eq!(BrowserKind::from_db("legacy_value"), BrowserKind::Chromium);
    }

    #[test]
    fn browser_kind_displays_as_db_string() {
        assert_eq!(BrowserKind::Edge.to_string(), "edge");
        assert_eq!(BrowserKind::Chromium.to_string(), "chromium");
    }
}
