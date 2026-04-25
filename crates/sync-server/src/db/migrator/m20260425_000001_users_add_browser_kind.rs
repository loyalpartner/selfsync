//! Add `users.browser_kind` and replace the column-level UNIQUE on `email`
//! with a composite UNIQUE on `(email, browser_kind)`.
//!
//! Why: Edge and Chromium with the same email cannot share a server-side
//! user — their cryptographers and permanent-folder sets are incompatible.
//! See `auth::BrowserKind` for the full reasoning.
//!
//! SQLite has no `ALTER TABLE DROP CONSTRAINT`, so removing the column-level
//! UNIQUE requires a table rebuild. We follow SQLite's recommended pattern:
//! create a new table, copy rows, drop the old one, rename. Existing rows
//! are backfilled as `chromium` because that's what every pre-v0.1.2 user
//! actually was.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        // 1. Rebuild `users` without the column-level UNIQUE on `email`,
        //    adding the new `browser_kind` column at the same time. Wrap in
        //    a transaction so a crash mid-rebuild can't lose the table.
        conn.execute_unprepared(
            "BEGIN;\
             CREATE TABLE users_new (\
                id INTEGER PRIMARY KEY AUTOINCREMENT,\
                email TEXT NOT NULL,\
                browser_kind TEXT NOT NULL DEFAULT 'chromium',\
                store_birthday TEXT NOT NULL,\
                next_version BIGINT NOT NULL DEFAULT 1,\
                encryption_key TEXT NOT NULL DEFAULT ''\
             );\
             INSERT INTO users_new (id, email, browser_kind, store_birthday, next_version, encryption_key)\
                SELECT id, email, 'chromium', store_birthday, next_version, encryption_key FROM users;\
             DROP TABLE users;\
             ALTER TABLE users_new RENAME TO users;\
             COMMIT;",
        )
        .await?;

        // 2. Composite uniqueness — same email is allowed across browsers.
        manager
            .create_index(
                Index::create()
                    .name("idx_users_email_browser")
                    .table(Users::Table)
                    .col(Users::Email)
                    .col(Users::BrowserKind)
                    .unique()
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        manager
            .drop_index(
                Index::drop()
                    .name("idx_users_email_browser")
                    .table(Users::Table)
                    .to_owned(),
            )
            .await?;

        // Rebuild without browser_kind, restoring email UNIQUE. We can only
        // do this safely if no two surviving rows share an email — otherwise
        // the rollback would have to discard data, which a `down()` should
        // never do silently. Fail loudly in that case.
        conn.execute_unprepared(
            "BEGIN;\
             CREATE TABLE users_old (\
                id INTEGER PRIMARY KEY AUTOINCREMENT,\
                email TEXT NOT NULL UNIQUE,\
                store_birthday TEXT NOT NULL,\
                next_version BIGINT NOT NULL DEFAULT 1,\
                encryption_key TEXT NOT NULL DEFAULT ''\
             );\
             INSERT INTO users_old (id, email, store_birthday, next_version, encryption_key)\
                SELECT id, email, store_birthday, next_version, encryption_key FROM users;\
             DROP TABLE users;\
             ALTER TABLE users_old RENAME TO users;\
             COMMIT;",
        )
        .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Email,
    BrowserKind,
}
