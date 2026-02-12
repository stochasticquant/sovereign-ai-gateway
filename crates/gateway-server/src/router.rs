//! Axum router configuration with middleware stack.

use crate::{handlers, middleware, state::AppState};
use axum::{
    Router,
    http::StatusCode,
    middleware as axum_middleware,
    routing::{get, post},
};
use std::time::Duration;
use tower_http::{cors::CorsLayer, timeout::TimeoutLayer, trace::TraceLayer};

/// Build the main application router with all routes and middleware.
pub fn build_router(state: AppState) -> Router {
    // Health check routes (no authentication required)
    let health_routes = Router::new()
        .route("/health", get(handlers::health::health))
        .route("/ready", get(handlers::health::ready))
        .route("/metrics", get(handlers::health::metrics));

    // API routes with authentication and quota enforcement
    // Middleware order (from outer to inner): auth → quota → handler
    let api_routes = Router::new()
        .route("/chat/completions", post(handlers::chat::chat_completions))
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            middleware::quota::quota_middleware,
        ))
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            middleware::auth::auth_middleware,
        ));

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
