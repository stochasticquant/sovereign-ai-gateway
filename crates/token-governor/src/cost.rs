//! Cost calculation for LLM requests.
//!
//! Bridges provider-specific pricing from `provider_adapters::cost` into the
//! token governor for per-tenant cost tracking and reporting.

use chrono::NaiveDate;
use gateway_core::error::{GatewayError, GatewayResult};
use provider_adapters::cost::{
    get_anthropic_pricing, get_azure_pricing, get_local_pricing, get_openai_pricing,
};
use provider_adapters::traits::Usage;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::instrument;
use uuid::Uuid;

/// Cost calculator that maps provider+model+usage to USD cost,
/// and queries aggregated cost data from the usage_tracking table.
pub struct CostCalculator {
    db: PgPool,
}

/// Cost breakdown for a specific provider and model over a date range.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostBreakdownEntry {
    pub provider_id: String,
    pub model: String,
    pub total_tokens: i64,
    pub request_count: i32,
    pub total_cost_usd: f64,
}

impl CostCalculator {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    /// Calculate cost for a completed request based on provider pricing.
    ///
    /// Returns 0.0 if no pricing data is available for the provider/model.
    pub fn calculate(provider_id: &str, model: &str, usage: &Usage) -> f64 {
        let pricing = match provider_id {
            "openai" => get_openai_pricing(model),
            "anthropic" => get_anthropic_pricing(model),
            "azure" => get_azure_pricing(model),
            "local" => get_local_pricing(model),
            _ => None,
        };

        match pricing {
            Some(p) => p.calculate_cost(usage),
            None => {
                tracing::warn!(
                    provider_id,
                    model,
                    "No pricing data for provider/model, using zero cost"
                );
                0.0
            }
        }
    }

    /// Get total estimated cost for a tenant on a specific date.
    #[instrument(skip(self))]
    pub async fn get_daily_cost(&self, tenant_id: Uuid, date: NaiveDate) -> GatewayResult<f64> {
        let row: (rust_decimal::Decimal,) = sqlx::query_as(
            "SELECT COALESCE(SUM(estimated_cost_usd), 0) FROM usage_tracking WHERE tenant_id = $1 AND date = $2",
        )
        .bind(tenant_id)
        .bind(date)
        .fetch_one(&self.db)
        .await
        .map_err(|e| GatewayError::Internal(format!("Failed to query daily cost: {}", e)))?;

        Ok(rust_decimal::prelude::ToPrimitive::to_f64(&row.0).unwrap_or(0.0))
    }

    /// Get cost breakdown by provider and model for a tenant over a date range.
    #[instrument(skip(self))]
    pub async fn get_cost_breakdown(
        &self,
        tenant_id: Uuid,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> GatewayResult<Vec<CostBreakdownEntry>> {
        let rows: Vec<CostBreakdownRow> = sqlx::query_as(
            "SELECT provider_id, model, \
             COALESCE(SUM(total_tokens), 0), \
             COALESCE(SUM(request_count), 0), \
             COALESCE(SUM(estimated_cost_usd), 0) \
             FROM usage_tracking \
             WHERE tenant_id = $1 AND date >= $2 AND date <= $3 \
             GROUP BY provider_id, model \
             ORDER BY SUM(estimated_cost_usd) DESC",
        )
        .bind(tenant_id)
        .bind(start_date)
        .bind(end_date)
        .fetch_all(&self.db)
        .await
        .map_err(|e| GatewayError::Internal(format!("Failed to query cost breakdown: {}", e)))?;

        Ok(rows
            .into_iter()
            .map(|row| CostBreakdownEntry {
                provider_id: row.provider_id,
                model: row.model,
                total_tokens: row.total_tokens,
                request_count: row.request_count as i32,
                total_cost_usd: rust_decimal::prelude::ToPrimitive::to_f64(&row.total_cost_usd)
                    .unwrap_or(0.0),
            })
            .collect())
    }
}

/// Internal row type for cost breakdown queries.
#[derive(sqlx::FromRow)]
struct CostBreakdownRow {
    provider_id: String,
    model: String,
    total_tokens: i64,
    request_count: i64,
    total_cost_usd: rust_decimal::Decimal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_openai_cost() {
        let usage = Usage {
            prompt_tokens: 1000,
            completion_tokens: 500,
            total_tokens: 1500,
        };
        let cost = CostCalculator::calculate("openai", "gpt-4o-mini", &usage);
        // (1000/1000 * 0.00015) + (500/1000 * 0.0006) = 0.00015 + 0.0003 = 0.00045
        assert!((cost - 0.00045).abs() < 0.000001);
    }

    #[test]
    fn test_calculate_anthropic_cost() {
        let usage = Usage {
            prompt_tokens: 2000,
            completion_tokens: 1000,
            total_tokens: 3000,
        };
        let cost = CostCalculator::calculate("anthropic", "claude-3-haiku-20240307", &usage);
        // (2000/1000 * 0.00025) + (1000/1000 * 0.00125) = 0.0005 + 0.00125 = 0.00175
        assert!((cost - 0.00175).abs() < 0.000001);
    }

    #[test]
    fn test_calculate_local_cost_is_zero() {
        let usage = Usage {
            prompt_tokens: 5000,
            completion_tokens: 2000,
            total_tokens: 7000,
        };
        let cost = CostCalculator::calculate("local", "llama-3-70b", &usage);
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn test_calculate_azure_uses_openai_pricing() {
        let usage = Usage {
            prompt_tokens: 1000,
            completion_tokens: 500,
            total_tokens: 1500,
        };
        let openai_cost = CostCalculator::calculate("openai", "gpt-4o", &usage);
        let azure_cost = CostCalculator::calculate("azure", "gpt-4o", &usage);
        assert_eq!(openai_cost, azure_cost);
    }

    #[test]
    fn test_calculate_unknown_provider_returns_zero() {
        let usage = Usage {
            prompt_tokens: 1000,
            completion_tokens: 500,
            total_tokens: 1500,
        };
        let cost = CostCalculator::calculate("unknown-provider", "some-model", &usage);
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn test_calculate_unknown_model_returns_zero() {
        let usage = Usage {
            prompt_tokens: 1000,
            completion_tokens: 500,
            total_tokens: 1500,
        };
        let cost = CostCalculator::calculate("openai", "nonexistent-model-xyz", &usage);
        assert_eq!(cost, 0.0);
    }
}
