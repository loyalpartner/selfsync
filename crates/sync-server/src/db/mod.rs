pub mod entity;
mod migrator;

use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbErr};
use sea_orm_migration::MigratorTrait;

pub async fn connect(db_path: &str) -> Result<DatabaseConnection, DbErr> {
    let url = format!("sqlite://{db_path}?mode=rwc");
    let mut opts = ConnectOptions::new(url);
    opts.sqlx_logging(false);
    let db = Database::connect(opts).await?;
    // Enable WAL mode for better concurrent read performance.
    db.execute_unprepared("PRAGMA journal_mode=WAL").await?;
    migrator::Migrator::up(&db, None).await?;
    Ok(db)
}
