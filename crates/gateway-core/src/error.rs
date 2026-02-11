use thiserror::Error;

/// Unified error type for the Sovereign AI Gateway.
///
/// Each variant carries a structured error code suitable for API responses
/// and audit logging.
#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("authorization denied: {0}")]
    AuthorizationDenied(String),

    #[error("policy violation: {0}")]
    PolicyViolation(String),

    #[error("context firewall blocked request: {0}")]
    FirewallBlocked(String),

    #[error("provider error: {0}")]
    ProviderError(String),

    #[error("rate limit exceeded: {0}")]
    RateLimitExceeded(String),

    #[error("token quota exceeded: {0}")]
    QuotaExceeded(String),

    #[error("configuration error: {0}")]
    ConfigError(String),

    #[error("database connection error: {0}")]
    DatabaseConnection(String),

    #[error("internal error: {0}")]
    Internal(String),

    #[error("request validation error: {0}")]
    ValidationError(String),

    #[error("not found: {0}")]
    NotFound(String),
}

impl GatewayError {
    /// Returns the HTTP status code associated with this error.
    pub fn status_code(&self) -> u16 {
        match self {
            Self::AuthenticationFailed(_) => 401,
            Self::AuthorizationDenied(_) => 403,
            Self::PolicyViolation(_) => 403,
            Self::FirewallBlocked(_) => 403,
            Self::RateLimitExceeded(_) => 429,
            Self::QuotaExceeded(_) => 429,
            Self::ValidationError(_) => 400,
            Self::NotFound(_) => 404,
            Self::ProviderError(_) => 502,
            Self::ConfigError(_) => 500,
            Self::DatabaseConnection(_) => 503,
            Self::Internal(_) => 500,
        }
    }

    /// Returns a machine-readable error code for API responses.
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::AuthenticationFailed(_) => "AUTHENTICATION_FAILED",
            Self::AuthorizationDenied(_) => "AUTHORIZATION_DENIED",
            Self::PolicyViolation(_) => "POLICY_VIOLATION",
            Self::FirewallBlocked(_) => "FIREWALL_BLOCKED",
            Self::ProviderError(_) => "PROVIDER_ERROR",
            Self::RateLimitExceeded(_) => "RATE_LIMIT_EXCEEDED",
            Self::QuotaExceeded(_) => "QUOTA_EXCEEDED",
            Self::ConfigError(_) => "CONFIG_ERROR",
            Self::DatabaseConnection(_) => "DATABASE_CONNECTION_ERROR",
            Self::Internal(_) => "INTERNAL_ERROR",
            Self::ValidationError(_) => "VALIDATION_ERROR",
            Self::NotFound(_) => "NOT_FOUND",
        }
    }
}

/// Convenience Result type for gateway operations.
pub type GatewayResult<T> = Result<T, GatewayError>;
