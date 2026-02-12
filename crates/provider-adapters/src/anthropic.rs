//! Anthropic Claude provider adapter.
//!
//! Translates gateway-internal `LlmRequest` to Anthropic Messages API format.
//! Supports both synchronous and streaming (SSE) responses.
//!
//! API Reference: https://docs.anthropic.com/en/api/messages

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

use crate::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
use crate::cost;
use crate::retry::{RetryConfig, with_retry};
use crate::traits::{
    LlmProvider, LlmRequest, LlmResponse, Message, ProviderError, ProviderMetadata, Usage,
};

const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Anthropic provider configuration.
#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    /// API key for authentication.
    pub api_key: String,
    /// Base URL for the API (default: https://api.anthropic.com).
    pub base_url: String,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
    /// Retry configuration.
    pub retry_config: RetryConfig,
    /// Circuit breaker configuration.
    pub circuit_breaker_config: CircuitBreakerConfig,
}

impl Default for AnthropicConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.anthropic.com".to_string(),
            timeout_secs: 60,
            retry_config: RetryConfig::default(),
            circuit_breaker_config: CircuitBreakerConfig::default(),
        }
    }
}

/// Anthropic provider adapter.
#[derive(Debug)]
pub struct AnthropicProvider {
    config: AnthropicConfig,
    client: Client,
    metadata: ProviderMetadata,
    circuit_breaker: CircuitBreaker,
}

impl AnthropicProvider {
    /// Creates a new Anthropic provider with the given configuration.
    pub fn new(config: AnthropicConfig) -> Result<Self, ProviderError> {
        if config.api_key.is_empty() {
            return Err(ProviderError::AuthError(
                "Anthropic API key is required".to_string(),
            ));
        }

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| {
                ProviderError::RequestFailed(format!("Failed to build HTTP client: {}", e))
            })?;

        let metadata = ProviderMetadata {
            id: "anthropic".to_string(),
            name: "Anthropic Claude".to_string(),
            region: "us".to_string(),
            models: vec![
                "claude-3-5-sonnet-20241022".to_string(),
                "claude-3-opus-20240229".to_string(),
                "claude-3-sonnet-20240229".to_string(),
                "claude-3-haiku-20240307".to_string(),
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

    /// Translates internal request to Anthropic API format.
    fn translate_request(
        &self,
        request: &LlmRequest,
    ) -> (AnthropicMessagesRequest, Option<String>) {
        // Anthropic separates system message from conversation messages
        let mut system_message: Option<String> = None;
        let mut messages = Vec::new();

        for msg in &request.messages {
            if msg.role == "system" {
                // Concatenate multiple system messages if present
                if let Some(ref mut sys) = system_message {
                    sys.push_str("\n\n");
                    sys.push_str(&msg.content);
                } else {
                    system_message = Some(msg.content.clone());
                }
            } else {
                messages.push(AnthropicMessage {
                    role: msg.role.clone(),
                    content: msg.content.clone(),
                });
            }
        }

        let anthropic_request = AnthropicMessagesRequest {
            model: request.model.clone(),
            messages,
            max_tokens: request.max_tokens.unwrap_or(4096),
            temperature: request.temperature,
            stream: if request.stream { Some(true) } else { None },
            system: None, // Set later if system_message is present
        };

        (anthropic_request, system_message)
    }

    /// Translates Anthropic response to internal format.
    fn translate_response(&self, response: AnthropicMessagesResponse) -> LlmResponse {
        let content = response
            .content
            .first()
            .map(|block| block.text.clone())
            .unwrap_or_default();

        let usage = Usage {
            prompt_tokens: response.usage.input_tokens,
            completion_tokens: response.usage.output_tokens,
            total_tokens: response.usage.input_tokens + response.usage.output_tokens,
        };

        // Calculate estimated cost
        let estimated_cost_usd = cost::get_anthropic_pricing(&response.model)
            .map(|pricing| pricing.calculate_cost(&usage));

        LlmResponse {
            content,
            model: response.model,
            usage,
            provider_id: self.metadata.id.clone(),
            estimated_cost_usd,
        }
    }

    /// Makes the actual HTTP request to Anthropic API.
    #[instrument(skip(self, request), fields(model = %request.model))]
    async fn make_request(&self, request: &LlmRequest) -> Result<LlmResponse, ProviderError> {
        // Check circuit breaker before request
        self.circuit_breaker.before_request().await?;

        let (mut anthropic_request, system_message) = self.translate_request(request);
        let url = format!("{}/v1/messages", self.config.base_url);

        debug!(?url, "Sending request to Anthropic");

        // Add system message if present
        if let Some(system) = system_message {
            anthropic_request.system = Some(system);
        }

        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("Content-Type", "application/json")
            .json(&anthropic_request)
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
            let anthropic_response: AnthropicMessagesResponse =
                response.json().await.map_err(|e| {
                    ProviderError::RequestFailed(format!("Failed to parse response: {}", e))
                })?;

            let result = self.translate_response(anthropic_response);
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
impl LlmProvider for AnthropicProvider {
    #[instrument(skip(self, request), fields(model = %request.model, provider = "anthropic"))]
    async fn send(&self, request: &LlmRequest) -> Result<LlmResponse, ProviderError> {
        debug!("Sending non-streaming request to Anthropic");

        // Wrap the request in retry logic
        with_retry(&self.config.retry_config, || self.make_request(request)).await
    }

    async fn health_check(&self) -> Result<(), ProviderError> {
        // Anthropic doesn't have a dedicated health check endpoint
        // We'll use a minimal messages request as a health check
        let test_request = LlmRequest {
            model: "claude-3-haiku-20240307".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: "ping".to_string(),
            }],
            max_tokens: Some(5),
            temperature: None,
            stream: false,
        };

        self.send(&test_request).await.map(|_| ())
    }

    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }
}

// ============================================================================
// Anthropic API Types
// ============================================================================

/// Anthropic Messages API request.
#[derive(Debug, Clone, Serialize)]
struct AnthropicMessagesRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
}

