//! Sliding window rate limiter middleware.
//!
//! Per-tenant rate limits backed by an in-memory sliding window.
//! Returns 429 with Retry-After header when exceeded.

use axum::{
    extract::{Request, State},
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::state::AppState;

/// In-memory sliding window rate limiter.
///
/// Tracks request timestamps per tenant and enforces
/// `max_requests_per_minute` limits from the tenant quota config.
#[derive(Clone)]
pub struct RateLimiter {
    windows: Arc<RwLock<HashMap<Uuid, Vec<Instant>>>>,
    window_duration: Duration,
}

impl RateLimiter {
    /// Create a new rate limiter and start its background cleanup task.
    pub fn new() -> Self {
        let limiter = Self {
            windows: Arc::new(RwLock::new(HashMap::new())),
            window_duration: Duration::from_secs(60),
        };

        // Spawn background cleanup task
        let cleanup_limiter = limiter.clone();
        tokio::spawn(async move {
            cleanup_limiter.cleanup_task().await;
        });

        limiter
    }

    /// Check if a request is allowed under the rate limit.
    ///
    /// Returns `Ok(())` if allowed, or `Err(retry_after_secs)` if denied.
    pub async fn check_rate_limit(
        &self,
        tenant_id: Uuid,
        max_requests_per_minute: i32,
    ) -> Result<(), u64> {
        let now = Instant::now();
        let cutoff = now - self.window_duration;

        let mut windows = self.windows.write().await;
        let timestamps = windows.entry(tenant_id).or_default();

        // Remove expired timestamps
        timestamps.retain(|t| *t > cutoff);

        // Check if over limit
        if timestamps.len() >= max_requests_per_minute as usize {
            let oldest = timestamps.first().copied().unwrap_or(now);
            let retry_after = self
                .window_duration
                .saturating_sub(now.duration_since(oldest))
                .as_secs()
                .max(1);
            return Err(retry_after);
        }

        // Record this request
        timestamps.push(now);
        Ok(())
    }

    /// Background task to periodically clean up expired entries.
    async fn cleanup_task(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            let mut windows = self.windows.write().await;
            let now = Instant::now();
            let cutoff = now - self.window_duration;
            windows.retain(|_, timestamps| {
                timestamps.retain(|t| *t > cutoff);
                !timestamps.is_empty()
            });
        }
    }
}

/// Middleware that enforces per-tenant request rate limits.
///
/// Checks `max_requests_per_minute` from the tenant's quota config.
/// Returns 429 Too Many Requests with Retry-After header when exceeded.
pub async fn rate_limit_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let tenant_id = match req.extensions().get::<Uuid>() {
        Some(id) => *id,
        None => return next.run(req).await,
    };

    // Fetch tenant quota to get max_requests_per_minute
    let quota = match state.quota_enforcer.get_quota(tenant_id).await {
        Ok(Some(q)) => q,
        Ok(None) => return next.run(req).await, // No quota configured = no rate limit
        Err(e) => {
            tracing::error!(
                tenant_id = %tenant_id,
                error = %e,
                "Failed to fetch quota for rate limiting"
            );
            // Fail closed on error
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to check rate limit",
            )
                .into_response();
        }
    };

    if let Some(max_rpm) = quota.max_requests_per_minute {
        match state
            .rate_limiter
            .check_rate_limit(tenant_id, max_rpm)
            .await
        {
            Ok(()) => next.run(req).await,
            Err(retry_after) => {
                tracing::warn!(
                    tenant_id = %tenant_id,
                    max_rpm,
                    "Rate limit exceeded"
                );

                crate::metrics::record_blocked("rate_limited");

                let mut response = (
                    StatusCode::TOO_MANY_REQUESTS,
                    "Rate limit exceeded. Try again later.",
                )
                    .into_response();

                if let Ok(header_value) = HeaderValue::from_str(&retry_after.to_string()) {
                    response.headers_mut().insert("retry-after", header_value);
                }

                response
            }
        }
    } else {
        next.run(req).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_allows_under_limit() {
        let limiter = RateLimiter {
            windows: Arc::new(RwLock::new(HashMap::new())),
            window_duration: Duration::from_secs(60),
        };
        let tenant_id = Uuid::now_v7();

        for _ in 0..5 {
            assert!(limiter.check_rate_limit(tenant_id, 10).await.is_ok());
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_denies_over_limit() {
        let limiter = RateLimiter {
            windows: Arc::new(RwLock::new(HashMap::new())),
            window_duration: Duration::from_secs(60),
        };
        let tenant_id = Uuid::now_v7();

        // Fill up the limit
        for _ in 0..5 {
            assert!(limiter.check_rate_limit(tenant_id, 5).await.is_ok());
        }

        // Next request should be denied
        let result = limiter.check_rate_limit(tenant_id, 5).await;
        assert!(result.is_err());
        let retry_after = result.unwrap_err();
        assert!(retry_after > 0);
        assert!(retry_after <= 60);
    }

    #[tokio::test]
    async fn test_rate_limiter_isolates_tenants() {
        let limiter = RateLimiter {
            windows: Arc::new(RwLock::new(HashMap::new())),
            window_duration: Duration::from_secs(60),
        };
        let tenant_a = Uuid::now_v7();
        let tenant_b = Uuid::now_v7();

        // Fill tenant A's limit
        for _ in 0..3 {
            limiter.check_rate_limit(tenant_a, 3).await.ok();
        }
        assert!(limiter.check_rate_limit(tenant_a, 3).await.is_err());

        // Tenant B should still be allowed
        assert!(limiter.check_rate_limit(tenant_b, 3).await.is_ok());
    }

    #[tokio::test]
    async fn test_rate_limiter_expired_entries_cleaned() {
        let limiter = RateLimiter {
            windows: Arc::new(RwLock::new(HashMap::new())),
            // Use a very short window for testing
            window_duration: Duration::from_millis(50),
        };
        let tenant_id = Uuid::now_v7();

        // Fill the limit
        for _ in 0..3 {
            limiter.check_rate_limit(tenant_id, 3).await.ok();
        }
        assert!(limiter.check_rate_limit(tenant_id, 3).await.is_err());

        // Wait for entries to expire
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Should be allowed again
        assert!(limiter.check_rate_limit(tenant_id, 3).await.is_ok());
    }
}
