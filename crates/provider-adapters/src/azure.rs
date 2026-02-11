//! Azure OpenAI provider adapter.
//!
//! Routes to Azure regional endpoints for data residency compliance.
//! Supports Azure OpenAI Service API format, which is similar to OpenAI but
//! with different authentication and endpoint structure.
//!
//! API Reference: https://learn.microsoft.com/en-us/azure/ai-services/openai/reference

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

use crate::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
use crate::retry::{with_retry, RetryConfig};
use crate::traits::{
    LlmProvider, LlmRequest, LlmResponse, Message, ProviderError, ProviderMetadata, Usage,
};

/// Azure OpenAI provider configuration.
#[derive(Debug, Clone)]
pub struct AzureConfig {
    /// API key for authentication.
    pub api_key: String,
    /// Azure resource name (e.g., "my-resource").
    pub resource_name: String,
    /// Azure deployment name (model deployment in Azure).
    pub deployment_name: String,
    /// API version (default: 2024-02-15-preview).
    pub api_version: String,
    /// Azure region for data residency (e.g., "eastus", "westeurope").
    pub region: String,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
    /// Retry configuration.
    pub retry_config: RetryConfig,
    /// Circuit breaker configuration.
    pub circuit_breaker_config: CircuitBreakerConfig,
}

impl Default for AzureConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            resource_name: String::new(),
            deployment_name: String::new(),
            api_version: "2024-02-15-preview".to_string(),
            region: "eastus".to_string(),
            timeout_secs: 60,
            retry_config: RetryConfig::default(),
            circuit_breaker_config: CircuitBreakerConfig::default(),
        }
    }
}

/// Azure OpenAI provider adapter.
#[derive(Debug)]
pub struct AzureProvider {
    config: AzureConfig,
    client: Client,
    metadata: ProviderMetadata,
    circuit_breaker: CircuitBreaker,
}

impl AzureProvider {
    /// Creates a new Azure OpenAI provider with the given configuration.
    pub fn new(config: AzureConfig) -> Result<Self, ProviderError> {
        if config.api_key.is_empty() {
            return Err(ProviderError::AuthError(
                "Azure OpenAI API key is required".to_string(),
            ));
        }
        if config.resource_name.is_empty() {
            return Err(ProviderError::AuthError(
                "Azure resource name is required".to_string(),
            ));
        }
        if config.deployment_name.is_empty() {
            return Err(ProviderError::AuthError(
                "Azure deployment name is required".to_string(),
            ));
        }

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| {
                ProviderError::RequestFailed(format!("Failed to build HTTP client: {}", e))
            })?;

