//! Per-device admission for sync requests.
//!
//! Chromium appends `?client=...&client_id=<cache_guid>` to every sync
//! request URL (`components/sync/engine/net/url_translator.cc`), so each
//! browser install can be told apart before any protobuf work happens. When
//! the admin turns on "require approval for new devices", unknown installs are
//! recorded as pending and refused until approved on the admin page. With the
//! switch off they are recorded and approved automatically, so turning it on
//! later does not lock out devices already in use.
//!
//! Pending and blocklisted devices are refused without touching the database,
//! and once approved plus pending devices reach the admin-set limit, unknown
//! installs are refused without being recorded. Either way an unapproved
//! client can't make the server write.
//!
//! See `docs/device-approval.md` for the design.

use std::net::SocketAddr;

use axum::http::HeaderMap;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    Set, sea_query::OnConflict,
};

use crate::auth::ClientIdentity;
use crate::db::entity::{device, setting};
use crate::util::now_millis;

const SETTING_DEVICE_APPROVAL: &str = "device_approval";
const SETTING_DEVICE_LIMIT: &str = "device_limit";
const MAX_USER_AGENT_LEN: usize = 256;
/// Chromium's cache GUID is 16 random bytes in base64 (24 chars); leave room
/// for other Chromium-based clients without letting a request store an
/// arbitrarily long id.
const MAX_CACHE_GUID_LEN: usize = 64;
/// RFC 5321 limit on a mailbox.
const MAX_EMAIL_LEN: usize = 254;

/// Default cap on approved plus pending devices.
pub const DEFAULT_DEVICE_LIMIT: u64 = 20;
/// Range the admin page accepts for the cap.
pub const DEVICE_LIMIT_RANGE: std::ops::RangeInclusive<u64> = 1..=1000;

/// Device metadata taken from the HTTP layer of a sync request.
#[derive(Debug, Clone, Default)]
pub struct DeviceRequest {
    /// Cache GUID from the `client_id` query parameter.
    pub cache_guid: Option<String>,
    pub user_agent: String,
    pub remote_addr: String,
}

impl DeviceRequest {
    pub fn new(client_id: Option<&str>, headers: &HeaderMap, peer: Option<SocketAddr>) -> Self {
        let cache_guid = client_id
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let user_agent: String = headers
            .get("user-agent")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .chars()
            .take(MAX_USER_AGENT_LEN)
            .collect();
        Self {
            cache_guid,
            user_agent,
            remote_addr: remote_addr(headers, peer),
        }
    }
}

/// Client address for display on the admin page. Behind a reverse proxy the
/// peer is the proxy itself, so prefer the first `X-Forwarded-For` hop. This
/// is informational only — never use it for access decisions, it is
/// client-controlled.
fn remote_addr(headers: &HeaderMap, peer: Option<SocketAddr>) -> String {
    let forwarded = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    match (forwarded, peer) {
        (Some(ip), _) => ip.to_string(),
        (None, Some(addr)) => addr.ip().to_string(),
        (None, None) => String::new(),
    }
}

/// Outcome of [`admit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Allow,
    Deny,
}

/// What to do with a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    /// Known approved device: let it through and refresh its row.
    Allow,
    /// Refuse without writing anything.
    Deny,
    /// New device: insert a row with this status. `Approved` lets the
    /// request through, `Pending` refuses it.
    Record(device::Status),
}

/// `at_limit`: approved plus pending devices have reached the cap. Only
/// consulted for devices without a row.
fn decide(existing: Option<device::Status>, approval_required: bool, at_limit: bool) -> Action {
    use device::Status::*;
    match existing {
        Some(Approved) => Action::Allow,
        // Pending stays pending until the admin acts, even if the switch is
        // turned off meanwhile.
        Some(Pending | Rejected) => Action::Deny,
        None if at_limit => Action::Deny,
        None if approval_required => Action::Record(Pending),
        None => Action::Record(Approved),
    }
}

/// A client id or email no Chromium-based browser would send. Refused
/// before anything is written.
fn malformed(identity: &ClientIdentity, cache_guid: &str) -> bool {
    let guid_ok = cache_guid.len() <= MAX_CACHE_GUID_LEN
        && cache_guid
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"+/=-_".contains(&b));
    !guid_ok || identity.email.len() > MAX_EMAIL_LEN
}

