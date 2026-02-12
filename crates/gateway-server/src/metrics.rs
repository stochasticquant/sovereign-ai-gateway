//! Prometheus metrics definitions and recording helpers.
//!
//! Provides metric name constants and helper functions for recording
//! gateway metrics. The PrometheusBuilder recorder is installed in main.rs,
//! so all metrics!() macros automatically feed into the Prometheus exporter.

use metrics::{counter, gauge, histogram};

// ── Metric name constants ──

/// Total requests processed (counter).
/// Labels: tenant_id, provider, model, status
pub const REQUESTS_TOTAL: &str = "gateway_requests_total";

/// Request latency in milliseconds (histogram).
/// Labels: provider, model
pub const REQUEST_LATENCY_MS: &str = "gateway_request_latency_ms";

/// Total tokens consumed (counter).
/// Labels: provider, model, type (prompt|completion)
pub const TOKENS_TOTAL: &str = "gateway_tokens_total";

/// Blocked requests (counter).
/// Labels: reason (auth_failed|quota_exceeded|rate_limited)
pub const BLOCKED_REQUESTS_TOTAL: &str = "gateway_blocked_requests_total";

/// Estimated cost in USD (counter, incremented per request).
/// Labels: provider, model
pub const ESTIMATED_COST_USD: &str = "gateway_estimated_cost_usd";

/// Audit entries written (counter).
pub const AUDIT_ENTRIES_TOTAL: &str = "gateway_audit_entries_total";

/// Circuit breaker state (gauge: 0=closed, 1=half_open, 2=open).
/// Labels: provider
pub const CIRCUIT_BREAKER_STATE: &str = "gateway_circuit_breaker_state";

// ── Recording helpers ──

/// Record a completed request with its metadata.
pub fn record_request(tenant_id: &str, provider: &str, model: &str, status: u16) {
    counter!(REQUESTS_TOTAL,
        "tenant_id" => tenant_id.to_owned(),
        "provider" => provider.to_owned(),
        "model" => model.to_owned(),
        "status" => status.to_string(),
    )
    .increment(1);
}

/// Record request latency.
pub fn record_latency(provider: &str, model: &str, latency_ms: f64) {
    histogram!(REQUEST_LATENCY_MS,
        "provider" => provider.to_owned(),
        "model" => model.to_owned(),
    )
    .record(latency_ms);
}

/// Record token consumption.
pub fn record_tokens(provider: &str, model: &str, token_type: &str, count: u32) {
    counter!(TOKENS_TOTAL,
        "provider" => provider.to_owned(),
        "model" => model.to_owned(),
        "type" => token_type.to_owned(),
    )
    .increment(count as u64);
}

/// Record a blocked request.
pub fn record_blocked(reason: &str) {
    counter!(BLOCKED_REQUESTS_TOTAL,
        "reason" => reason.to_owned(),
    )
    .increment(1);
}

/// Record estimated cost for a request.
pub fn record_cost(provider: &str, model: &str, cost_usd: f64) {
    // Use counter with float increment for cumulative cost tracking
    counter!(ESTIMATED_COST_USD,
        "provider" => provider.to_owned(),
        "model" => model.to_owned(),
    )
    .increment(cost_usd as u64);
}

/// Record an audit entry write.
pub fn record_audit_entry() {
    counter!(AUDIT_ENTRIES_TOTAL).increment(1);
}

/// Record circuit breaker state change.
pub fn record_circuit_breaker(provider: &str, state: u8) {
    gauge!(CIRCUIT_BREAKER_STATE,
        "provider" => provider.to_owned(),
    )
    .set(state as f64);
}
