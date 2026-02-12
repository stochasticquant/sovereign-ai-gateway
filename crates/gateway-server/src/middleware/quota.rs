//! Quota enforcement middleware for token limits.
//!
//! Pre-flight checks to reject requests that would exceed quotas.

use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use http_body_util::BodyExt;
use serde_json::Value;
use token_governor::{QuotaCheckRequest, QuotaCheckResult};
use uuid::Uuid;

use crate::state::AppState;

/// Header name for retry-after in seconds.
pub const RETRY_AFTER: &str = "retry-after";

/// Middleware that enforces quota limits before processing chat requests.
///
/// This middleware:
/// 1. Extracts tenant_id from request extensions (set by auth middleware)
/// 2. Estimates requested tokens from the request body
/// 3. Checks if the request would exceed quotas
/// 4. Returns 429 Too Many Requests if quota would be exceeded
/// 5. Allows the request to proceed if within quota
pub async fn quota_middleware(State(state): State<AppState>, req: Request, next: Next) -> Response {
    // Extract tenant_id from extensions (set by auth middleware)
    let tenant_id = match req.extensions().get::<Uuid>() {
        Some(id) => *id,
        None => {
            // If no tenant_id, skip quota check (auth middleware should have rejected)
            tracing::warn!("No tenant_id in request extensions, skipping quota check");
            return next.run(req).await;
        }
    };

    // Only apply quota checks to chat completion endpoints
    let path = req.uri().path();
    if !path.contains("/chat/completions") {
        return next.run(req).await;
    }

    // Read the body to estimate tokens
    let (parts, body) = req.into_parts();
    let bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(e) => {
            tracing::error!("Failed to read request body: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to read request body",
            )
                .into_response();
        }
    };

    // Estimate tokens from request body
    let estimated_tokens = estimate_tokens_from_body(&bytes);

    // Check quota
    let quota_check = QuotaCheckRequest {
        tenant_id,
        requested_tokens: estimated_tokens,
    };

    match state.quota_enforcer.check_quota(quota_check).await {
        Ok(QuotaCheckResult::Allowed) => {
            // Quota check passed, reconstruct request and continue
            let req = Request::from_parts(parts, Body::from(bytes));
            next.run(req).await
        }
        Ok(QuotaCheckResult::Denied {
            reason,
            retry_after_secs,
        }) => {
            tracing::warn!(
                tenant_id = %tenant_id,
                reason = %reason,
                "Quota exceeded"
            );

            crate::metrics::record_blocked("quota_exceeded");

            // Return 429 Too Many Requests
            let mut response = (StatusCode::TOO_MANY_REQUESTS, reason).into_response();

            // Add Retry-After header if available
            if let Some(secs) = retry_after_secs {
                if let Ok(header_value) = HeaderValue::from_str(&secs.to_string()) {
                    response.headers_mut().insert(RETRY_AFTER, header_value);
                }
            }

            response
        }
        Err(e) => {
            tracing::error!(
                tenant_id = %tenant_id,
                error = %e,
                "Failed to check quota"
            );
            // On error, fail closed - reject the request
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to check quota").into_response()
        }
    }
}

/// Estimate tokens from request body using heuristic-based estimation.
///
/// Parses the request body to extract messages and max_tokens,
/// then uses `TokenEstimator` for a character-based estimate.
fn estimate_tokens_from_body(body: &Bytes) -> i32 {
    let json: Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => return 2048,
    };

    let model = json
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let max_tokens = json
        .get("max_tokens")
        .and_then(|v| v.as_u64())
        .map(|v| v.min(u32::MAX as u64) as u32);

    // Extract messages as (role, content) pairs for the estimator
    let messages: Vec<(String, String)> = json
        .get("messages")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|msg| {
                    let role = msg.get("role")?.as_str()?.to_string();
                    let content = msg.get("content")?.as_str()?.to_string();
                    Some((role, content))
                })
                .collect()
        })
        .unwrap_or_default();

    if messages.is_empty() {
        return max_tokens.unwrap_or(2048) as i32;
    }

    token_governor::TokenEstimator::estimate_total_tokens(model, &messages, max_tokens) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens_with_messages_and_max_tokens() {
        let body = br#"{"model": "gpt-4", "max_tokens": 1000, "messages": [{"role": "user", "content": "Hello, world!"}]}"#;
        let bytes = Bytes::from_static(body);
        let estimate = estimate_tokens_from_body(&bytes);
        // 13 chars → ceil(13/4)=4 + 4 overhead = 8, + 3 base = 11 prompt + 1000 completion = 1011
        assert_eq!(estimate, 1011);
    }

    #[test]
    fn test_estimate_tokens_with_messages_no_max_tokens() {
        let body = br#"{"model": "gpt-4o", "messages": [{"role": "user", "content": "Hi"}]}"#;
        let bytes = Bytes::from_static(body);
        let estimate = estimate_tokens_from_body(&bytes);
        // "Hi" = 2 chars → ceil(2/4)=1 + 4 = 5, + 3 base = 8 prompt + 4096 default = 4104
        assert_eq!(estimate, 4104);
    }

    #[test]
    fn test_estimate_tokens_no_messages_with_max_tokens() {
        let body = br#"{"model": "gpt-4", "max_tokens": 500}"#;
        let bytes = Bytes::from_static(body);
        assert_eq!(estimate_tokens_from_body(&bytes), 500);
    }

    #[test]
    fn test_estimate_tokens_no_messages_no_max_tokens() {
        let body = br#"{"model": "gpt-4"}"#;
        let bytes = Bytes::from_static(body);
        assert_eq!(estimate_tokens_from_body(&bytes), 2048);
    }

    #[test]
    fn test_estimate_tokens_invalid_json() {
        let body = b"not json";
        let bytes = Bytes::from(&body[..]);
        assert_eq!(estimate_tokens_from_body(&bytes), 2048);
    }
}
