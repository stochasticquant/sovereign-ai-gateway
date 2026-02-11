//! Health check and metrics endpoints for observability.

use crate::state::AppState;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use tracing::warn;

/// GET /health - Liveness probe
/// Always returns 200 OK if the process is running.
/// Used by Kubernetes/Docker health checks to determine if container should be restarted.
pub async fn health() -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "service": "sovereign-ai-gateway"
    }))
}

/// GET /ready - Readiness probe
/// Returns 200 if service is ready to accept traffic (DB connected, etc.)
/// Returns 503 if service is not ready.
/// Used by Kubernetes to determine if pod should receive traffic.
pub async fn ready(State(state): State<AppState>) -> Response {
    // Check database connectivity
    let db_healthy = match sqlx::query("SELECT 1")
        .fetch_one(&state.db)
        .await
    {
        Ok(_) => true,
        Err(e) => {
            warn!("Database health check failed: {}", e);
            false
        }
    };

    if db_healthy {
        (
            StatusCode::OK,
            Json(json!({
                "status": "ready",
                "checks": {
                    "database": "ok"
                }
            })),
        )
            .into_response()
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "status": "not_ready",
                "checks": {
                    "database": "failed"
                }
            })),
        )
            .into_response()
    }
}

/// GET /metrics - Prometheus metrics endpoint
/// Returns metrics in Prometheus text format for scraping.
pub async fn metrics(State(state): State<AppState>) -> Response {
    let metrics_text = state.metrics.render();
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4")],
        metrics_text,
    )
        .into_response()
}
