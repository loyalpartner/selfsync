mod commit;
mod get_updates;
mod init;
pub(crate) mod sync;
mod users;

pub use sync::handle_command;
pub use users::list_users;

use axum::{
    extract::Request,
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

/// Logs every incoming request: method, path, and the response status. Used
/// to trace the full surface a client touches, not just the routes we matched.
pub async fn log_request(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let query = req.uri().query().unwrap_or("").to_string();

    let response = next.run(req).await;
    let status = response.status();

    tracing::info!(
        target: "http",
        method = %method,
        path = %path,
        query = %query,
        status = status.as_u16(),
        "request"
    );

    response
}

/// Edge MSA private endpoint: `Diagnostic.SendCheckResult()`. Edge calls this
/// alongside the sync command endpoint, and a 404 here was observed to fail
/// `BookmarkDataTypeController` initialization (other data types don't gate
/// on this, which is why bookmarks were the only type stuck).
///
/// Real MSA returns a 6-byte protobuf `0a 04 08 01 10 01` (a tiny success
/// envelope). We reproduce it byte-for-byte; we have no schema for it.
pub async fn handle_diagnostic_check_result() -> impl IntoResponse {
    const RESPONSE: [u8; 6] = [0x0a, 0x04, 0x08, 0x01, 0x10, 0x01];
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/octet-stream")],
        RESPONSE,
    )
}
