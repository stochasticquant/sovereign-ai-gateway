//! Database connection pool management.

use gateway_core::config::DatabaseConfig;
use gateway_core::error::GatewayError;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;
use tracing::{info, instrument};

/// Initialize PostgreSQL connection pool from configuration.
#[instrument(skip(config), fields(url = %redact_password(&config.url)))]
pub async fn init_pool(config: &DatabaseConfig) -> Result<PgPool, GatewayError> {
    info!(
        "Initializing database pool (min={}, max={})",
        config.min_connections, config.max_connections
    );

    let pool = PgPoolOptions::new()
        .min_connections(config.min_connections)
        .max_connections(config.max_connections)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.url)
        .await
        .map_err(|e| {
            GatewayError::DatabaseConnection(format!("Failed to connect to database: {}", e))
        })?;

    info!("Database pool initialized successfully");
    Ok(pool)
}

/// Redact password from database URL for logging.
fn redact_password(url: &str) -> String {
    if let Some(at_pos) = url.rfind('@') {
        if let Some(colon_pos) = url[..at_pos].rfind(':') {
            let mut redacted = url.to_string();
            redacted.replace_range(colon_pos + 1..at_pos, "****");
            return redacted;
        }
    }
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_password() {
        let url = "postgres://user:secret123@localhost:5432/dbname";
        let redacted = redact_password(url);
        assert_eq!(redacted, "postgres://user:****@localhost:5432/dbname");
    }

    #[test]
    fn test_redact_password_no_password() {
        let url = "postgres://localhost:5432/dbname";
        let redacted = redact_password(url);
        assert_eq!(redacted, "postgres://localhost:5432/dbname");
    }
}
