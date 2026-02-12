mod db;
mod handlers;
mod middleware;
mod router;
mod shutdown;
mod state;

use gateway_core::config::GatewayConfig;
use metrics_exporter_prometheus::PrometheusBuilder;
use state::AppState;
use std::net::SocketAddr;
use tracing::{error, info};

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sovereign_gateway=debug,tower_http=debug".into()),
        )
        .json()
        .init();

    info!("Sovereign AI Gateway starting up");

    // Load configuration
    let config = match GatewayConfig::load() {
        Ok(cfg) => {
            info!("Configuration loaded successfully");
            cfg
        }
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

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

    // Create shared application state
    let app_state = AppState::new(db_pool, config.clone(), metrics_handle);

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

    info!("Sovereign AI Gateway shut down gracefully");
}
