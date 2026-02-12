//! Shared application state.

use gateway_core::config::GatewayConfig;
use metrics_exporter_prometheus::PrometheusHandle;
use sqlx::PgPool;
use std::sync::Arc;
use token_governor::{QuotaEnforcer, TokenCounter};

/// Shared state passed to all handlers and middleware.
#[derive(Clone)]
pub struct AppState {
    /// Database connection pool.
    pub db: PgPool,

    /// Gateway configuration.
    pub config: Arc<GatewayConfig>,

    /// Prometheus metrics registry handle.
    pub metrics: PrometheusHandle,

    /// Token usage counter.
    pub token_counter: Arc<TokenCounter>,

    /// Quota enforcement service.
    pub quota_enforcer: Arc<QuotaEnforcer>,
}

impl AppState {
    pub fn new(db: PgPool, config: GatewayConfig, metrics: PrometheusHandle) -> Self {
        let token_counter = Arc::new(TokenCounter::new(db.clone()));
        let quota_enforcer = Arc::new(QuotaEnforcer::new(db.clone()));

        Self {
            db,
            config: Arc::new(config),
            metrics,
            token_counter,
            quota_enforcer,
        }
    }
}