/// Decide whether a sync request may proceed, recording the device as a
/// side effect. Must run before the user row is looked up or created, so an
/// unapproved device cannot make the server allocate a user.
pub async fn admit(
    db: &DatabaseConnection,
    identity: &ClientIdentity,
    req: &DeviceRequest,
) -> anyhow::Result<Admission> {
    let approval_required = approval_required(db).await?;

    let Some(cache_guid) = req.cache_guid.as_deref() else {
        // Old or non-standard client: nothing to key a device row on.
        return Ok(if approval_required {
            Admission::Deny
        } else {
            Admission::Allow
        });
    };
    if malformed(identity, cache_guid) {
        return Ok(Admission::Deny);
    }

    let now = now_millis();
    let existing = find(db, cache_guid, identity).await?;
    let at_limit = existing.is_none() && active_count(db).await? >= device_limit(db).await?;
    let action = decide(
        existing.as_ref().map(device::Model::status),
        approval_required,
        at_limit,
    );

    match (action, existing) {
        (Action::Allow, Some(row)) => {
            let mut active: device::ActiveModel = row.into();
            active.user_agent = Set(req.user_agent.clone());
            active.remote_addr = Set(req.remote_addr.clone());
            active.last_seen = Set(now);
            active.update(db).await?;
            Ok(Admission::Allow)
        }
        (Action::Record(status), None) => {
            let row = device::ActiveModel {
                cache_guid: Set(cache_guid.to_string()),
                email: Set(identity.email.clone()),
                browser_kind: Set(identity.browser.as_db_str().to_string()),
                status: Set(status.as_db_str().to_string()),
                user_agent: Set(req.user_agent.clone()),
                remote_addr: Set(req.remote_addr.clone()),
                first_seen: Set(now),
                last_seen: Set(now),
                ..Default::default()
            };
            // Concurrent first requests from one device race here; the
            // unique index keeps a single row and the loser is a no-op.
            device::Entity::insert(row)
                .on_conflict(
                    OnConflict::columns([
                        device::Column::CacheGuid,
                        device::Column::Email,
                        device::Column::BrowserKind,
                    ])
                    .do_nothing()
                    .to_owned(),
                )
                .exec_without_returning(db)
                .await?;
            if status == device::Status::Approved {
                return Ok(Admission::Allow);
            }
            tracing::info!(
                email = %identity.email,
                browser = %identity.browser,
                cache_guid,
                "new device waiting for approval"
            );
            Ok(Admission::Deny)
        }
        // Deny, or a stale pairing from a concurrent admin change: refuse.
        _ => {
            if at_limit {
                tracing::warn!(
                    email = %identity.email,
                    cache_guid,
                    "device limit reached, new device refused"
                );
            }
            Ok(Admission::Deny)
        }
    }
}

async fn find(
    db: &DatabaseConnection,
    cache_guid: &str,
    identity: &ClientIdentity,
) -> anyhow::Result<Option<device::Model>> {
    Ok(device::Entity::find()
        .filter(device::Column::CacheGuid.eq(cache_guid))
        .filter(device::Column::Email.eq(&identity.email))
        .filter(device::Column::BrowserKind.eq(identity.browser.as_db_str()))
        .one(db)
        .await?)
}

/// Whether new devices need approval. Defaults to off.
pub async fn approval_required(db: &DatabaseConnection) -> anyhow::Result<bool> {
    let row = setting::Entity::find_by_id(SETTING_DEVICE_APPROVAL)
        .one(db)
        .await?;
    Ok(row.is_some_and(|r| r.value == "1"))
}

pub async fn set_approval_required(db: &DatabaseConnection, on: bool) -> anyhow::Result<()> {
    let row = setting::ActiveModel {
        key: Set(SETTING_DEVICE_APPROVAL.to_string()),
        value: Set(if on { "1" } else { "0" }.to_string()),
    };
    setting::Entity::insert(row)
        .on_conflict(
            OnConflict::column(setting::Column::Key)
                .update_column(setting::Column::Value)
                .to_owned(),
        )
        .exec_without_returning(db)
        .await?;
    Ok(())
}

/// Change a device's status from the admin page. Returns the updated row, or
/// `None` when the id does not exist.
pub async fn set_status(
    db: &DatabaseConnection,
    id: i32,
    status: device::Status,
) -> anyhow::Result<Option<device::Model>> {
    let Some(row) = device::Entity::find_by_id(id).one(db).await? else {
        return Ok(None);
    };
    let mut active: device::ActiveModel = row.into();
    active.status = Set(status.as_db_str().to_string());
    Ok(Some(active.update(db).await?))
}

/// Cap on approved plus pending devices. Unknown devices beyond it are
/// refused without being recorded.
pub async fn device_limit(db: &DatabaseConnection) -> anyhow::Result<u64> {
    let row = setting::Entity::find_by_id(SETTING_DEVICE_LIMIT)
        .one(db)
        .await?;
    Ok(row
        .and_then(|r| r.value.parse().ok())
        .unwrap_or(DEFAULT_DEVICE_LIMIT))
}

