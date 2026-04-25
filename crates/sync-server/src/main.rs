mod auth;
mod db;
mod handler;
mod progress;
mod proto;
mod util;

use axum::{
    Extension, Router, middleware,
    routing::{get, post},
};
use clap::Parser;
use tower_http::decompression::RequestDecompressionLayer;

#[derive(Parser, Debug)]
#[command(
    name = "selfsync-server",
    version,
    about = "Self-hosted Chrome sync server"
)]
struct Cli {
    /// TCP address to bind (e.g. 0.0.0.0:8080)
    #[arg(long, env = "SELFSYNC_ADDR", default_value = "127.0.0.1:8080")]
    addr: String,

    /// SQLite database path.
    #[arg(long, env = "SELFSYNC_DB", default_value = "selfsync.db")]
    db: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "selfsync_server=info,http=info".parse().unwrap()),
        )
        .init();

    let db = db::connect(&cli.db).await?;
    tracing::info!(db_path = %cli.db, "database connected");

    let app = Router::new()
        .route("/", get(handler::list_users))
        .route("/healthz", get(|| async { "ok" }))
        .route("/command/", post(handler::handle_command))
        .route("/command", post(handler::handle_command))
        .route("/chrome-sync/command/", post(handler::handle_command))
        .route("/chrome-sync/command", post(handler::handle_command))
        // Edge sync endpoint. Edge derives from Chromium, so when --sync-url is
        // set to ".../v1/feeds/me/syncEntities" the engine appends /command/.
        // Also accept the bare path in case Edge POSTs there directly.
        .route(
            "/v1/feeds/me/syncEntities/command/",
            post(handler::handle_command),
        )
        .route(
            "/v1/feeds/me/syncEntities/command",
            post(handler::handle_command),
        )
        .route("/v1/feeds/me/syncEntities/", post(handler::handle_command))
        .route("/v1/feeds/me/syncEntities", post(handler::handle_command))
        // Edge MSA private endpoint — observed via relay capture. Edge calls
        // this during sync init in addition to /command/. Returning 404 here
        // failed only the BookmarkDataTypeController. Real MSA emits both
        // single- and double-slash variants depending on URL concatenation.
        .route(
            "/sync/v1/diagnosticData/Diagnostic.SendCheckResult()/",
            post(handler::handle_diagnostic_check_result),
        )
        .route(
            "/sync/v1/diagnosticData/Diagnostic.SendCheckResult()",
            post(handler::handle_diagnostic_check_result),
        )
        .route(
            "/v1/diagnosticData/Diagnostic.SendCheckResult()/",
            post(handler::handle_diagnostic_check_result),
        )
        .route(
            "/v1/diagnosticData/Diagnostic.SendCheckResult()",
            post(handler::handle_diagnostic_check_result),
        )
        .layer(middleware::from_fn(handler::log_request))
        .layer(RequestDecompressionLayer::new())
        .layer(Extension(db));

    let listener = tokio::net::TcpListener::bind(&cli.addr).await?;
    tracing::info!(bind_addr = %cli.addr, "selfsync server listening");
    axum::serve(listener, app).await?;

    Ok(())
}
