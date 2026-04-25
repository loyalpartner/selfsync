use sea_orm::entity::prelude::*;

use crate::auth::BrowserKind;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    /// Account email parsed from the request. May collide across browsers,
    /// so it is NOT unique on its own — `(email, browser_kind)` is.
    pub email: String,
    /// Stable string from `BrowserKind::as_str` (e.g. `"chromium"`, `"edge"`).
    /// Edge and Chromium with the same email are different rows: their
    /// cryptographers and permanent folder sets are incompatible.
    pub browser_kind: String,
    pub store_birthday: String,
    /// Monotonically increasing version counter for this user's sync entities.
    pub next_version: i64,
    /// Random encryption key for Nigori keystore encryption.
    pub encryption_key: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::sync_entity::Entity")]
    SyncEntities,
}

impl Related<super::sync_entity::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::SyncEntities.def()
    }
}

impl Model {
    /// Decoded browser kind. Cheap (string match), but cache the result if
    /// you need it many times in one request.
    pub fn browser(&self) -> BrowserKind {
        BrowserKind::from_db(&self.browser_kind)
    }
}

impl ActiveModelBehavior for ActiveModel {}
