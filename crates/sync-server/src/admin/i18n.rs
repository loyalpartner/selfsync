//! Admin page copy in Chinese and English.
//!
//! Both languages are `const` instances of one struct, so a string missing
//! from either fails to compile instead of leaking a key onto the page.

use axum::http::HeaderMap;

pub const LANG_COOKIE: &str = "selfsync_lang";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "zh" | "zh-cn" => Some(Lang::Zh),
            "en" => Some(Lang::En),
            _ => None,
        }
    }

    pub const fn code(self) -> &'static str {
        match self {
            Lang::Zh => "zh",
            Lang::En => "en",
        }
    }

    pub const fn texts(self) -> &'static Texts {
        match self {
            Lang::Zh => &ZH,
            Lang::En => &EN,
        }
    }

    /// Pick the page language: explicit `?lang=`, then the cookie, then
    /// `Accept-Language`, then English.
    pub fn negotiate(query: Option<&str>, headers: &HeaderMap) -> Self {
        if let Some(lang) = query.and_then(Lang::parse) {
            return lang;
        }
        if let Some(lang) = cookie(headers, LANG_COOKIE).and_then(Lang::parse) {
            return lang;
        }
        let accept = headers
            .get("accept-language")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        // Honor the first listed language only; good enough for two choices.
        let first = accept.split(',').next().unwrap_or("").trim();
        if first.to_ascii_lowercase().starts_with("zh") {
            Lang::Zh
        } else {
            Lang::En
        }
    }
}

fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all("cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
}

pub struct Texts {
    pub html_lang: &'static str,
    pub page_title: &'static str,
    pub admin: &'static str,
    pub admin_port: &'static str,
    pub local_only: &'static str,
    pub exposed: &'static str,
    pub language: &'static str,

    pub approval_title: &'static str,
    pub approval_desc: &'static str,
    pub status_on: &'static str,
    pub status_off: &'static str,

    pub pending_title: &'static str,
    pub pending_hint: &'static str,
    pub off_note: &'static str,
    pub no_pending: &'static str,
    pub ip: &'static str,
    pub first_seen: &'static str,
    pub last_attempt: &'static str,
    pub device_id: &'static str,

    pub approved_title: &'static str,
    pub rejected_title: &'static str,
    pub rejected_hint: &'static str,
    pub no_approved: &'static str,
    pub col_device: &'static str,
    pub col_account: &'static str,
    pub col_last_sync: &'static str,
    pub col_actions: &'static str,
    pub name_after_sync: &'static str,

    pub approve: &'static str,
    pub reject: &'static str,
    pub revoke: &'static str,
    pub re_approve: &'static str,

    pub users_title: &'static str,
    pub no_users: &'static str,
    pub col_browser: &'static str,
    pub col_approved_devices: &'static str,
    pub col_data_version: &'static str,

    pub footer: &'static str,

    pub done_approved: &'static str,
    pub done_rejected: &'static str,
    pub done_on: &'static str,
    pub done_off: &'static str,
}

impl Texts {
    /// Relative time for "last seen" columns. `then` and `now` are Unix
    /// milliseconds.
    pub fn ago(&self, now: i64, then: i64) -> String {
        let secs = (now - then).max(0) / 1000;
        let (n, zh, en) = match secs {
            0..60 => return self.just_now().to_string(),
            60..3600 => (secs / 60, "分钟前", "min ago"),
            3600..86400 => (secs / 3600, "小时前", "h ago"),
            _ => (secs / 86400, "天前", "days ago"),
        };
        match self.html_lang {
            "zh-CN" => format!("{n} {zh}"),
            _ if n == 1 && en == "days ago" => "1 day ago".to_string(),
            _ => format!("{n} {en}"),
        }
    }

    fn just_now(&self) -> &'static str {
        if self.html_lang == "zh-CN" {
            "刚刚"
        } else {
            "just now"
        }
    }
}

