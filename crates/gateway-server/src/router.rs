//! Axum router configuration with middleware stack.

use crate::{handlers, middleware, state::AppState};
use axum::{
    http::StatusCode,
    middleware as axum_middleware,
    routing::get,
    Router,
};
use std::time::Duration;
use tower_http::{
    cors::CorsLayer,
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

/// Build the main application router with all routes and middleware.
pub fn build_router(state: AppState) -> Router {
    // Health check routes (no authentication required)
    let health_routes = Router::new()
        .route("/health", get(handlers::health::health))
        .route("/ready", get(handlers::health::ready))
        .route("/metrics", get(handlers::health::metrics));

    // API routes (will add authentication later)
    // TODO(phase-2): Add /v1/chat/completions, /v1/embeddings, etc.
    let api_routes = Router::new();

    // Admin routes (will add authentication later)
    // TODO(phase-2): Add admin endpoints for tenants, keys, policies
    let admin_routes = Router::new();

    // Combine all routes and apply middleware
    // Note: Middleware is applied in reverse order (bottom-up)
    Router::new()
        .merge(health_routes)
        .nest("/v1", api_routes)
        .nest("/admin", admin_routes)
        .with_state(state)
        // Apply middleware in reverse order
        .layer(axum_middleware::from_fn(middleware::request_id::request_id_middleware))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_secs(30)))
}