pub async fn set_device_limit(db: &DatabaseConnection, limit: u64) -> anyhow::Result<()> {
    let row = setting::ActiveModel {
        key: Set(SETTING_DEVICE_LIMIT.to_string()),
        value: Set(limit.to_string()),
    };
    setting::Entity::insert(row)
        .on_conflict(
            OnConflict::column(setting::Column::Key)
                .update_column(setting::Column::Value)
                .to_owned(),
        )
        .exec_without_returning(db)
        .await?;
    Ok(())
}

/// Approved plus pending devices; what [`device_limit`] caps. Blocklisted
/// rows don't count, so blocking junk frees room.
async fn active_count(db: &DatabaseConnection) -> anyhow::Result<u64> {
    Ok(device::Entity::find()
        .filter(device::Column::Status.ne(device::Status::Rejected.as_db_str()))
        .count(db)
        .await?)
}

/// Forget a device; its next request is handled as a new device. Returns
/// the deleted row, or `None` when the id does not exist.
pub async fn delete(db: &DatabaseConnection, id: i32) -> anyhow::Result<Option<device::Model>> {
    let Some(row) = device::Entity::find_by_id(id).one(db).await? else {
        return Ok(None);
    };
    device::Entity::delete_by_id(id).exec(db).await?;
    Ok(Some(row))
}

