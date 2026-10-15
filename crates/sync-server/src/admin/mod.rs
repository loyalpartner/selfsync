//! Admin page, served on its own listener (`SELFSYNC_ADMIN_ADDR`).
//!
//! It lives apart from the sync port because the sync protocol carries no
//! credentials: anything that can reach `/command` could otherwise reach the
//! approve buttons too. Server-rendered HTML with plain form POSTs; no
//! JavaScript.

mod i18n;
mod ua;

use std::collections::HashMap;

use askama::Template;
use axum::{
    Extension, Form, Router,
    extract::{Path, Query, Request, rejection::QueryRejection},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use prost::Message;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, Order, QueryFilter, QueryOrder};
use serde::Deserialize;

use crate::db::entity::{device, sync_entity, user};
use crate::device::{approval_required, set_approval_required, set_status};
use crate::proto::sync_pb;
use crate::util::now_millis;
use i18n::{LANG_COOKIE, Lang, Texts};

/// `EntitySpecifics.device_info` field number, used as the data type id.
const DATA_TYPE_DEVICE_INFO: i32 = 154522;

/// How the admin listener is exposed; drives the header note and the
/// Host check.
#[derive(Clone, Debug)]
pub struct AdminConfig {
    pub addr: String,
    /// Bound to a loopback address, so only this machine can reach it.
    pub loopback: bool,
}

pub fn router(db: DatabaseConnection, config: AdminConfig) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/settings/device-approval", post(toggle_approval))
        .route("/devices/{id}/approve", post(approve))
        .route("/devices/{id}/reject", post(reject))
        .layer(middleware::from_fn(check_request))
        .layer(Extension(db))
        .layer(Extension(config))
}

/// Refuse requests a browser could be tricked into sending.
///
/// - Loopback listener: the Host must be a loopback name, which stops DNS
///   rebinding from turning a hostile page into a same-origin client.
/// - POST: must not come from another site (cross-site form submission).
async fn check_request(
    Extension(config): Extension<AdminConfig>,
    req: Request,
    next: Next,
) -> Response {
    let headers = req.headers();
    if config.loopback && !host_is_loopback(headers) {
        return (StatusCode::FORBIDDEN, "admin page: unexpected Host header").into_response();
    }
    if req.method() == Method::POST && !same_origin(headers) {
        return (
            StatusCode::FORBIDDEN,
            "admin page: cross-site request refused",
        )
            .into_response();
    }
    next.run(req).await
}

fn host_is_loopback(headers: &HeaderMap) -> bool {
    let Some(host) = headers.get(header::HOST).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let name = if let Some(rest) = host.strip_prefix('[') {
        // [::1]:8081
        rest.split(']').next().unwrap_or("")
    } else {
        host.rsplit_once(':').map_or(host, |(name, _)| name)
    };
    matches!(name, "localhost" | "127.0.0.1" | "::1")
}

/// `Sec-Fetch-Site` when the browser sends it; otherwise compare `Origin`
/// with `Host`. Neither header (curl, scripts) is allowed.
fn same_origin(headers: &HeaderMap) -> bool {
    let get = |name| {
        headers
            .get(name)
            .and_then(|v: &HeaderValue| v.to_str().ok())
    };
    if let Some(site) = get("sec-fetch-site") {
        return matches!(site, "same-origin" | "none");
    }
    match (get("origin"), get("host")) {
        (None, _) => true,
        (Some(origin), Some(host)) => origin.split_once("://").map(|(_, rest)| rest) == Some(host),
        (Some(_), None) => false,
    }
}

#[derive(Deserialize, Default)]
struct IndexParams {
    lang: Option<String>,
    done: Option<String>,
    device: Option<i32>,
}

struct DeviceRow {
    id: i32,
    title: String,
    sub: String,
    email: String,
    ip: String,
    first_seen: String,
    last_seen: String,
    guid: String,
}

struct UserRow {
    email: String,
    browser: String,
    devices: usize,
    version: i64,
}

#[derive(Template)]
#[template(path = "admin.html")]
struct AdminPage {
    t: &'static Texts,
    lang: &'static str,
    approval_on: bool,
    admin_addr: String,
    loopback: bool,
    flash: Option<String>,
    pending: Vec<DeviceRow>,
    approved: Vec<DeviceRow>,
    rejected: Vec<DeviceRow>,
    users: Vec<UserRow>,
}

