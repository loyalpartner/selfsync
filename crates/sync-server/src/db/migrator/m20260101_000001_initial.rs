//! Initial schema (matches selfsync v0.1.1 release).
//!
//! `users.email` is column-level UNIQUE here. The next migration relaxes
//! that to a composite `(email, browser_kind)` unique key.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Users::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Users::Id)
                            .integer()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Users::Email).string().not_null().unique_key())
                    .col(ColumnDef::new(Users::StoreBirthday).string().not_null())
                    .col(
                        ColumnDef::new(Users::NextVersion)
                            .big_integer()
                            .not_null()
                            .default(1),
                    )
                    .col(
                        ColumnDef::new(Users::EncryptionKey)
                            .string()
                            .not_null()
                            .default(""),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(SyncEntities::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SyncEntities::Id)
                            .integer()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(SyncEntities::UserId).integer().not_null())
                    .col(ColumnDef::new(SyncEntities::IdString).string().not_null())
                    .col(ColumnDef::new(SyncEntities::ParentIdString).string().null())
                    .col(ColumnDef::new(SyncEntities::DataTypeId).integer().not_null())
                    .col(
                        ColumnDef::new(SyncEntities::Version)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(SyncEntities::Ctime).big_integer().null())
                    .col(ColumnDef::new(SyncEntities::Mtime).big_integer().null())
                    .col(ColumnDef::new(SyncEntities::Name).string().null())
                    .col(ColumnDef::new(SyncEntities::ClientTag).string().null())
                    .col(ColumnDef::new(SyncEntities::ServerTag).string().null())
                    .col(ColumnDef::new(SyncEntities::Specifics).blob().null())
                    .col(ColumnDef::new(SyncEntities::UniquePosition).blob().null())
                    .col(
                        ColumnDef::new(SyncEntities::OriginatorCacheGuid)
                            .string()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(SyncEntities::OriginatorClientItemId)
                            .string()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(SyncEntities::Folder)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(SyncEntities::Deleted)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await?;

        // GetUpdates: fetch entities by data type with version > token.
        manager
            .create_index(
                Index::create()
                    .name("idx_sync_updates")
                    .table(SyncEntities::Table)
                    .col(SyncEntities::UserId)
                    .col(SyncEntities::DataTypeId)
                    .col(SyncEntities::Version)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        // (user_id, id_string) uniqueness for upserts and dedup.
        manager
            .create_index(
                Index::create()
                    .name("idx_sync_user_id_string")
                    .table(SyncEntities::Table)
                    .col(SyncEntities::UserId)
                    .col(SyncEntities::IdString)
                    .unique()
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        // client_tag lookups (partial index — backend-specific predicate).
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX IF NOT EXISTS idx_sync_client_tag \
                 ON sync_entities(user_id, client_tag) WHERE client_tag IS NOT NULL",
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(SyncEntities::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Users::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
    Email,
    StoreBirthday,
    NextVersion,
    EncryptionKey,
}

#[derive(DeriveIden)]
enum SyncEntities {
    Table,
    Id,
    UserId,
    IdString,
    ParentIdString,
    DataTypeId,
    Version,
    Ctime,
    Mtime,
    Name,
    ClientTag,
    ServerTag,
    Specifics,
    UniquePosition,
    OriginatorCacheGuid,
    OriginatorClientItemId,
    Folder,
    Deleted,
}
