mod commit;
mod get_updates;
mod init;
pub(crate) mod sync;
mod users;

pub use sync::handle_command;
pub use users::list_users;

use axum::{extract::Request, middleware::Next, response::Response};

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
