pub mod entity;
pub mod migration;

use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbErr};

pub async fn connect(db_path: &str) -> Result<DatabaseConnection, DbErr> {
    let url = format!("sqlite://{db_path}?mode=rwc");
    let mut opts = ConnectOptions::new(url);
    opts.sqlx_logging(false);
    let db = Database::connect(opts).await?;
    // Enable WAL mode for better concurrent read performance
    db.execute_unprepared("PRAGMA journal_mode=WAL").await?;
    migration::run(&db).await?;
    Ok(db)
}
