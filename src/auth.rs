use std::sync::Arc;

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::{self, Next},
    response::Response,
    Router,
};

/// Wrap a router with bearer token auth.
pub fn with_bearer_auth(router: Router, token: String) -> Router {
    let token = Arc::new(token);
    router.layer(middleware::from_fn(move |request: Request, next: Next| {
        let token = token.clone();
        async move { check_bearer(&token, request, next).await }
    }))
}

async fn check_bearer(
    expected: &str,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok());

    match auth_header {
        Some(header) if header.starts_with("Bearer ") => {
            let token = &header[7..];
            if token == expected {
                Ok(next.run(request).await)
            } else {
                tracing::warn!("Invalid bearer token");
                Err(StatusCode::UNAUTHORIZED)
            }
        }
        _ => {
            tracing::warn!("Missing or malformed Authorization header");
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}
