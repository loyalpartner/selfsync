use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr};

/// Create all tables if they don't exist.
pub async fn run(db: &DatabaseConnection) -> Result<(), DbErr> {
    let backend = db.get_database_backend();

    db.execute(
        backend.build(
            sea_orm::sea_query::Table::create()
                .table(super::entity::user::Entity)
                .if_not_exists()
                .col(
                    sea_orm::sea_query::ColumnDef::new(super::entity::user::Column::Id)
                        .integer()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    sea_orm::sea_query::ColumnDef::new(super::entity::user::Column::Email)
                        .string()
                        .not_null()
                        .unique_key(),
                )
                .col(
                    sea_orm::sea_query::ColumnDef::new(super::entity::user::Column::StoreBirthday)
                        .string()
                        .not_null(),
                )
                .col(
                    sea_orm::sea_query::ColumnDef::new(super::entity::user::Column::NextVersion)
                        .big_integer()
                        .not_null()
                        .default(1),
                )
                .col(
                    sea_orm::sea_query::ColumnDef::new(super::entity::user::Column::EncryptionKey)
                        .string()
                        .not_null()
                        .default(""),
                ),
        ),
    )
    .await?;

    db.execute(
        backend.build(
            sea_orm::sea_query::Table::create()
                .table(super::entity::sync_entity::Entity)
                .if_not_exists()
                .col(
                    sea_orm::sea_query::ColumnDef::new(super::entity::sync_entity::Column::Id)
                        .integer()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    sea_orm::sea_query::ColumnDef::new(super::entity::sync_entity::Column::UserId)
                        .integer()
                        .not_null(),
                )
                .col(
                    sea_orm::sea_query::ColumnDef::new(
                        super::entity::sync_entity::Column::IdString,
                    )
                    .string()
                    .not_null(),
                )
                .col(
                    sea_orm::sea_query::ColumnDef::new(
                        super::entity::sync_entity::Column::ParentIdString,
                    )
                    .string()
                    .null(),
                )
                .col(
                    sea_orm::sea_query::ColumnDef::new(
                        super::entity::sync_entity::Column::DataTypeId,
                    )
                    .integer()
                    .not_null(),
                )
                .col(
                    sea_orm::sea_query::ColumnDef::new(super::entity::sync_entity::Column::Version)
                        .big_integer()
                        .not_null()
                        .default(0),
                )
                .col(
                    sea_orm::sea_query::ColumnDef::new(super::entity::sync_entity::Column::Ctime)
                        .big_integer()
                        .null(),
                )
                .col(
                    sea_orm::sea_query::ColumnDef::new(super::entity::sync_entity::Column::Mtime)
                        .big_integer()
                        .null(),
                )
                .col(
                    sea_orm::sea_query::ColumnDef::new(super::entity::sync_entity::Column::Name)
                        .string()
                        .null(),
                )
                .col(
                    sea_orm::sea_query::ColumnDef::new(
                        super::entity::sync_entity::Column::ClientTag,
                    )
                    .string()
                    .null(),
                )
                .col(
                    sea_orm::sea_query::ColumnDef::new(
                        super::entity::sync_entity::Column::ServerTag,
                    )
                    .string()
                    .null(),
                )
                .col(
                    sea_orm::sea_query::ColumnDef::new(
                        super::entity::sync_entity::Column::Specifics,
                    )
                    .blob()
                    .null(),
                )
                .col(
                    sea_orm::sea_query::ColumnDef::new(
                        super::entity::sync_entity::Column::UniquePosition,
                    )
                    .blob()
                    .null(),
                )
                .col(
                    sea_orm::sea_query::ColumnDef::new(
                        super::entity::sync_entity::Column::OriginatorCacheGuid,
                    )
                    .string()
                    .null(),
                )
                .col(
                    sea_orm::sea_query::ColumnDef::new(
                        super::entity::sync_entity::Column::OriginatorClientItemId,
                    )
                    .string()
                    .null(),
                )
                .col(
                    sea_orm::sea_query::ColumnDef::new(super::entity::sync_entity::Column::Folder)
                        .boolean()
                        .not_null()
                        .default(false),
                )
                .col(
                    sea_orm::sea_query::ColumnDef::new(super::entity::sync_entity::Column::Deleted)
                        .boolean()
                        .not_null()
                        .default(false),
                ),
        ),
    )
    .await?;

    // Index for GetUpdates: fetch entities by data type with version > token
    db.execute_unprepared(
        "CREATE INDEX IF NOT EXISTS idx_sync_updates \
         ON sync_entities(user_id, data_type_id, version)",
    )
    .await?;

    // Index for client tag deduplication
    db.execute_unprepared(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_sync_user_id_string \
         ON sync_entities(user_id, id_string)",
    )
    .await?;

    // Index for client_defined_unique_tag lookups
    db.execute_unprepared(
        "CREATE INDEX IF NOT EXISTS idx_sync_client_tag \
         ON sync_entities(user_id, client_tag) WHERE client_tag IS NOT NULL",
    )
    .await?;

    Ok(())
}