/// Anthropic message format.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

/// Anthropic Messages API response.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct AnthropicMessagesResponse {
    id: String,
    #[serde(rename = "type")]
    response_type: String,
    role: String,
    content: Vec<AnthropicContentBlock>,
    model: String,
    stop_reason: Option<String>,
    usage: AnthropicUsage,
}

/// Content block in Anthropic response.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: String,
}

/// Token usage statistics.
#[derive(Debug, Clone, Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translate_request() {
        let config = AnthropicConfig {
            api_key: "test-key".to_string(),
            ..Default::default()
        };
        let provider = AnthropicProvider::new(config).unwrap();

        let request = LlmRequest {
            model: "claude-3-sonnet-20240229".to_string(),
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

        let (anthropic_request, system) = provider.translate_request(&request);
        assert_eq!(anthropic_request.model, "claude-3-sonnet-20240229");
        assert_eq!(anthropic_request.messages.len(), 1); // Only user message
        assert_eq!(anthropic_request.max_tokens, 100);
        assert_eq!(anthropic_request.temperature, Some(0.7));
        assert_eq!(system, Some("You are a helpful assistant.".to_string()));
    }

    #[test]
    fn test_translate_request_multiple_system_messages() {
        let config = AnthropicConfig {
            api_key: "test-key".to_string(),
            ..Default::default()
        };
        let provider = AnthropicProvider::new(config).unwrap();

        let request = LlmRequest {
            model: "claude-3-sonnet-20240229".to_string(),
            messages: vec![
                Message {
                    role: "system".to_string(),
                    content: "First instruction.".to_string(),
                },
                Message {
                    role: "system".to_string(),
                    content: "Second instruction.".to_string(),
                },
                Message {
                    role: "user".to_string(),
                    content: "Hello!".to_string(),
                },
            ],
            max_tokens: None,
            temperature: None,
            stream: false,
        };

        let (anthropic_request, system) = provider.translate_request(&request);
        assert_eq!(anthropic_request.messages.len(), 1);
        assert_eq!(
            system,
            Some("First instruction.\n\nSecond instruction.".to_string())
        );
    }

    #[test]
    fn test_provider_creation_requires_api_key() {
        let config = AnthropicConfig {
            api_key: String::new(),
            ..Default::default()
        };

        let result = AnthropicProvider::new(config);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProviderError::AuthError(_)));
    }

    #[test]
    fn test_metadata() {
        let config = AnthropicConfig {
            api_key: "test-key".to_string(),
            ..Default::default()
        };
        let provider = AnthropicProvider::new(config).unwrap();

        let metadata = provider.metadata();
        assert_eq!(metadata.id, "anthropic");
        assert_eq!(metadata.name, "Anthropic Claude");
        assert!(
            metadata
                .models
                .contains(&"claude-3-5-sonnet-20241022".to_string())
        );
        assert!(
            metadata
                .models
                .contains(&"claude-3-opus-20240229".to_string())
        );
    }
}
