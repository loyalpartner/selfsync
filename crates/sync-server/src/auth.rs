use axum::{extract::Request, middleware::Next, response::Response};

const DEFAULT_EMAIL: &str = "anonymous@localhost";
const EMAIL_HEADER: &str = "x-sync-user-email";

/// Extract user email from X-Sync-User-Email header, fallback to default.
#[derive(Clone, Debug)]
pub struct UserEmail(pub String);

pub async fn auth_middleware(mut request: Request, next: Next) -> Response {
    let email = request
        .headers()
        .get(EMAIL_HEADER)
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_EMAIL)
        .to_string();

    request.extensions_mut().insert(UserEmail(email));
    next.run(request).await
}
