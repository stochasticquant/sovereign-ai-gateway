//! Shared application state.

use gateway_core::config::GatewayConfig;
use metrics_exporter_prometheus::PrometheusHandle;
use sqlx::PgPool;
use std::sync::Arc;

/// Shared state passed to all handlers and middleware.
#[derive(Clone)]
pub struct AppState {
    /// Database connection pool.
    pub db: PgPool,

    /// Gateway configuration.
    pub config: Arc<GatewayConfig>,

    /// Prometheus metrics registry handle.
    pub metrics: PrometheusHandle,
}

impl AppState {
    pub fn new(db: PgPool, config: GatewayConfig, metrics: PrometheusHandle) -> Self {
        Self {
            db,
            config: Arc::new(config),
            metrics,
        }
    }
}
