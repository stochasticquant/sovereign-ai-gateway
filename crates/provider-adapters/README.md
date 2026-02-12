# Provider Adapters

Enterprise-grade LLM provider abstraction layer with built-in resilience patterns.

## Features

- **Unified API**: Common interface across OpenAI, Anthropic, Azure OpenAI, and local providers
- **Resilience Patterns**:
  - Circuit breaker to prevent cascading failures
  - Exponential backoff retry with jitter
  - Configurable timeouts and thresholds
- **Health Monitoring**: Automatic health checks with background monitoring
- **Provider Registry**: Central management with dynamic provider registration
- **Production Ready**: Comprehensive error handling and observability

## Architecture

```
┌─────────────────────────────────────────────┐
│          Provider Registry                  │
│  - Registers providers                      │
│  - Tracks health status                     │
│  - Background health checks                 │
└─────────────────┬───────────────────────────┘
                  │
      ┌───────────┼───────────┐
      │           │           │
┌─────▼─────┬────▼─────┬────▼─────┬──────────┐
│  OpenAI   │ Anthropic│  Azure   │  Local   │
│  Adapter  │  Adapter │  Adapter │  Adapter │
└─────┬─────┴────┬─────┴────┬─────┴────┬─────┘
      │          │          │          │
┌─────▼──────────▼──────────▼──────────▼─────┐
│         Circuit Breaker & Retry             │
│  - Exponential backoff                      │
│  - Jitter to prevent thundering herd        │
│  - Circuit states: Closed/Open/Half-Open    │
└─────────────────────────────────────────────┘
```

## Usage

### Quick Start

```rust
use provider_adapters::openai::{OpenAiProvider, OpenAiConfig};
use provider_adapters::traits::{LlmProvider, LlmRequest, Message};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure provider
    let config = OpenAiConfig {
        api_key: std::env::var("OPENAI_API_KEY")?,
        ..Default::default()
    };

    let provider = OpenAiProvider::new(config)?;

    // Create request
    let request = LlmRequest {
        model: "gpt-4".to_string(),
        messages: vec![
            Message {
                role: "user".to_string(),
                content: "Hello, how are you?".to_string(),
            }
        ],
        max_tokens: Some(100),
        temperature: Some(0.7),
        stream: false,
    };

    // Send request
    let response = provider.send(&request).await?;
    println!("Response: {}", response.content);
    println!("Tokens used: {}", response.usage.total_tokens);

    Ok(())
}
```

### Provider Registry with Health Monitoring

```rust
use provider_adapters::registry::{ProviderRegistry, HealthCheckConfig};
use provider_adapters::openai::{OpenAiProvider, OpenAiConfig};
use provider_adapters::anthropic::{AnthropicProvider, AnthropicConfig};
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create registry with custom health check config
    let health_config = HealthCheckConfig {
        check_interval: Duration::from_secs(30),
        check_timeout: Duration::from_secs(5),
        failure_threshold: 3,
        success_threshold: 2,
    };

    let registry = ProviderRegistry::new(health_config);

    // Register OpenAI provider
    let openai_config = OpenAiConfig {
        api_key: std::env::var("OPENAI_API_KEY")?,
        ..Default::default()
    };
    let openai_provider = Arc::new(OpenAiProvider::new(openai_config)?);
    registry.register("openai-primary", openai_provider).await;

    // Register Anthropic provider
    let anthropic_config = AnthropicConfig {
        api_key: std::env::var("ANTHROPIC_API_KEY")?,
        ..Default::default()
    };
    let anthropic_provider = Arc::new(AnthropicProvider::new(anthropic_config)?);
    registry.register("anthropic-primary", anthropic_provider).await;

    // Start background health monitoring
    let registry_clone = registry.clone();
    tokio::spawn(async move {
        registry_clone.start_health_checks().await;
    });

    // Initial health check
    registry.check_all_now().await;

    // Get healthy providers for routing
    let healthy_providers = registry.get_healthy_providers().await;
    println!("Healthy providers: {}", healthy_providers.len());

    // Use a specific provider
    if let Some(provider) = registry.get("openai-primary").await {
        let request = LlmRequest {
            model: "gpt-4".to_string(),
            messages: vec![
                Message {
                    role: "user".to_string(),
                    content: "Hello!".to_string(),
                }
            ],
            max_tokens: Some(50),
            temperature: None,
            stream: false,
        };

        let response = provider.send(&request).await?;
        println!("Response: {}", response.content);
    }

    Ok(())
}
```

### Custom Resilience Configuration

