mod db;
mod handlers;
pub mod metrics;
mod middleware;
mod router;
mod shutdown;
mod state;
mod telemetry;

use gateway_core::config::GatewayConfig;
use metrics_exporter_prometheus::PrometheusBuilder;
use state::AppState;
use std::net::SocketAddr;
use tracing::{error, info};

#[tokio::main]
async fn main() {
    // Load configuration first (needed for telemetry init)
    let config = match GatewayConfig::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    // Initialize telemetry (tracing + optional OpenTelemetry)
    let otel_provider = telemetry::init_telemetry(&config.telemetry);

    info!("Sovereign AI Gateway starting up");
    info!("Configuration loaded successfully");

    // Initialize Prometheus metrics
    let metrics_handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("Failed to install Prometheus recorder");

    // Initialize database connection pool
    let db_pool = match db::init_pool(&config.database).await {
        Ok(pool) => pool,
        Err(e) => {
            error!("Failed to initialize database pool: {}", e);
            std::process::exit(1);
        }
    };

    // Create shared application state (initializes audit writer, rate limiter, etc.)
    let app_state = match AppState::new(db_pool, config.clone(), metrics_handle).await {
        Ok(state) => state,
        Err(e) => {
            error!("Failed to initialize application state: {}", e);
            std::process::exit(1);
        }
    };

    // Build router
    let app = router::build_router(app_state);

    // Bind to address
    let addr = SocketAddr::from((
        config
            .server
            .host
            .parse::<std::net::IpAddr>()
            .unwrap_or_else(|_| {
                error!("Invalid host address in configuration");
                std::process::exit(1);
            }),
        config.server.port,
    ));

    info!("Starting HTTP server on {}", addr);

    // Start server with graceful shutdown
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind to address");

    info!("Sovereign AI Gateway ready and listening on {}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown::shutdown_signal())
        .await
        .expect("Server error");

    // Flush telemetry on shutdown
    telemetry::shutdown_telemetry(otel_provider);

    info!("Sovereign AI Gateway shut down gracefully");
}
