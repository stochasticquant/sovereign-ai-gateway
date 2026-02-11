//! Local LLM provider adapter.
//!
//! Supports locally-hosted models for maximum data sovereignty.
//! Compatible with:
//! - Ollama (OpenAI-compatible API)
//! - vLLM (OpenAI-compatible API)
//! - LocalAI
//! - Any OpenAI-compatible local inference server

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

use crate::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
use crate::cost;
use crate::retry::{with_retry, RetryConfig};
use crate::traits::{
    LlmProvider, LlmRequest, LlmResponse, ProviderError, ProviderMetadata, Usage,
};

/// Local provider configuration.
#[derive(Debug, Clone)]
pub struct LocalConfig {
    /// Base URL for the local inference server (e.g., "http://localhost:11434").
    pub base_url: String,
    /// Optional API key if the local server requires authentication.
    pub api_key: Option<String>,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
    /// Retry configuration.
    pub retry_config: RetryConfig,
    /// Circuit breaker configuration.
    pub circuit_breaker_config: CircuitBreakerConfig,
    /// Custom provider ID for identification.
    pub provider_id: String,
    /// Human-readable provider name.
    pub provider_name: String,
}

impl Default for LocalConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".to_string(),
            api_key: None,
            timeout_secs: 120, // Local models may be slower
            retry_config: RetryConfig::default(),
            circuit_breaker_config: CircuitBreakerConfig::default(),
            provider_id: "local".to_string(),
            provider_name: "Local LLM".to_string(),
        }
    }
}

/// Local provider adapter for self-hosted models.
#[derive(Debug)]
pub struct LocalProvider {
    config: LocalConfig,
    client: Client,
    metadata: ProviderMetadata,
    circuit_breaker: CircuitBreaker,
}

impl LocalProvider {
    /// Creates a new local provider with the given configuration.
    pub fn new(config: LocalConfig) -> Result<Self, ProviderError> {
        if config.base_url.is_empty() {
            return Err(ProviderError::RequestFailed(
                "Base URL is required for local provider".to_string(),
            ));
        }

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| {
                ProviderError::RequestFailed(format!("Failed to build HTTP client: {}", e))
            })?;

        let metadata = ProviderMetadata {
            id: config.provider_id.clone(),
            name: config.provider_name.clone(),
            region: "local".to_string(),
            models: vec![
                "llama3".to_string(),
                "mistral".to_string(),
                "codellama".to_string(),
                "custom".to_string(),
            ],
        };

        let circuit_breaker = CircuitBreaker::new(config.circuit_breaker_config.clone());

