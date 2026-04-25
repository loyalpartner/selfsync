//! Schema migrations, applied in order on every startup. New migrations
//! append to [`Migrator::migrations`] — never reorder, never edit a released
//! entry, never delete one. `sea-orm-migration` records applied versions in
//! the `seaql_migrations` table so each migration runs exactly once.

use sea_orm_migration::prelude::*;

mod m20260101_000001_initial;
mod m20260425_000001_users_add_browser_kind;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260101_000001_initial::Migration),
            Box::new(m20260425_000001_users_add_browser_kind::Migration),
        ]
    }
}
