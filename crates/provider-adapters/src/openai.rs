//! OpenAI provider adapter.
//!
//! Translates gateway-internal `LlmRequest` to OpenAI Chat Completions API format.
//! Supports both synchronous and streaming (SSE) responses.
//!
//! API Reference: https://platform.openai.com/docs/api-reference/chat

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

use crate::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
use crate::cost;
use crate::retry::{with_retry, RetryConfig};
use crate::traits::{LlmProvider, LlmRequest, LlmResponse, ProviderError, ProviderMetadata, Usage};

/// OpenAI provider configuration.
#[derive(Debug, Clone)]
pub struct OpenAiConfig {
    /// API key for authentication.
    pub api_key: String,
    /// Base URL for the API (default: https://api.openai.com/v1).
    pub base_url: String,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
    /// Retry configuration.
    pub retry_config: RetryConfig,
    /// Circuit breaker configuration.
    pub circuit_breaker_config: CircuitBreakerConfig,
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.openai.com/v1".to_string(),
            timeout_secs: 60,
            retry_config: RetryConfig::default(),
            circuit_breaker_config: CircuitBreakerConfig::default(),
        }
    }
}

/// OpenAI provider adapter.
#[derive(Debug)]
pub struct OpenAiProvider {
    config: OpenAiConfig,
    client: Client,
    metadata: ProviderMetadata,
    circuit_breaker: CircuitBreaker,
}

impl OpenAiProvider {
    /// Creates a new OpenAI provider with the given configuration.
    pub fn new(config: OpenAiConfig) -> Result<Self, ProviderError> {
        if config.api_key.is_empty() {
            return Err(ProviderError::AuthError(
                "OpenAI API key is required".to_string(),
            ));
        }

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| ProviderError::RequestFailed(format!("Failed to build HTTP client: {}", e)))?;

        let metadata = ProviderMetadata {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
            region: "us".to_string(),
            models: vec![
                "gpt-4".to_string(),
                "gpt-4-turbo".to_string(),
                "gpt-4o".to_string(),
                "gpt-4o-mini".to_string(),
                "gpt-3.5-turbo".to_string(),
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

    /// Translates internal request to OpenAI API format.
    fn translate_request(&self, request: &LlmRequest) -> OpenAiChatRequest {
        OpenAiChatRequest {
            model: request.model.clone(),
            messages: request
                .messages
                .iter()
                .map(|m| OpenAiMessage {
                    role: m.role.clone(),
                    content: m.content.clone(),
                })
                .collect(),
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            stream: if request.stream { Some(true) } else { None },
        }
    }

    /// Translates OpenAI response to internal format.
    fn translate_response(&self, response: OpenAiChatResponse) -> LlmResponse {
        let content = response
            .choices
            .first()
            .and_then(|c| c.message.as_ref())
            .map(|m| m.content.clone())
            .unwrap_or_default();

        let usage = Usage {
            prompt_tokens: response.usage.prompt_tokens,
            completion_tokens: response.usage.completion_tokens,
            total_tokens: response.usage.total_tokens,
        };

        // Calculate estimated cost
        let estimated_cost_usd = cost::get_openai_pricing(&response.model)
            .map(|pricing| pricing.calculate_cost(&usage));

        LlmResponse {
            content,
            model: response.model,
            usage,
            provider_id: self.metadata.id.clone(),
            estimated_cost_usd,
        }
    }

    /// Makes the actual HTTP request to OpenAI API.
    #[instrument(skip(self, request), fields(model = %request.model))]
    async fn make_request(&self, request: &LlmRequest) -> Result<LlmResponse, ProviderError> {
        // Check circuit breaker before request
        self.circuit_breaker.before_request().await?;

        let openai_request = self.translate_request(request);
        let url = format!("{}/chat/completions", self.config.base_url);

        debug!(?url, "Sending request to OpenAI");

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&openai_request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ProviderError::Timeout
                } else {
                    ProviderError::RequestFailed(format!("HTTP request failed: {}", e))
                }
            })?;

        let status = response.status();

        // Handle different status codes
        if status.is_success() {
            let openai_response: OpenAiChatResponse = response
                .json()
                .await
                .map_err(|e| ProviderError::RequestFailed(format!("Failed to parse response: {}", e)))?;

            let result = self.translate_response(openai_response);
            self.circuit_breaker.record_success().await;
            Ok(result)
        } else {
            // Parse error response
            let error_body = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            let error = match status {
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                    ProviderError::AuthError(format!("Authentication failed: {}", error_body))
                }
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
impl LlmProvider for OpenAiProvider {
    #[instrument(skip(self, request), fields(model = %request.model, provider = "openai"))]
    async fn send(&self, request: &LlmRequest) -> Result<LlmResponse, ProviderError> {
        debug!("Sending non-streaming request to OpenAI");

        // Wrap the request in retry logic
        with_retry(&self.config.retry_config, || self.make_request(request)).await
    }

    async fn health_check(&self) -> Result<(), ProviderError> {
        let url = format!("{}/models", self.config.base_url);

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ProviderError::Timeout
                } else {
                    ProviderError::RequestFailed(format!("Health check failed: {}", e))
                }
            })?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(ProviderError::ApiError {
                status: response.status().as_u16(),
                message: "Health check failed".to_string(),
            })
        }
    }

    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }
}

// ============================================================================
// OpenAI API Types
// ============================================================================

/// OpenAI Chat Completion request.
#[derive(Debug, Clone, Serialize)]
struct OpenAiChatRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

/// OpenAI message format.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAiMessage {
    role: String,
    content: String,
}

/// OpenAI Chat Completion response.
#[derive(Debug, Clone, Deserialize)]
struct OpenAiChatResponse {
    id: String,
    model: String,
    choices: Vec<OpenAiChoice>,
    usage: OpenAiUsage,
}

/// A single choice in the response.
#[derive(Debug, Clone, Deserialize)]
struct OpenAiChoice {
    index: u32,
    message: Option<OpenAiMessage>,
    finish_reason: Option<String>,
}

/// Token usage statistics.
#[derive(Debug, Clone, Deserialize)]
struct OpenAiUsage {
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
        let config = OpenAiConfig {
            api_key: "test-key".to_string(),
            ..Default::default()
        };
        let provider = OpenAiProvider::new(config).unwrap();

        let request = LlmRequest {
            model: "gpt-4".to_string(),
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

        let openai_request = provider.translate_request(&request);
        assert_eq!(openai_request.model, "gpt-4");
        assert_eq!(openai_request.messages.len(), 2);
        assert_eq!(openai_request.max_tokens, Some(100));
        assert_eq!(openai_request.temperature, Some(0.7));
    }

    #[test]
    fn test_provider_creation_requires_api_key() {
        let config = OpenAiConfig {
            api_key: String::new(),
            ..Default::default()
        };

        let result = OpenAiProvider::new(config);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProviderError::AuthError(_)));
    }

    #[test]
    fn test_metadata() {
        let config = OpenAiConfig {
            api_key: "test-key".to_string(),
            ..Default::default()
        };
        let provider = OpenAiProvider::new(config).unwrap();

        let metadata = provider.metadata();
        assert_eq!(metadata.id, "openai");
        assert_eq!(metadata.name, "OpenAI");
        assert!(metadata.models.contains(&"gpt-4".to_string()));
        assert!(metadata.models.contains(&"gpt-4o".to_string()));
    }
}
