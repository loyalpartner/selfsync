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

    pub limit_title: &'static str,
    pub limit_desc: &'static str,
    pub limit_usage: &'static str,
    pub save: &'static str,
    pub limit_reached: &'static str,

    pub pending_title: &'static str,
    pub pending_hint: &'static str,
    pub no_pending: &'static str,
    pub clear_pending: &'static str,
    pub ip: &'static str,
    pub first_seen: &'static str,
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
    pub block: &'static str,
    pub unblock: &'static str,
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
    pub done_limit: &'static str,
    pub done_deleted: &'static str,
    pub resume_hint: &'static str,
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

    /// Flash after clearing pending devices.
    pub fn cleared(&self, n: u64) -> String {
        match (self.html_lang, n) {
            ("zh-CN", _) => format!("已清空 {n} 台待审核设备"),
            (_, 1) => "Cleared 1 pending device".to_string(),
            _ => format!("Cleared {n} pending devices"),
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
    approval_desc: "打开后，没批准过的浏览器无法同步，会出现在下方「待审核」里，等你批准。关闭时，新设备连上来会自动批准。已经在待审核里的设备不受开关影响，仍要你来处理。",
    status_on: "已开启 · 新设备批准后才能同步",
    status_off: "已关闭 · 新设备自动批准",

    limit_title: "设备数量上限",
    limit_desc: "已批准和待审核的设备加起来达到上限后，新设备直接拒绝，不记录、不出现在待审核里。黑名单不占名额。",
    limit_usage: "当前",
    save: "保存",
    limit_reached: "已达上限：新设备会被直接拒绝。要加设备，先调高上限，或把不用的设备加入黑名单。",

    pending_title: "待审核",
    pending_hint: "批准后，浏览器下次重试时自动恢复同步，一般几分钟内，不用重启浏览器。",
    no_pending: "没有待审核的设备。",
    clear_pending: "全部清空",
    ip: "来源 IP",
    first_seen: "首次出现",
    device_id: "设备 ID",

    approved_title: "已批准设备",
    rejected_title: "黑名单",
    rejected_hint: "这些设备的请求一律拒绝。「改为批准」直接放行；「移出黑名单」删除记录，下次连上来按新设备处理。",
    no_approved: "还没有已批准的设备。",
    col_device: "设备",
    col_account: "账号",
    col_last_sync: "最近同步",
    col_actions: "操作",
    name_after_sync: "设备名在首次同步后显示",

    approve: "批准",
    block: "加入黑名单",
    unblock: "移出黑名单",
    re_approve: "改为批准",

    users_title: "账号",
    no_users: "还没有账号。",
    col_browser: "浏览器",
    col_approved_devices: "已批准设备",
    col_data_version: "数据版本",

    footer: "设备 ID 由浏览器自己生成。在浏览器里退出账号、关掉再打开同步、或换一个浏览器配置文件后，设备 ID 会变，同一台电脑会以新设备的身份重新出现在「待审核」里。",

    done_approved: "已批准",
    done_rejected: "已加入黑名单",
    done_on: "审核已打开，新设备需要批准",
    done_off: "审核已关闭，新设备会自动批准",
    done_limit: "设备数量上限已保存",
    done_deleted: "已移出黑名单",
    resume_hint: "浏览器会在下次重试时自动恢复同步，一般几分钟内。想马上恢复：重启浏览器，或打开 chrome://sync-internals（Edge 是 edge://sync-internals）点「Trigger GetUpdates」。不要关掉再打开同步，那样设备 ID 会变，要重新审批。",
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
    approval_desc: "When on, browsers you haven't approved can't sync. They show up under Pending below until you approve them. When off, new devices are approved automatically. Devices already pending are not affected by this switch; they wait for you either way.",
    status_on: "On · new devices sync only after approval",
    status_off: "Off · new devices are approved automatically",

    limit_title: "Device limit",
    limit_desc: "Once approved and pending devices reach the limit, new devices are refused outright: not recorded, not listed as pending. Blocked devices don't count.",
    limit_usage: "In use",
    save: "Save",
    limit_reached: "Limit reached: new devices are refused. To add one, raise the limit or block a device you no longer use.",

    pending_title: "Pending",
    pending_hint: "Once approved, the browser resumes on its next retry, usually within a few minutes. No restart needed.",
    no_pending: "No devices waiting for approval.",
    clear_pending: "Clear all",
    ip: "IP",
    first_seen: "First seen",
    device_id: "Device ID",

    approved_title: "Approved devices",
    rejected_title: "Blocked",
    rejected_hint: "Every request from these devices is refused. Approve lets one back in; Unblock forgets it, so its next request is handled as a new device.",
    no_approved: "No approved devices yet.",
    col_device: "Device",
    col_account: "Account",
    col_last_sync: "Last sync",
    col_actions: "Actions",
    name_after_sync: "Device name appears after its first sync",

    approve: "Approve",
    block: "Block",
    unblock: "Unblock",
    re_approve: "Approve",

    users_title: "Accounts",
    no_users: "No accounts yet.",
    col_browser: "Browser",
    col_approved_devices: "Approved devices",
    col_data_version: "Data version",

    footer: "Device IDs are generated by the browser. Signing out, turning sync off and on, or switching browser profiles gives the same computer a new ID, and it shows up under Pending again.",

    done_approved: "Approved",
    done_rejected: "Blocked",
    done_on: "Approval is on; new devices need approval",
    done_off: "Approval is off; new devices are approved automatically",
    done_limit: "Device limit saved",
    done_deleted: "Unblocked",
    resume_hint: "The browser resumes on its next retry, usually within a few minutes. To resume now, restart the browser, or open chrome://sync-internals (edge://sync-internals in Edge) and click Trigger GetUpdates. Don't turn sync off and on: that changes the device ID and it needs approval again.",
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
