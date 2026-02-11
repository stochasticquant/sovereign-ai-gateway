use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use std::env;

/// Top-level gateway configuration, loaded from layered sources:
/// default.toml → environment-specific file → env vars → CLI flags.
#[derive(Debug, Clone, Deserialize)]
pub struct GatewayConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub providers: ProvidersConfig,
    pub firewall: FirewallConfig,
    pub policy: PolicyConfig,
    pub telemetry: TelemetryConfig,
}

impl GatewayConfig {
    /// Load configuration with layered precedence:
    /// 1. config/default.toml (base)
    /// 2. config/{environment}.toml (if GATEWAY_ENV is set)
    /// 3. Environment variables with GATEWAY_ prefix
    pub fn load() -> Result<Self, ConfigError> {
        let environment = env::var("GATEWAY_ENV").unwrap_or_else(|_| "development".to_string());

        let config = Config::builder()
            // Start with default config
            .add_source(File::with_name("config/default").required(true))
            // Layer environment-specific config (optional)
            .add_source(File::with_name(&format!("config/{}", environment)).required(false))
            // Override with environment variables (GATEWAY_SERVER__PORT=8080 → server.port)
            .add_source(
                Environment::with_prefix("GATEWAY")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?;

        config.try_deserialize()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub request_timeout_secs: u64,
    pub max_request_body_bytes: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProvidersConfig {
    pub default_provider: String,
    pub request_timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FirewallConfig {
    pub enabled: bool,
    pub block_on_detection: bool,
    pub redaction_mode: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PolicyConfig {
    pub policy_dir: String,
    pub hot_reload: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelemetryConfig {
    pub log_level: String,
    pub log_format: String,
    pub metrics_enabled: bool,
    pub otlp_endpoint: Option<String>,
}