```rust
use provider_adapters::openai::{OpenAiProvider, OpenAiConfig};
use provider_adapters::circuit_breaker::CircuitBreakerConfig;
use provider_adapters::retry::RetryConfig;
use std::time::Duration;

let config = OpenAiConfig {
    api_key: "your-api-key".to_string(),
    base_url: "https://api.openai.com/v1".to_string(),
    timeout_secs: 60,
    retry_config: RetryConfig {
        max_retries: 5,
        base_delay: Duration::from_millis(200),
        max_delay: Duration::from_secs(30),
        jitter: true,
    },
    circuit_breaker_config: CircuitBreakerConfig {
        failure_threshold: 5,
        failure_window: Duration::from_secs(60),
        timeout_rate_threshold: 0.5,
        recovery_timeout: Duration::from_secs(30),
        success_threshold: 2,
    },
};

let provider = OpenAiProvider::new(config)?;
```

## Supported Providers

### OpenAI

```rust
use provider_adapters::openai::{OpenAiProvider, OpenAiConfig};

let config = OpenAiConfig {
    api_key: std::env::var("OPENAI_API_KEY")?,
    base_url: "https://api.openai.com/v1".to_string(), // Optional
    timeout_secs: 60,
    retry_config: RetryConfig::default(),
    circuit_breaker_config: CircuitBreakerConfig::default(),
};

let provider = OpenAiProvider::new(config)?;
```

**Supported Models**: GPT-4, GPT-4 Turbo, GPT-4o, GPT-4o-mini, GPT-3.5 Turbo

### Anthropic Claude

```rust
use provider_adapters::anthropic::{AnthropicProvider, AnthropicConfig};

let config = AnthropicConfig {
    api_key: std::env::var("ANTHROPIC_API_KEY")?,
    base_url: "https://api.anthropic.com".to_string(), // Optional
    timeout_secs: 60,
    retry_config: RetryConfig::default(),
    circuit_breaker_config: CircuitBreakerConfig::default(),
};

let provider = AnthropicProvider::new(config)?;
```

**Supported Models**: Claude 3 Opus, Claude 3 Sonnet, Claude 3 Haiku, Claude 3.5 Sonnet

### Azure OpenAI

```rust
use provider_adapters::azure::{AzureOpenAiProvider, AzureOpenAiConfig};

let config = AzureOpenAiConfig {
    api_key: std::env::var("AZURE_OPENAI_API_KEY")?,
    endpoint: std::env::var("AZURE_OPENAI_ENDPOINT")?,
    deployment_id: "your-deployment-name".to_string(),
    api_version: "2024-02-15-preview".to_string(),
    timeout_secs: 60,
    retry_config: RetryConfig::default(),
    circuit_breaker_config: CircuitBreakerConfig::default(),
};

let provider = AzureOpenAiProvider::new(config)?;
```

### Local Models

```rust
use provider_adapters::local::{LocalProvider, LocalConfig};

let config = LocalConfig {
    endpoint: "http://localhost:11434".to_string(), // Ollama default
    model: "llama3".to_string(),
    timeout_secs: 120,
    retry_config: RetryConfig::default(),
    circuit_breaker_config: CircuitBreakerConfig::default(),
};

let provider = LocalProvider::new(config)?;
```

**Compatible with**: Ollama, LocalAI, LM Studio, vLLM

## Resilience Patterns

### Circuit Breaker

Prevents cascading failures by failing fast when a provider is unhealthy.

**States**:
- **Closed**: Normal operation, all requests pass through
- **Open**: Too many failures detected, requests fail immediately
- **Half-Open**: Testing if provider has recovered

**Configuration**:
```rust
CircuitBreakerConfig {
    failure_threshold: 5,        // Failures before opening circuit
    failure_window: Duration::from_secs(60),  // Time window for counting failures
    timeout_rate_threshold: 0.5, // 50% timeout rate triggers opening
    recovery_timeout: Duration::from_secs(30), // Wait before testing recovery
    success_threshold: 2,        // Successes needed to close circuit
}
```

### Retry Logic

Automatically retries transient errors with exponential backoff and jitter.

**Retryable Errors**:
- Timeouts
- 5xx server errors
- Network errors
- Rate limiting (with backoff)

**Non-Retryable Errors**:
- 4xx client errors
- Authentication failures

**Configuration**:
```rust
RetryConfig {
    max_retries: 3,
    base_delay: Duration::from_millis(100),
    max_delay: Duration::from_secs(10),
    jitter: true,  // Adds random 0-50% variation to prevent thundering herd
}
```

### Health Monitoring

Periodic health checks with automatic provider exclusion when unhealthy.

**Configuration**:
```rust
HealthCheckConfig {
    check_interval: Duration::from_secs(30),  // Time between check cycles
    check_timeout: Duration::from_secs(5),    // Timeout per health check
    failure_threshold: 3,                     // Failures before marking unhealthy
    success_threshold: 2,                     // Successes to mark healthy again
}
```

