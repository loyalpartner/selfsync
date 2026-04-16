use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(unique)]
    pub email: String,
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

impl ActiveModelBehavior for ActiveModel {}