        Ok(Self {
            config,
            client,
            metadata,
            circuit_breaker,
        })
    }

    /// Translates internal request to local API format (OpenAI-compatible).
    fn translate_request(&self, request: &LlmRequest) -> LocalChatRequest {
        LocalChatRequest {
            model: request.model.clone(),
            messages: request
                .messages
                .iter()
                .map(|m| LocalMessage {
                    role: m.role.clone(),
                    content: m.content.clone(),
                })
                .collect(),
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            stream: if request.stream { Some(true) } else { None },
        }
    }

    /// Translates local API response to internal format.
    fn translate_response(&self, response: LocalChatResponse) -> LlmResponse {
        let content = response
            .choices
            .first()
            .and_then(|c| c.message.as_ref())
            .map(|m| m.content.clone())
            .unwrap_or_default();

        // Local servers may not always provide usage stats
        let local_usage = response.usage.unwrap_or_else(|| LocalUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        });

        let model = response.model.unwrap_or_else(|| "unknown".to_string());
        let usage = Usage {
            prompt_tokens: local_usage.prompt_tokens,
            completion_tokens: local_usage.completion_tokens,
            total_tokens: local_usage.total_tokens,
        };

        // Local models have no API costs
        let estimated_cost_usd = cost::get_local_pricing(&model)
            .map(|pricing| pricing.calculate_cost(&usage));

        LlmResponse {
            content,
            model,
            usage,
            provider_id: self.metadata.id.clone(),
            estimated_cost_usd,
        }
    }

    /// Makes the actual HTTP request to local inference server.
    #[instrument(skip(self, request), fields(model = %request.model, base_url = %self.config.base_url))]
    async fn make_request(&self, request: &LlmRequest) -> Result<LlmResponse, ProviderError> {
        // Check circuit breaker before request
        self.circuit_breaker.before_request().await?;

        let local_request = self.translate_request(request);
        let url = format!("{}/v1/chat/completions", self.config.base_url);

        debug!(?url, "Sending request to local LLM server");

        let mut request_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");

        // Add API key if configured
        if let Some(ref api_key) = self.config.api_key {
            request_builder = request_builder.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = request_builder
            .json(&local_request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ProviderError::Timeout
                } else if e.is_connect() {
                    ProviderError::RequestFailed(format!(
                        "Failed to connect to local server at {}: {}",
                        self.config.base_url, e
                    ))
                } else {
                    ProviderError::RequestFailed(format!("HTTP request failed: {}", e))
                }
            })?;

        let status = response.status();

        // Handle different status codes
        if status.is_success() {
            let local_response: LocalChatResponse = response.json().await.map_err(|e| {
                ProviderError::RequestFailed(format!("Failed to parse response: {}", e))
            })?;

            let result = self.translate_response(local_response);
            self.circuit_breaker.record_success().await;
            Ok(result)
        } else {
            // Parse error response
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            let error = match status {
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                    ProviderError::AuthError(format!("Authentication failed: {}", error_body))
                }
                StatusCode::NOT_FOUND => ProviderError::RequestFailed(format!(
                    "Endpoint not found. Is the local server running at {}?",
                    self.config.base_url
                )),
                StatusCode::TOO_MANY_REQUESTS => ProviderError::RateLimited,
                _ => ProviderError::ApiError {
                    status: status.as_u16(),
                    message: error_body,
                },
            };

            self.circuit_breaker.record_failure(&error).await;
            Err(error)
        }
    }
}

#[async_trait]
impl LlmProvider for LocalProvider {
    #[instrument(skip(self, request), fields(model = %request.model, provider = "local"))]
    async fn send(&self, request: &LlmRequest) -> Result<LlmResponse, ProviderError> {
        debug!("Sending non-streaming request to local LLM");

        // Wrap the request in retry logic
        with_retry(&self.config.retry_config, || self.make_request(request)).await
    }

    async fn health_check(&self) -> Result<(), ProviderError> {
        // Try to fetch models list as a health check
        let url = format!("{}/v1/models", self.config.base_url);

        let mut request_builder = self.client.get(&url);

        if let Some(ref api_key) = self.config.api_key {
            request_builder = request_builder.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = request_builder.send().await.map_err(|e| {
            if e.is_timeout() {
                ProviderError::Timeout
            } else if e.is_connect() {
                ProviderError::RequestFailed(format!(
                    "Cannot connect to local server at {}",
                    self.config.base_url
                ))
            } else {
                ProviderError::RequestFailed(format!("Health check failed: {}", e))
            }
        })?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(ProviderError::ApiError {
                status: response.status().as_u16(),
                message: format!("Local server returned error status"),
            })
        }
    }

    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }
}

// ============================================================================
// Local API Types (OpenAI-compatible)
// ============================================================================

/// Local Chat Completion request (OpenAI-compatible).
#[derive(Debug, Clone, Serialize)]
struct LocalChatRequest {
    model: String,
    messages: Vec<LocalMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

/// Local message format.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LocalMessage {
    role: String,
    content: String,
}

/// Local Chat Completion response.
#[derive(Debug, Clone, Deserialize)]
struct LocalChatResponse {
    id: Option<String>,
    model: Option<String>,
    choices: Vec<LocalChoice>,
    usage: Option<LocalUsage>,
}

