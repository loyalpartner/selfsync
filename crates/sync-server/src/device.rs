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
//! See `docs/device-approval.md` for the design.

use std::net::SocketAddr;

use axum::http::HeaderMap;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
    sea_query::OnConflict,
};

use crate::auth::ClientIdentity;
use crate::db::entity::{device, setting};
use crate::util::now_millis;

const SETTING_DEVICE_APPROVAL: &str = "device_approval";
const MAX_USER_AGENT_LEN: usize = 256;

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

/// What to do with a request, given the device's stored status (if any) and
/// whether approval is required. `store` is the status to persist; `None`
/// means leave the row alone (or create none).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Decision {
    admission: Admission,
    store: Option<device::Status>,
}

fn decide(existing: Option<device::Status>, approval_required: bool) -> Decision {
    use device::Status::*;
    match (existing, approval_required) {
        (Some(Approved), _) => Decision {
            admission: Admission::Allow,
            store: None,
        },
        (Some(Rejected), _) => Decision {
            admission: Admission::Deny,
            store: None,
        },
        // A device left pending when the switch was turned off is let in on
        // its next attempt, as the admin page promises.
        (Some(Pending) | None, false) => Decision {
            admission: Admission::Allow,
            store: Some(Approved),
        },
        (Some(Pending) | None, true) => Decision {
            admission: Admission::Deny,
            store: Some(Pending),
        },
    }
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

    let now = now_millis();
    let existing = find(db, cache_guid, identity).await?;
    let decision = decide(
        existing.as_ref().map(device::Model::status),
        approval_required,
    );

    match existing {
        Some(row) => {
            let mut active: device::ActiveModel = row.into();
            if let Some(status) = decision.store {
                active.status = Set(status.as_db_str().to_string());
            }
            active.user_agent = Set(req.user_agent.clone());
            active.remote_addr = Set(req.remote_addr.clone());
            active.last_seen = Set(now);
            active.update(db).await?;
        }
        None => {
            let status = decision.store.unwrap_or(device::Status::Pending);
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
            if status == device::Status::Pending {
                tracing::info!(
                    email = %identity.email,
                    browser = %identity.browser,
                    cache_guid,
                    "new device waiting for approval"
                );
            }
        }
    }

    Ok(decision.admission)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::BrowserKind;
    use device::Status::*;

    #[test]
    fn decide_matrix() {
        let allow = |store| Decision {
            admission: Admission::Allow,
            store,
        };
        let deny = |store| Decision {
            admission: Admission::Deny,
            store,
        };
        assert_eq!(decide(Some(Approved), true), allow(None));
        assert_eq!(decide(Some(Approved), false), allow(None));
        assert_eq!(decide(Some(Rejected), true), deny(None));
        assert_eq!(decide(Some(Rejected), false), deny(None));
        assert_eq!(decide(Some(Pending), true), deny(Some(Pending)));
        assert_eq!(decide(Some(Pending), false), allow(Some(Approved)));
        assert_eq!(decide(None, true), deny(Some(Pending)));
        assert_eq!(decide(None, false), allow(Some(Approved)));
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

        // Revoke: denied even after the switch is turned off again.
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
}
