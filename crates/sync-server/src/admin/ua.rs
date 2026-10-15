//! Turn the sync engine's User-Agent into a short label.
//!
//! Chromium's sync requests do not send the web User-Agent. They send
//! `<product> <SYSTEM> <version> (<last change>) channel(<channel>)`, built by
//! `FormatUserAgentForSync()` in `components/sync/base/sync_util.cc`, e.g.
//! `Chrome WIN 141.0.7390.54 (abc-refs/branch-heads/7390@{#1}) channel(stable)`.
//! The system token is a family only — there is no OS version.

/// Longest label shown for a User-Agent we can't parse.
const MAX_RAW_LEN: usize = 60;

/// `Chrome 141 · Windows`, or a truncated raw string when the format is not
/// the sync-engine one (other forks, non-browser clients).
pub fn label(user_agent: &str) -> String {
    if let Some(parsed) = parse(user_agent) {
        return parsed;
    }
    let raw = user_agent.trim();
    if raw.chars().count() <= MAX_RAW_LEN {
        raw.to_string()
    } else {
        let head: String = raw.chars().take(MAX_RAW_LEN).collect();
        format!("{head}…")
    }
}

fn parse(user_agent: &str) -> Option<String> {
    let mut parts = user_agent.split_whitespace();
    let product = parts.next()?;
    let system = system_name(parts.next()?)?;
    let major = parts.next()?.split('.').next()?;
    if major.is_empty() || !major.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(format!("{product} {major} · {system}"))
}

fn system_name(token: &str) -> Option<&'static str> {
    Some(match token {
        "WIN" => "Windows",
        "MAC" => "macOS",
        "LINUX" => "Linux",
        "CROS" => "ChromeOS",
        "FREEBSD" => "FreeBSD",
        "OPENBSD" => "OpenBSD",
        "ANDROID" | "ANDROID-PHONE" => "Android",
        "ANDROID-TABLET" => "Android tablet",
        "ANDROID-DESKTOP" => "Android desktop",
        "IOS-PHONE" => "iOS",
        "IOS-TABLET" => "iPadOS",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sync_engine_format() {
        assert_eq!(
            label("Chrome WIN 141.0.7390.54 (abc-refs/branch-heads/7390@{#1}) channel(stable)"),
            "Chrome 141 · Windows"
        );
        assert_eq!(
            label("Chromium LINUX 147.0.7727.116 (deadbeef)-devel"),
            "Chromium 147 · Linux"
        );
        assert_eq!(
            label("Chrome ANDROID-PHONE 140.0.1.2 (x) channel(stable)"),
            "Chrome 140 · Android"
        );
    }

    #[test]
    fn falls_back_to_truncated_raw() {
        assert_eq!(label("curl/8.4.0"), "curl/8.4.0");
        assert_eq!(label(""), "");
        let long = "x".repeat(100);
        assert_eq!(label(&long).chars().count(), MAX_RAW_LEN + 1);
    }
}
