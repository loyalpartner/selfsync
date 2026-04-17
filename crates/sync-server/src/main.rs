mod auth;
mod db;
mod handler;
mod progress;
mod proto;
mod util;

use axum::{
    Extension, Router,
    routing::{get, post},
};
use tower_http::decompression::RequestDecompressionLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "selfsync_server=info".parse().unwrap()),
        )
        .init();

    let db_path = std::env::var("SELFSYNC_DB").unwrap_or_else(|_| "selfsync.db".to_string());
    let bind_addr = std::env::var("SELFSYNC_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());

    let db = db::connect(&db_path).await?;
    tracing::info!(db_path, "database connected");

    let app = Router::new()
        .route("/", get(handler::list_users))
        .route("/healthz", get(|| async { "ok" }))
        .route("/command/", post(handler::handle_command))
        .route("/command", post(handler::handle_command))
        .route("/chrome-sync/command/", post(handler::handle_command))
        .route("/chrome-sync/command", post(handler::handle_command))
        .layer(RequestDecompressionLayer::new())
        .layer(Extension(db));

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(bind_addr, "selfsync server listening");
    axum::serve(listener, app).await?;

    Ok(())
}