## Error Handling

All errors implement the `ProviderError` enum:

```rust
pub enum ProviderError {
    RequestFailed(String),           // Network or request errors
    Timeout,                         // Request timed out
    RateLimited,                     // Rate limit exceeded
    AuthError(String),               // Authentication failed
    ApiError { status: u16, message: String }, // API returned error
}
```

Example error handling:

```rust
use provider_adapters::traits::ProviderError;

match provider.send(&request).await {
    Ok(response) => {
        println!("Success: {}", response.content);
    }
    Err(ProviderError::RateLimited) => {
        println!("Rate limited, waiting before retry...");
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
    Err(ProviderError::AuthError(msg)) => {
        eprintln!("Authentication failed: {}", msg);
    }
    Err(ProviderError::Timeout) => {
        eprintln!("Request timed out");
    }
    Err(ProviderError::ApiError { status, message }) => {
        eprintln!("API error {}: {}", status, message);
    }
    Err(ProviderError::RequestFailed(msg)) => {
        eprintln!("Request failed: {}", msg);
    }
}
```

## Testing

### Unit Tests

```bash
cargo test --package provider-adapters
```

### Integration Tests

Integration tests use `wiremock` to mock HTTP responses:

```bash
cargo test --package provider-adapters --test integration_tests
```

### Example Test

```rust
#[tokio::test]
async fn test_openai_with_mock() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(/* ... */))
        .mount(&mock_server)
        .await;

    let config = OpenAiConfig {
        api_key: "test-key".to_string(),
        base_url: mock_server.uri(),
        ..Default::default()
    };

    let provider = OpenAiProvider::new(config)?;
    let result = provider.send(&request).await?;

    assert_eq!(result.content, "Expected response");
}
```

## Best Practices

### 1. Use the Registry for Multi-Provider Setups

```rust
// Good: Centralized provider management
let registry = ProviderRegistry::new(config);
registry.register("openai", openai_provider).await;
registry.register("anthropic", anthropic_provider).await;

let healthy = registry.get_healthy_providers().await;
```

### 2. Configure Appropriate Timeouts

```rust
// For fast models (GPT-3.5)
timeout_secs: 30

// For larger models (GPT-4, Claude Opus)
timeout_secs: 60

// For very long completions
timeout_secs: 120
```

### 3. Handle Rate Limits Gracefully

```rust
match provider.send(&request).await {
    Err(ProviderError::RateLimited) => {
        // Implement backoff or switch to backup provider
        tokio::time::sleep(Duration::from_secs(60)).await;
        // Retry or use alternate provider
    }
    result => result,
}
```

### 4. Monitor Circuit Breaker State

```rust
let state = circuit_breaker.state().await;
let state_changes = circuit_breaker.state_change_count();

// Log or emit metrics
tracing::info!(
    circuit_state = ?state,
    state_changes = state_changes,
    "Circuit breaker status"
);
```

### 5. Use Health Checks Before Critical Operations

```rust
// Ensure providers are healthy before important operations
registry.check_all_now().await;

let healthy = registry.get_healthy_providers().await;
if healthy.is_empty() {
    return Err("No healthy providers available".into());
}
```

## Performance Considerations

### Connection Pooling

All providers use `reqwest::Client` which maintains a connection pool automatically.

### Concurrent Requests

Providers are `Send + Sync` and can be safely shared across threads:

```rust
let provider = Arc::new(OpenAiProvider::new(config)?);

let handles: Vec<_> = (0..10)
    .map(|i| {
        let provider = provider.clone();
        let request = create_request(i);
        tokio::spawn(async move {
            provider.send(&request).await
        })
    })
    .collect();

for handle in handles {
    let result = handle.await??;
    println!("Result: {}", result.content);
}
```

### Memory Usage

- Each provider instance: ~few KB
- Circuit breaker state: ~few KB
- Registry overhead: ~O(n) where n = number of providers

## Observability

All operations emit structured logs using `tracing`:

```rust
use tracing_subscriber;

tracing_subscriber::fmt()
    .with_target(true)
    .with_level(true)
    .init();

// Logs will include:
// - Request attempts and retries
// - Circuit breaker state changes
// - Health check results
// - Error details
```

Example log output:
```
INFO provider_adapters::registry: Registering provider provider_id="openai-primary"
DEBUG provider_adapters::circuit_breaker: Circuit breaker transitioning Closed -> Open
WARN provider_adapters::retry: Operation failed, retrying after delay attempt=1 delay_ms=100
```

## Contributing

See the main project [CONTRIBUTING.md](../../CONTRIBUTING.md) for guidelines.

## License

MIT OR Apache-2.0
