//! Atomic token counters for per-tenant per-day usage tracking.
//!
//! Uses PostgreSQL UPSERT for atomic increments across horizontal replicas.

use chrono::Utc;
use gateway_core::error::GatewayError;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// Token usage record for a specific tenant, date, provider, and model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub date: chrono::NaiveDate,
    pub provider_id: String,
    pub model: String,
    pub total_tokens: i64,
    pub request_count: i32,
    pub estimated_cost_usd: rust_decimal::Decimal,
}

/// Token usage increment request.
#[derive(Debug, Clone)]
pub struct UsageIncrement {
    pub tenant_id: Uuid,
    pub provider_id: String,
    pub model: String,
    pub tokens: i64,
    pub estimated_cost_usd: f64,
}

/// Token counter service.
pub struct TokenCounter {
    db: PgPool,
}

impl TokenCounter {
    /// Create a new token counter.
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    /// Atomically increment token usage for a tenant.
    ///
    /// Uses INSERT ... ON CONFLICT DO UPDATE for atomic UPSERT.
    /// Returns the updated usage record.
    pub async fn increment(&self, increment: UsageIncrement) -> Result<UsageRecord, GatewayError> {
        let today = Utc::now().date_naive();

        let record = sqlx::query_as!(
            UsageRecord,
            r#"
            INSERT INTO usage_tracking (tenant_id, date, provider_id, model, total_tokens, request_count, estimated_cost_usd)
            VALUES ($1, $2, $3, $4, $5, 1, $6)
            ON CONFLICT (tenant_id, date, provider_id, model)
            DO UPDATE SET
                total_tokens = usage_tracking.total_tokens + $5,
                request_count = usage_tracking.request_count + 1,
                estimated_cost_usd = usage_tracking.estimated_cost_usd + $6,
                updated_at = NOW()
            RETURNING id, tenant_id, date, provider_id, model, total_tokens, request_count, estimated_cost_usd as "estimated_cost_usd: _"
            "#,
            increment.tenant_id,
            today,
            increment.provider_id,
            increment.model,
            increment.tokens,
            rust_decimal::Decimal::from_f64_retain(increment.estimated_cost_usd).unwrap_or_default(),
        )
        .fetch_one(&self.db)
        .await
        .map_err(|e| GatewayError::Internal(format!("Database error: {}", e)))?;

        Ok(record)
    }

    /// Get current usage for a tenant on a specific date.
    pub async fn get_usage(
        &self,
        tenant_id: Uuid,
        date: chrono::NaiveDate,
    ) -> Result<Vec<UsageRecord>, GatewayError> {
        let records = sqlx::query_as!(
            UsageRecord,
            r#"
            SELECT id, tenant_id, date, provider_id, model, total_tokens, request_count, estimated_cost_usd as "estimated_cost_usd: _"
            FROM usage_tracking
            WHERE tenant_id = $1 AND date = $2
            ORDER BY provider_id, model
            "#,
            tenant_id,
            date,
        )
        .fetch_all(&self.db)
        .await
        .map_err(|e| GatewayError::Internal(format!("Database error: {}", e)))?;

        Ok(records)
    }

    /// Get total token usage for a tenant on a specific date (across all providers/models).
    pub async fn get_total_usage(
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

#[cfg(test)]
mod tests {

    #[tokio::test]
    async fn test_increment_upsert() {
        // Integration test - requires database
        // Run with: cargo test --package token-governor -- --ignored
    }
}
