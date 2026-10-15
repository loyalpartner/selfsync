use sea_orm::entity::prelude::*;

/// One browser install seen on the sync endpoint. Identified by the cache
/// GUID the browser sends as `?client_id=` plus the user it acted as.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "devices")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub cache_guid: String,
    pub email: String,
    /// Same values as `users.browser_kind`.
    pub browser_kind: String,
    /// Stable string from [`Status::as_db_str`].
    pub status: String,
    /// Sync-engine User-Agent, e.g. `Chrome WIN 141.0.7390.54 (...)`.
    pub user_agent: String,
    pub remote_addr: String,
    /// Milliseconds since Unix epoch.
    pub first_seen: i64,
    pub last_seen: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl Model {
    pub fn status(&self) -> Status {
        Status::from_db(&self.status)
    }
}

/// Approval state of a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Approved,
    Pending,
    Rejected,
}

impl Status {
    /// Stored in `devices.status`. Changing these values is a breaking
    /// schema change.
    pub const fn as_db_str(self) -> &'static str {
        match self {
            Status::Approved => "approved",
            Status::Pending => "pending",
            Status::Rejected => "rejected",
        }
    }

    /// Unknown values map to `Rejected`: a row we can't interpret must not
    /// be allowed to sync.
    pub fn from_db(s: &str) -> Self {
        match s {
            "approved" => Status::Approved,
            "pending" => Status::Pending,
            _ => Status::Rejected,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_round_trips_through_db_column() {
        for s in [Status::Approved, Status::Pending, Status::Rejected] {
            assert_eq!(Status::from_db(s.as_db_str()), s);
        }
        assert_eq!(Status::from_db("garbage"), Status::Rejected);
    }
}
