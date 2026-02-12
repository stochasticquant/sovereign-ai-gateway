//! Shared application state.

use audit_log::{AuditWriter, AuditWriterConfig};
use gateway_core::config::GatewayConfig;
use metrics_exporter_prometheus::PrometheusHandle;
use provider_adapters::registry::ProviderRegistry;
use sqlx::PgPool;
use std::sync::Arc;
use token_governor::{QuotaEnforcer, TokenCounter};

use crate::middleware::rate_limit::RateLimiter;

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

    /// Audit log writer (non-blocking, channel-based).
    pub audit_writer: Arc<AuditWriter>,

    /// In-memory rate limiter.
    pub rate_limiter: Arc<RateLimiter>,

    /// LLM provider registry with health monitoring.
    pub provider_registry: Arc<ProviderRegistry>,
}

impl AppState {
    pub async fn new(
        db: PgPool,
        config: GatewayConfig,
        metrics: PrometheusHandle,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let token_counter = Arc::new(TokenCounter::new(db.clone()));
        let quota_enforcer = Arc::new(QuotaEnforcer::new(db.clone()));
        let audit_writer =
            Arc::new(AuditWriter::new(db.clone(), AuditWriterConfig::default()).await?);
        let rate_limiter = Arc::new(RateLimiter::new());
        let provider_registry = Arc::new(ProviderRegistry::new(Default::default()));

        Ok(Self {
            db,
            config: Arc::new(config),
            metrics,
            token_counter,
            quota_enforcer,
            audit_writer,
            rate_limiter,
            provider_registry,
        })
    }
}
