use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "sync_entities")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub user_id: i32,
    /// Server-assigned entity ID (base64-encoded UUID).
    pub id_string: String,
    pub parent_id_string: Option<String>,
    /// Chrome sync data type identifier (e.g. 47745 = Nigori).
    pub data_type_id: i32,
    /// Monotonically increasing version, assigned from user.next_version.
    pub version: i64,
    pub ctime: Option<i64>,
    pub mtime: Option<i64>,
    pub name: Option<String>,
    pub client_tag: Option<String>,
    pub server_tag: Option<String>,
    /// Raw protobuf bytes of EntitySpecifics.
    #[sea_orm(column_type = "Blob")]
    pub specifics: Option<Vec<u8>>,
    /// Raw protobuf bytes of UniquePosition.
    #[sea_orm(column_type = "Blob")]
    pub unique_position: Option<Vec<u8>>,
    pub originator_cache_guid: Option<String>,
    pub originator_client_item_id: Option<String>,
    pub folder: bool,
    pub deleted: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id"
    )]
    User,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