        let metadata = ProviderMetadata {
            id: format!("azure-{}", config.region),
            name: format!("Azure OpenAI ({})", config.region),
            region: config.region.clone(),
            models: vec![
                "gpt-4".to_string(),
                "gpt-4-32k".to_string(),
                "gpt-4-turbo".to_string(),
                "gpt-35-turbo".to_string(),
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

    /// Builds the Azure OpenAI endpoint URL.
    fn build_endpoint_url(&self) -> String {
        format!(
            "https://{}.openai.azure.com/openai/deployments/{}/chat/completions?api-version={}",
            self.config.resource_name, self.config.deployment_name, self.config.api_version
        )
    }

    /// Translates internal request to Azure OpenAI API format.
    /// Azure uses the same format as OpenAI Chat Completions.
    fn translate_request(&self, request: &LlmRequest) -> AzureChatRequest {
        AzureChatRequest {
            messages: request
                .messages
                .iter()
                .map(|m| AzureMessage {
                    role: m.role.clone(),
                    content: m.content.clone(),
                })
                .collect(),
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            stream: if request.stream { Some(true) } else { None },
        }
    }

    /// Translates Azure response to internal format.
    fn translate_response(&self, response: AzureChatResponse) -> LlmResponse {
        let content = response
            .choices
            .first()
            .and_then(|c| c.message.as_ref())
            .map(|m| m.content.clone())
            .unwrap_or_default();

        LlmResponse {
            content,
            model: response.model.unwrap_or_else(|| self.config.deployment_name.clone()),
            usage: Usage {
                prompt_tokens: response.usage.prompt_tokens,
                completion_tokens: response.usage.completion_tokens,
                total_tokens: response.usage.total_tokens,
            },
            provider_id: self.metadata.id.clone(),
        }
    }

    /// Makes the actual HTTP request to Azure OpenAI API.
    #[instrument(skip(self, request), fields(deployment = %self.config.deployment_name, region = %self.config.region))]
    async fn make_request(&self, request: &LlmRequest) -> Result<LlmResponse, ProviderError> {
        // Check circuit breaker before request
        self.circuit_breaker.before_request().await?;

        let azure_request = self.translate_request(request);
        let url = self.build_endpoint_url();

        debug!(?url, "Sending request to Azure OpenAI");

        let response = self
            .client
            .post(&url)
            .header("api-key", &self.config.api_key)
            .header("Content-Type", "application/json")
            .json(&azure_request)
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
            let azure_response: AzureChatResponse = response.json().await.map_err(|e| {
                ProviderError::RequestFailed(format!("Failed to parse response: {}", e))
            })?;

            let result = self.translate_response(azure_response);
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
impl LlmProvider for AzureProvider {
    #[instrument(skip(self, request), fields(deployment = %self.config.deployment_name, provider = "azure"))]
    async fn send(&self, request: &LlmRequest) -> Result<LlmResponse, ProviderError> {
        debug!("Sending non-streaming request to Azure OpenAI");

        // Wrap the request in retry logic
        with_retry(&self.config.retry_config, || self.make_request(request)).await
    }

    async fn health_check(&self) -> Result<(), ProviderError> {
        // Azure doesn't have a separate health endpoint
        // We'll use a minimal chat completion as health check
        let test_request = LlmRequest {
            model: self.config.deployment_name.clone(),
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
// Azure OpenAI API Types
// ============================================================================

/// Azure OpenAI Chat Completion request.
/// Format is identical to OpenAI, but endpoint and auth differ.
#[derive(Debug, Clone, Serialize)]
struct AzureChatRequest {
    messages: Vec<AzureMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

/// Azure message format (same as OpenAI).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AzureMessage {
    role: String,
    content: String,
}

/// Azure OpenAI Chat Completion response.
#[derive(Debug, Clone, Deserialize)]
struct AzureChatResponse {
    id: String,
    model: Option<String>, // Azure sometimes omits this
    choices: Vec<AzureChoice>,
    usage: AzureUsage,
}

/// A single choice in the response.
#[derive(Debug, Clone, Deserialize)]
struct AzureChoice {
    index: u32,
    message: Option<AzureMessage>,
    finish_reason: Option<String>,
}

/// Token usage statistics.
#[derive(Debug, Clone, Deserialize)]
struct AzureUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translate_request() {
        let config = AzureConfig {
            api_key: "test-key".to_string(),
            resource_name: "test-resource".to_string(),
            deployment_name: "gpt-4-deployment".to_string(),
            ..Default::default()
        };
        let provider = AzureProvider::new(config).unwrap();

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

        let azure_request = provider.translate_request(&request);
        assert_eq!(azure_request.messages.len(), 2);
        assert_eq!(azure_request.max_tokens, Some(100));
        assert_eq!(azure_request.temperature, Some(0.7));
    }

    #[test]
    fn test_build_endpoint_url() {
        let config = AzureConfig {
            api_key: "test-key".to_string(),
            resource_name: "my-resource".to_string(),
            deployment_name: "gpt4-deployment".to_string(),
            api_version: "2024-02-15-preview".to_string(),
            ..Default::default()
        };
        let provider = AzureProvider::new(config).unwrap();

        let url = provider.build_endpoint_url();
        assert_eq!(
            url,
            "https://my-resource.openai.azure.com/openai/deployments/gpt4-deployment/chat/completions?api-version=2024-02-15-preview"
        );
    }

    #[test]
    fn test_provider_creation_requires_api_key() {
        let config = AzureConfig {
            api_key: String::new(),
            resource_name: "test".to_string(),
            deployment_name: "test".to_string(),
            ..Default::default()
        };

        let result = AzureProvider::new(config);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ProviderError::AuthError(_)
        ));
    }

    #[test]
    fn test_provider_creation_requires_resource_name() {
        let config = AzureConfig {
            api_key: "test-key".to_string(),
            resource_name: String::new(),
            deployment_name: "test".to_string(),
            ..Default::default()
        };

        let result = AzureProvider::new(config);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ProviderError::AuthError(_)
        ));
    }

    #[test]
    fn test_provider_creation_requires_deployment_name() {
        let config = AzureConfig {
            api_key: "test-key".to_string(),
            resource_name: "test".to_string(),
            deployment_name: String::new(),
            ..Default::default()
        };

        let result = AzureProvider::new(config);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ProviderError::AuthError(_)
        ));
    }

    #[test]
    fn test_metadata() {
        let config = AzureConfig {
            api_key: "test-key".to_string(),
            resource_name: "test-resource".to_string(),
            deployment_name: "gpt-4-deployment".to_string(),
            region: "westeurope".to_string(),
            ..Default::default()
        };
        let provider = AzureProvider::new(config).unwrap();

        let metadata = provider.metadata();
        assert_eq!(metadata.id, "azure-westeurope");
        assert_eq!(metadata.name, "Azure OpenAI (westeurope)");
        assert_eq!(metadata.region, "westeurope");
        assert!(metadata.models.contains(&"gpt-4".to_string()));
    }
}