pub const ZH: Texts = Texts {
    html_lang: "zh-CN",
    page_title: "selfsync 管理",
    admin: "管理",
    admin_port: "管理端口",
    local_only: "仅本机可访问",
    exposed: "已对外监听，请在反向代理上加认证",
    language: "界面语言",

    approval_title: "新设备需要审核",
    approval_desc: "打开后，没批准过的浏览器无法同步，会出现在下方「待审核」里，等你批准。关闭时，新设备连上来会自动批准。",
    status_on: "已开启 · 新设备批准后才能同步",
    status_off: "已关闭 · 新设备自动批准",

    pending_title: "待审核",
    pending_hint: "批准后，浏览器下次重试时自动恢复同步，一般几分钟内，不用重启浏览器。",
    off_note: "审核已关闭：下面这些设备下次连接时会被自动批准。",
    no_pending: "没有待审核的设备。",
    ip: "来源 IP",
    first_seen: "首次出现",
    last_attempt: "最近一次尝试",
    device_id: "设备 ID",

    approved_title: "已批准设备",
    rejected_title: "已拒绝",
    rejected_hint: "这些设备会一直被拒绝，开关关闭也不会放行。",
    no_approved: "还没有已批准的设备。",
    col_device: "设备",
    col_account: "账号",
    col_last_sync: "最近同步",
    col_actions: "操作",
    name_after_sync: "设备名在首次同步后显示",

    approve: "批准",
    reject: "拒绝",
    revoke: "撤销",
    re_approve: "改为批准",

    users_title: "账号",
    no_users: "还没有账号。",
    col_browser: "浏览器",
    col_approved_devices: "已批准设备",
    col_data_version: "数据版本",

    footer: "设备 ID 由浏览器自己生成。在浏览器里退出账号、关掉再打开同步、或换一个浏览器配置文件后，设备 ID 会变，同一台电脑会以新设备的身份重新出现在「待审核」里。",

    done_approved: "已批准",
    done_rejected: "已拒绝",
    done_on: "审核已打开，新设备需要批准",
    done_off: "审核已关闭，新设备会自动批准",
};

pub const EN: Texts = Texts {
    html_lang: "en",
    page_title: "selfsync admin",
    admin: "Admin",
    admin_port: "Admin port",
    local_only: "local access only",
    exposed: "listening publicly; put authentication in front of it",
    language: "Language",

    approval_title: "Require approval for new devices",
    approval_desc: "When on, browsers you haven't approved can't sync. They show up under Pending below until you approve them. When off, new devices are approved automatically.",
    status_on: "On · new devices sync only after approval",
    status_off: "Off · new devices are approved automatically",

    pending_title: "Pending",
    pending_hint: "Once approved, the browser resumes on its next retry, usually within a few minutes. No restart needed.",
    off_note: "Approval is off: these devices will be approved the next time they connect.",
    no_pending: "No devices waiting for approval.",
    ip: "IP",
    first_seen: "First seen",
    last_attempt: "Last attempt",
    device_id: "Device ID",

    approved_title: "Approved devices",
    rejected_title: "Rejected",
    rejected_hint: "These devices stay blocked, even when approval is turned off.",
    no_approved: "No approved devices yet.",
    col_device: "Device",
    col_account: "Account",
    col_last_sync: "Last sync",
    col_actions: "Actions",
    name_after_sync: "Device name appears after its first sync",

    approve: "Approve",
    reject: "Reject",
    revoke: "Revoke",
    re_approve: "Approve",

    users_title: "Accounts",
    no_users: "No accounts yet.",
    col_browser: "Browser",
    col_approved_devices: "Approved devices",
    col_data_version: "Data version",

    footer: "Device IDs are generated by the browser. Signing out, turning sync off and on, or switching browser profiles gives the same computer a new ID, and it shows up under Pending again.",

    done_approved: "Approved",
    done_rejected: "Rejected",
    done_on: "Approval is on; new devices need approval",
    done_off: "Approval is off; new devices are approved automatically",
};

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.append(*k, v.parse().unwrap());
        }
        h
    }

    #[test]
    fn query_beats_cookie_beats_accept_language() {
        let h = headers(&[
            ("cookie", "a=1; selfsync_lang=en"),
            ("accept-language", "zh-CN,zh;q=0.9"),
        ]);
        assert_eq!(Lang::negotiate(Some("zh"), &h), Lang::Zh);
        assert_eq!(Lang::negotiate(None, &h), Lang::En);
        let h = headers(&[("accept-language", "zh-TW,en;q=0.8")]);
        assert_eq!(Lang::negotiate(None, &h), Lang::Zh);
        assert_eq!(Lang::negotiate(Some("fr"), &HeaderMap::new()), Lang::En);
    }

    #[test]
    fn ago_formats_per_language() {
        let now = 10 * 86_400_000;
        assert_eq!(ZH.ago(now, now - 5_000), "刚刚");
        assert_eq!(EN.ago(now, now - 5_000), "just now");
        assert_eq!(ZH.ago(now, now - 120_000), "2 分钟前");
        assert_eq!(EN.ago(now, now - 7_200_000), "2 h ago");
        assert_eq!(EN.ago(now, now - 86_400_000), "1 day ago");
        assert_eq!(EN.ago(now, now - 3 * 86_400_000), "3 days ago");
        assert_eq!(ZH.ago(now, now + 5_000), "刚刚");
    }
}
