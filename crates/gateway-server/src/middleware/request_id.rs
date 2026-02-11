//! Request ID middleware for tracing and correlation.

use axum::{
    extract::Request,
    http::HeaderValue,
    middleware::Next,
    response::Response,
};
use gateway_core::types::RequestId;

/// Header name for request ID.
pub const X_REQUEST_ID: &str = "x-request-id";

/// Middleware that generates or extracts a request ID and adds it to response headers.
/// If the incoming request has an X-Request-Id header, it will be preserved.
/// Otherwise, a new UUIDv7 will be generated.
pub async fn request_id_middleware(mut req: Request, next: Next) -> Response {
    // Extract or generate request ID
    let request_id = req
        .headers()
        .get(X_REQUEST_ID)
        .and_then(|v| v.to_str().ok())
        .map(|s| RequestId(s.to_string()))
        .unwrap_or_else(RequestId::new);

    // Store request ID in extensions for handlers to access
    req.extensions_mut().insert(request_id.clone());

    // Add tracing span with request ID
    let span = tracing::info_span!("request", request_id = %request_id);
    let _enter = span.enter();

    // Call next middleware/handler
    let mut response = next.run(req).await;

    // Add request ID to response headers
    if let Ok(header_value) = HeaderValue::from_str(&request_id.to_string()) {
        response.headers_mut().insert(X_REQUEST_ID, header_value);
    }

    response
}