async fn index(
    Extension(db): Extension<DatabaseConnection>,
    Extension(config): Extension<AdminConfig>,
    params: Result<Query<IndexParams>, QueryRejection>,
    headers: HeaderMap,
) -> Response {
    let Query(params) = params.unwrap_or_default();
    let lang = Lang::negotiate(params.lang.as_deref(), &headers);
    match render_index(&db, &config, lang, &params).await {
        Ok(html) => {
            let mut resp = Html(html).into_response();
            // Remember an explicit choice from the language switch.
            if params.lang.as_deref().and_then(Lang::parse).is_some() {
                let cookie = format!(
                    "{LANG_COOKIE}={}; Path=/; Max-Age=31536000; SameSite=Lax",
                    lang.code()
                );
                if let Ok(v) = HeaderValue::from_str(&cookie) {
                    resp.headers_mut().insert(header::SET_COOKIE, v);
                }
            }
            resp
        }
        Err(e) => {
            tracing::error!("admin page failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    }
}

async fn render_index(
    db: &DatabaseConnection,
    config: &AdminConfig,
    lang: Lang,
    params: &IndexParams,
) -> anyhow::Result<String> {
    let t = lang.texts();
    let now = now_millis();
    let approval_on = approval_required(db).await?;
    let devices = device::Entity::find()
        .order_by(device::Column::LastSeen, Order::Desc)
        .all(db)
        .await?;
    let names = device_names(db).await?;

    let row = |d: &device::Model| {
        let ua_label = ua::label(&d.user_agent);
        let (title, sub) = match names.get(&d.cache_guid) {
            Some(name) => (name.clone(), ua_label),
            None => (ua_label, t.name_after_sync.to_string()),
        };
        DeviceRow {
            id: d.id,
            title,
            sub,
            email: d.email.clone(),
            ip: d.remote_addr.clone(),
            first_seen: utc_date(d.first_seen),
            last_seen: t.ago(now, d.last_seen),
            guid: short_guid(&d.cache_guid),
        }
    };
    let with_status = |s: device::Status| -> Vec<DeviceRow> {
        devices
            .iter()
            .filter(|d| d.status() == s)
            .map(row)
            .collect()
    };
    let pending = with_status(device::Status::Pending);
    let approved = with_status(device::Status::Approved);
    let rejected = with_status(device::Status::Rejected);

    let users = user::Entity::find()
        .order_by(user::Column::Id, Order::Asc)
        .all(db)
        .await?
        .into_iter()
        .map(|u| UserRow {
            devices: devices
                .iter()
                .filter(|d| {
                    d.status() == device::Status::Approved
                        && d.email == u.email
                        && d.browser_kind == u.browser_kind
                })
                .count(),
            email: u.email,
            browser: u.browser_kind,
            version: u.next_version,
        })
        .collect();

    let flash = flash_message(t, params, &devices, &names);

    let page = AdminPage {
        t,
        lang: lang.code(),
        approval_on,
        admin_addr: config.addr.clone(),
        loopback: config.loopback,
        flash,
        pending,
        approved,
        rejected,
        users,
    };
    Ok(page.render()?)
}

/// One line confirming the action that redirected here.
fn flash_message(
    t: &Texts,
    params: &IndexParams,
    devices: &[device::Model],
    names: &HashMap<String, String>,
) -> Option<String> {
    let described = |prefix: &str| {
        let d = devices.iter().find(|d| Some(d.id) == params.device)?;
        let label = names
            .get(&d.cache_guid)
            .cloned()
            .unwrap_or_else(|| ua::label(&d.user_agent));
        Some(format!("{prefix}: {label} ({})", d.email))
    };
    match params.done.as_deref()? {
        "approved" => described(t.done_approved),
        "rejected" => described(t.done_rejected),
        "on" => Some(t.done_on.to_string()),
        "off" => Some(t.done_off.to_string()),
        _ => None,
    }
}

/// Device names keyed by cache GUID, from the DeviceInfo entities each
/// browser syncs about itself. Only devices that have synced have one.
async fn device_names(db: &DatabaseConnection) -> anyhow::Result<HashMap<String, String>> {
    use sync_pb::entity_specifics::SpecificsVariant;

    let rows = sync_entity::Entity::find()
        .filter(sync_entity::Column::DataTypeId.eq(DATA_TYPE_DEVICE_INFO))
        .filter(sync_entity::Column::Deleted.eq(false))
        .all(db)
        .await?;
    let mut names = HashMap::new();
    for row in rows {
        let Some(bytes) = row.specifics else { continue };
        let Ok(specifics) = sync_pb::EntitySpecifics::decode(bytes.as_slice()) else {
            continue;
        };
        if let Some(SpecificsVariant::DeviceInfo(info)) = specifics.specifics_variant
            && let (Some(guid), Some(name)) = (info.cache_guid, info.client_name)
            && !name.is_empty()
        {
            names.insert(guid, name);
        }
    }
    Ok(names)
}

#[derive(Deserialize)]
struct ToggleForm {
    enabled: String,
}

async fn toggle_approval(
    Extension(db): Extension<DatabaseConnection>,
    Form(form): Form<ToggleForm>,
) -> Response {
    let on = form.enabled == "1";
    match set_approval_required(&db, on).await {
        Ok(()) => {
            tracing::info!(on, "device approval switched");
            Redirect::to(if on { "/?done=on" } else { "/?done=off" }).into_response()
        }
        Err(e) => {
            tracing::error!("toggle device approval failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    }
}

async fn approve(Extension(db): Extension<DatabaseConnection>, Path(id): Path<i32>) -> Response {
    change_status(&db, id, device::Status::Approved, "approved").await
}

async fn reject(Extension(db): Extension<DatabaseConnection>, Path(id): Path<i32>) -> Response {
    change_status(&db, id, device::Status::Rejected, "rejected").await
}

async fn change_status(
    db: &DatabaseConnection,
    id: i32,
    status: device::Status,
    done: &str,
) -> Response {
    match set_status(db, id, status).await {
        Ok(Some(d)) => {
            tracing::info!(
                id,
                email = %d.email,
                cache_guid = %d.cache_guid,
                status = status.as_db_str(),
                "device status changed"
            );
            Redirect::to(&format!("/?done={done}&device={id}")).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, "no such device").into_response(),
        Err(e) => {
            tracing::error!(id, "device status change failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    }
}

/// `Qm9r…x7Ta`: enough to tell devices apart without printing the whole
/// GUID, which works like a password for that device.
fn short_guid(guid: &str) -> String {
    let chars: Vec<char> = guid.chars().collect();
    if chars.len() <= 10 {
        return guid.to_string();
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}…{tail}")
}

/// `YYYY-MM-DD` in UTC for Unix milliseconds.
fn utc_date(ms: i64) -> String {
    // Howard Hinnant's civil_from_days.
    let z = ms.div_euclid(86_400_000) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(*k, v.parse().unwrap());
        }
        h
    }

    #[test]
    fn utc_date_known_values() {
        assert_eq!(utc_date(0), "1970-01-01");
        assert_eq!(utc_date(951_782_400_000), "2000-02-29");
        assert_eq!(utc_date(1_792_065_600_000), "2026-10-15");
    }

    #[test]
    fn short_guid_keeps_ends() {
        assert_eq!(short_guid("Qm9rAAAAAAAAx7Ta"), "Qm9r…x7Ta");
        assert_eq!(short_guid("short"), "short");
    }

    #[test]
    fn loopback_host_names() {
        assert!(host_is_loopback(&headers(&[("host", "127.0.0.1:8081")])));
        assert!(host_is_loopback(&headers(&[("host", "localhost:8081")])));
        assert!(host_is_loopback(&headers(&[("host", "[::1]:8081")])));
        assert!(!host_is_loopback(&headers(&[(
            "host",
            "evil.example:8081"
        )])));
        assert!(!host_is_loopback(&HeaderMap::new()));
    }

    #[test]
    fn cross_site_posts_are_refused() {
        assert!(same_origin(&headers(&[("sec-fetch-site", "same-origin")])));
        assert!(!same_origin(&headers(&[("sec-fetch-site", "cross-site")])));
        assert!(!same_origin(&headers(&[("sec-fetch-site", "same-site")])));
        assert!(same_origin(&headers(&[
            ("origin", "http://127.0.0.1:8081"),
            ("host", "127.0.0.1:8081"),
        ])));
        assert!(!same_origin(&headers(&[
            ("origin", "https://evil.example"),
            ("host", "127.0.0.1:8081"),
        ])));
        assert!(same_origin(&HeaderMap::new()));
    }
}
