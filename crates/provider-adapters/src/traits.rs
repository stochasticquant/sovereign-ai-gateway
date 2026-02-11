use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Provider metadata for routing and display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMetadata {
    pub id: String,
    pub name: String,
    pub region: String,
    pub models: Vec<String>,
}

/// Normalized LLM request (provider-agnostic).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub stream: bool,
}

/// A single message in a chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

/// Normalized LLM response (provider-agnostic).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub content: String,
    pub model: String,
    pub usage: Usage,
    pub provider_id: String,
    /// Estimated cost in USD (if available).
    pub estimated_cost_usd: Option<f64>,
}

/// Token usage statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// The core trait that all LLM provider adapters must implement.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send a synchronous (non-streaming) request.
    async fn send(&self, request: &LlmRequest) -> Result<LlmResponse, ProviderError>;

    /// Check if the provider is healthy and reachable.
    async fn health_check(&self) -> Result<(), ProviderError>;

    /// Return metadata about this provider.
    fn metadata(&self) -> &ProviderMetadata;
}

/// Errors specific to provider communication.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("request failed: {0}")]
    RequestFailed(String),

    #[error("request timed out")]
    Timeout,

    #[error("rate limited by provider")]
    RateLimited,

    #[error("authentication error: {0}")]
    AuthError(String),

    #[error("provider returned error {status}: {message}")]
    ApiError { status: u16, message: String },
}