/// Forget every pending device. Returns how many were removed.
pub async fn clear_pending(db: &DatabaseConnection) -> anyhow::Result<u64> {
    let res = device::Entity::delete_many()
        .filter(device::Column::Status.eq(device::Status::Pending.as_db_str()))
        .exec(db)
        .await?;
    Ok(res.rows_affected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::BrowserKind;
    use device::Status::*;

    #[test]
    fn decide_matrix() {
        use Action::*;
        for switch in [false, true] {
            for limit in [false, true] {
                assert_eq!(decide(Some(Approved), switch, limit), Allow);
                assert_eq!(decide(Some(Pending), switch, limit), Deny);
                assert_eq!(decide(Some(Rejected), switch, limit), Deny);
                assert_eq!(decide(None, switch, true), Deny);
            }
        }
        assert_eq!(decide(None, false, false), Record(Approved));
        assert_eq!(decide(None, true, false), Record(Pending));
    }

    #[test]
    fn malformed_ids_and_emails() {
        let me = alice();
        assert!(!malformed(&me, "Qm9yQ2FjaGVHdWlkMTIzNA=="));
        assert!(!malformed(&me, "dev-a_b"));
        assert!(malformed(&me, &"a".repeat(MAX_CACHE_GUID_LEN + 1)));
        assert!(malformed(&me, "has space"));
        assert!(malformed(&me, "<script>"));
        let long = ClientIdentity {
            email: format!("{}@example.com", "a".repeat(MAX_EMAIL_LEN)),
            browser: BrowserKind::Chromium,
        };
        assert!(malformed(&long, "dev-a"));
    }

    #[test]
    fn remote_addr_prefers_first_forwarded_hop() {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "203.0.113.5, 10.0.0.1".parse().unwrap());
        let peer: SocketAddr = "127.0.0.1:9999".parse().unwrap();
        assert_eq!(remote_addr(&h, Some(peer)), "203.0.113.5");
        assert_eq!(remote_addr(&HeaderMap::new(), Some(peer)), "127.0.0.1");
        assert_eq!(remote_addr(&HeaderMap::new(), None), "");
    }

    #[test]
    fn blank_client_id_is_no_guid() {
        let r = DeviceRequest::new(Some("  "), &HeaderMap::new(), None);
        assert!(r.cache_guid.is_none());
    }

    /// A fresh on-disk database. `:memory:` would give each pooled
    /// connection its own empty database.
    async fn test_db() -> DatabaseConnection {
        let path = std::env::temp_dir().join(format!("selfsync-test-{}.db", uuid::Uuid::new_v4()));
        crate::db::connect(path.to_str().unwrap()).await.unwrap()
    }

    fn alice() -> ClientIdentity {
        ClientIdentity {
            email: "alice@example.com".into(),
            browser: BrowserKind::Chromium,
        }
    }

    fn from(guid: &str) -> DeviceRequest {
        DeviceRequest {
            cache_guid: Some(guid.into()),
            user_agent: "Chrome MAC 141.0.0.0 (x) channel(stable)".into(),
            remote_addr: "192.0.2.1".into(),
        }
    }

    #[tokio::test]
    async fn existing_devices_survive_turning_approval_on() {
        let db = test_db().await;
        let me = alice();

        // Switch off (default): device is recorded and approved.
        assert_eq!(
            admit(&db, &me, &from("dev-a")).await.unwrap(),
            Admission::Allow
        );

        set_approval_required(&db, true).await.unwrap();
        assert!(approval_required(&db).await.unwrap());

        // Known device still allowed; a new one is held.
        assert_eq!(
            admit(&db, &me, &from("dev-a")).await.unwrap(),
            Admission::Allow
        );
        assert_eq!(
            admit(&db, &me, &from("dev-b")).await.unwrap(),
            Admission::Deny
        );

        let b = find(&db, "dev-b", &me).await.unwrap().unwrap();
        assert_eq!(b.status(), Pending);

        // Approve from the admin page, next attempt goes through.
        set_status(&db, b.id, Approved).await.unwrap();
        assert_eq!(
            admit(&db, &me, &from("dev-b")).await.unwrap(),
            Admission::Allow
        );

        // Blocklist: denied even after the switch is turned off again.
        set_status(&db, b.id, Rejected).await.unwrap();
        set_approval_required(&db, false).await.unwrap();
        assert_eq!(
            admit(&db, &me, &from("dev-b")).await.unwrap(),
            Admission::Deny
        );
    }

    #[tokio::test]
    async fn missing_client_id_follows_the_switch() {
        let db = test_db().await;
        let me = alice();
        let anon = DeviceRequest::default();
        assert_eq!(admit(&db, &me, &anon).await.unwrap(), Admission::Allow);
        set_approval_required(&db, true).await.unwrap();
        assert_eq!(admit(&db, &me, &anon).await.unwrap(), Admission::Deny);
    }

    async fn device_count(db: &DatabaseConnection) -> u64 {
        device::Entity::find().count(db).await.unwrap()
    }

    async fn last_seen(db: &DatabaseConnection, guid: &str) -> i64 {
        find(db, guid, &alice()).await.unwrap().unwrap().last_seen
    }

    #[tokio::test]
    async fn pending_stays_pending_and_untouched() {
        let db = test_db().await;
        let me = alice();
        set_approval_required(&db, true).await.unwrap();
        admit(&db, &me, &from("dev-a")).await.unwrap();
        let seen = last_seen(&db, "dev-a").await;

        // Turning the switch off doesn't let a pending device in.
        set_approval_required(&db, false).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        assert_eq!(
            admit(&db, &me, &from("dev-a")).await.unwrap(),
            Admission::Deny
        );
        assert_eq!(last_seen(&db, "dev-a").await, seen);
        let a = find(&db, "dev-a", &me).await.unwrap().unwrap();
        assert_eq!(a.status(), Pending);
    }

    #[tokio::test]
    async fn limit_refuses_unknown_devices_without_recording() {
        let db = test_db().await;
        let me = alice();
        set_device_limit(&db, 2).await.unwrap();
        assert_eq!(device_limit(&db).await.unwrap(), 2);

        for guid in ["dev-a", "dev-b"] {
            assert_eq!(
                admit(&db, &me, &from(guid)).await.unwrap(),
                Admission::Allow
            );
        }
        assert_eq!(
            admit(&db, &me, &from("dev-c")).await.unwrap(),
            Admission::Deny
        );
        assert_eq!(device_count(&db).await, 2);
        // Known devices are unaffected.
        assert_eq!(
            admit(&db, &me, &from("dev-a")).await.unwrap(),
            Admission::Allow
        );

        // Blocklisting one frees a slot.
        let b = find(&db, "dev-b", &me).await.unwrap().unwrap();
        set_status(&db, b.id, Rejected).await.unwrap();
        assert_eq!(
            admit(&db, &me, &from("dev-c")).await.unwrap(),
            Admission::Allow
        );
    }

    #[tokio::test]
    async fn malformed_requests_are_refused_without_recording() {
        let db = test_db().await;
        let long = from(&"a".repeat(MAX_CACHE_GUID_LEN + 1));
        assert_eq!(admit(&db, &alice(), &long).await.unwrap(), Admission::Deny);
        assert_eq!(device_count(&db).await, 0);
    }

    #[tokio::test]
    async fn delete_and_clear_pending() {
        let db = test_db().await;
        let me = alice();
        set_approval_required(&db, true).await.unwrap();
        for guid in ["dev-a", "dev-b", "dev-c"] {
            admit(&db, &me, &from(guid)).await.unwrap();
        }
        let a = find(&db, "dev-a", &me).await.unwrap().unwrap();
        assert!(delete(&db, a.id).await.unwrap().is_some());
        assert!(delete(&db, a.id).await.unwrap().is_none());
        assert_eq!(clear_pending(&db).await.unwrap(), 2);
        assert_eq!(device_count(&db).await, 0);
    }
}
