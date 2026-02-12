//! Quota enforcement for token limits and rate limiting.
//!
//! Pre-flight checks to reject requests before they incur cost.
//! Returns 429 Too Many Requests when quotas are exceeded.

use chrono::Utc;
use gateway_core::error::GatewayError;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// Tenant quota configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantQuota {
    pub tenant_id: Uuid,
    pub daily_token_limit: Option<i64>,
    pub max_tokens_per_request: Option<i32>,
    pub max_requests_per_minute: Option<i32>,
    pub max_concurrent_requests: Option<i32>,
}

/// Quota check request.
#[derive(Debug, Clone)]
pub struct QuotaCheckRequest {
    pub tenant_id: Uuid,
    pub requested_tokens: i32,
}

/// Quota check result.
#[derive(Debug, Clone)]
pub enum QuotaCheckResult {
    /// Request is allowed.
    Allowed,
    /// Request exceeds quota.
    Denied {
        reason: String,
        retry_after_secs: Option<u64>,
    },
}

/// Quota enforcement service.
pub struct QuotaEnforcer {
    db: PgPool,
}

impl QuotaEnforcer {
    /// Create a new quota enforcer.
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    /// Check if a request is allowed under the tenant's quota.
    ///
    /// Performs pre-flight checks:
    /// 1. Check if tenant has quotas configured
    /// 2. Check if requested tokens exceed per-request limit
    /// 3. Check if daily token limit would be exceeded
    ///
    /// Returns QuotaCheckResult indicating if the request should be allowed.
    pub async fn check_quota(
        &self,
        request: QuotaCheckRequest,
    ) -> Result<QuotaCheckResult, GatewayError> {
        // Fetch tenant quota configuration
        let quota = self.get_quota(request.tenant_id).await?;

        // If no quota is configured, allow the request
        let Some(quota) = quota else {
            return Ok(QuotaCheckResult::Allowed);
        };

        // Check per-request token limit
        if let Some(max_tokens_per_request) = quota.max_tokens_per_request {
            if request.requested_tokens > max_tokens_per_request {
                return Ok(QuotaCheckResult::Denied {
                    reason: format!(
                        "Request exceeds per-request token limit: {} > {}",
                        request.requested_tokens, max_tokens_per_request
                    ),
                    retry_after_secs: None,
                });
            }
        }

        // Check daily token limit
        if let Some(daily_limit) = quota.daily_token_limit {
            let today = Utc::now().date_naive();
            let current_usage = self.get_daily_usage(request.tenant_id, today).await?;

            let projected_usage = current_usage + request.requested_tokens as i64;
            if projected_usage > daily_limit {
                let remaining = daily_limit.saturating_sub(current_usage);
                return Ok(QuotaCheckResult::Denied {
                    reason: format!(
                        "Request would exceed daily token limit: {} + {} > {} (remaining: {})",
                        current_usage, request.requested_tokens, daily_limit, remaining
                    ),
                    retry_after_secs: Some(seconds_until_midnight()),
                });
            }
        }

        Ok(QuotaCheckResult::Allowed)
    }

    /// Get quota configuration for a tenant.
    pub async fn get_quota(&self, tenant_id: Uuid) -> Result<Option<TenantQuota>, GatewayError> {
        let quota = sqlx::query_as!(
            TenantQuota,
            r#"
            SELECT tenant_id, daily_token_limit, max_tokens_per_request, max_requests_per_minute, max_concurrent_requests
            FROM tenant_quotas
            WHERE tenant_id = $1
            "#,
            tenant_id,
        )
        .fetch_optional(&self.db)
        .await
        .map_err(|e| GatewayError::Internal(format!("Database error: {}", e)))?;

        Ok(quota)
    }

    /// Set quota configuration for a tenant.
    pub async fn set_quota(&self, quota: TenantQuota) -> Result<(), GatewayError> {
        sqlx::query!(
            r#"
            INSERT INTO tenant_quotas (tenant_id, daily_token_limit, max_tokens_per_request, max_requests_per_minute, max_concurrent_requests)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (tenant_id)
            DO UPDATE SET
                daily_token_limit = $2,
                max_tokens_per_request = $3,
                max_requests_per_minute = $4,
                max_concurrent_requests = $5,
                updated_at = NOW()
            "#,
            quota.tenant_id,
            quota.daily_token_limit,
            quota.max_tokens_per_request,
            quota.max_requests_per_minute,
            quota.max_concurrent_requests,
        )
        .execute(&self.db)
        .await
        .map_err(|e| GatewayError::Internal(format!("Database error: {}", e)))?;

        Ok(())
    }

    /// Get current daily token usage for a tenant.
    async fn get_daily_usage(
        &self,
        tenant_id: Uuid,
        date: chrono::NaiveDate,
    ) -> Result<i64, GatewayError> {
        let result = sqlx::query!(
            r#"
            SELECT COALESCE(SUM(total_tokens), 0) as "total!: i64"
            FROM usage_tracking
            WHERE tenant_id = $1 AND date = $2
            "#,
            tenant_id,
            date,
        )
        .fetch_one(&self.db)
        .await
        .map_err(|e| GatewayError::Internal(format!("Database error: {}", e)))?;

        Ok(result.total)
    }
}

/// Calculate seconds until midnight UTC (for retry-after header).
fn seconds_until_midnight() -> u64 {
    let now = Utc::now();
    let tomorrow = (now + chrono::Duration::days(1))
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let midnight = tomorrow.and_utc();
    (midnight - now).num_seconds().max(0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seconds_until_midnight() {
        let seconds = seconds_until_midnight();
        assert!(seconds > 0);
        assert!(seconds <= 86400); // Max 24 hours
    }
}
