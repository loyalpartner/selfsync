//! Add `devices` (one row per browser install seen on the sync endpoint) and
//! `settings` (admin-page key/value switches such as `device_approval`).
//!
//! A device is keyed by `(cache_guid, email, browser_kind)`: the cache GUID
//! comes from the `client_id` query parameter Chromium appends to every sync
//! request, and the email/browser pair is the user it was seen acting as.
//! See `docs/device-approval.md`.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Devices::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Devices::Id)
                            .integer()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Devices::CacheGuid).string().not_null())
                    .col(ColumnDef::new(Devices::Email).string().not_null())
                    .col(ColumnDef::new(Devices::BrowserKind).string().not_null())
                    .col(ColumnDef::new(Devices::Status).string().not_null())
                    .col(
                        ColumnDef::new(Devices::UserAgent)
                            .string()
                            .not_null()
                            .default(""),
                    )
                    .col(
                        ColumnDef::new(Devices::RemoteAddr)
                            .string()
                            .not_null()
                            .default(""),
                    )
                    .col(ColumnDef::new(Devices::FirstSeen).big_integer().not_null())
                    .col(ColumnDef::new(Devices::LastSeen).big_integer().not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_devices_guid_user")
                    .table(Devices::Table)
                    .col(Devices::CacheGuid)
                    .col(Devices::Email)
                    .col(Devices::BrowserKind)
                    .unique()
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Settings::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Settings::Key)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Settings::Value).string().not_null())
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Settings::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Devices::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Devices {
    Table,
    Id,
    CacheGuid,
    Email,
    BrowserKind,
    Status,
    UserAgent,
    RemoteAddr,
    FirstSeen,
    LastSeen,
}

#[derive(DeriveIden)]
enum Settings {
    Table,
    Key,
    Value,
}