/// A single choice in the response.
#[derive(Debug, Clone, Deserialize)]
struct LocalChoice {
    index: u32,
    message: Option<LocalMessage>,
    finish_reason: Option<String>,
}

/// Token usage statistics (may not be provided by all local servers).
#[derive(Debug, Clone, Deserialize)]
struct LocalUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::Message;

    #[test]
    fn test_translate_request() {
        let config = LocalConfig {
            base_url: "http://localhost:8080".to_string(),
            ..Default::default()
        };
        let provider = LocalProvider::new(config).unwrap();

        let request = LlmRequest {
            model: "llama3".to_string(),
            messages: vec![
                Message {
                    role: "system".to_string(),
                    content: "You are a helpful assistant.".to_string(),
                },
                Message {
                    role: "user".to_string(),
                    content: "Hello!".to_string(),
                },
            ],
            max_tokens: Some(100),
            temperature: Some(0.7),
            stream: false,
        };

        let local_request = provider.translate_request(&request);
        assert_eq!(local_request.model, "llama3");
        assert_eq!(local_request.messages.len(), 2);
        assert_eq!(local_request.max_tokens, Some(100));
        assert_eq!(local_request.temperature, Some(0.7));
    }

    #[test]
    fn test_provider_creation_requires_base_url() {
        let config = LocalConfig {
            base_url: String::new(),
            ..Default::default()
        };

        let result = LocalProvider::new(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_provider_with_custom_id_and_name() {
        let config = LocalConfig {
            base_url: "http://localhost:8080".to_string(),
            provider_id: "my-local-llm".to_string(),
            provider_name: "My Custom LLM".to_string(),
            ..Default::default()
        };
        let provider = LocalProvider::new(config).unwrap();

        let metadata = provider.metadata();
        assert_eq!(metadata.id, "my-local-llm");
        assert_eq!(metadata.name, "My Custom LLM");
        assert_eq!(metadata.region, "local");
    }

    #[test]
    fn test_metadata() {
        let config = LocalConfig::default();
        let provider = LocalProvider::new(config).unwrap();

        let metadata = provider.metadata();
        assert_eq!(metadata.id, "local");
        assert_eq!(metadata.name, "Local LLM");
        assert!(metadata.models.contains(&"llama3".to_string()));
        assert!(metadata.models.contains(&"mistral".to_string()));
    }

    #[test]
    fn test_translate_response_with_usage() {
        let config = LocalConfig::default();
        let provider = LocalProvider::new(config).unwrap();

        let local_response = LocalChatResponse {
            id: Some("test-id".to_string()),
            model: Some("llama3".to_string()),
            choices: vec![LocalChoice {
                index: 0,
                message: Some(LocalMessage {
                    role: "assistant".to_string(),
                    content: "Hello there!".to_string(),
                }),
                finish_reason: Some("stop".to_string()),
            }],
            usage: Some(LocalUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            }),
        };

        let response = provider.translate_response(local_response);
        assert_eq!(response.content, "Hello there!");
        assert_eq!(response.model, "llama3");
        assert_eq!(response.usage.prompt_tokens, 10);
        assert_eq!(response.usage.completion_tokens, 5);
        assert_eq!(response.usage.total_tokens, 15);
    }

    #[test]
    fn test_translate_response_without_usage() {
        let config = LocalConfig::default();
        let provider = LocalProvider::new(config).unwrap();

        let local_response = LocalChatResponse {
            id: None,
            model: None,
            choices: vec![LocalChoice {
                index: 0,
                message: Some(LocalMessage {
                    role: "assistant".to_string(),
                    content: "Response without usage".to_string(),
                }),
                finish_reason: None,
            }],
            usage: None,
        };

        let response = provider.translate_response(local_response);
        assert_eq!(response.content, "Response without usage");
        assert_eq!(response.model, "unknown");
        assert_eq!(response.usage.total_tokens, 0); // Defaults to 0
    }
}
