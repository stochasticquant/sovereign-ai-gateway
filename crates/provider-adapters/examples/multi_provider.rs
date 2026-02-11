//! Multi-provider example with registry and health monitoring.
//!
//! Run with: cargo run --example multi_provider
//!
//! Requires:
//! - OPENAI_API_KEY environment variable
//! - ANTHROPIC_API_KEY environment variable

use provider_adapters::anthropic::{AnthropicConfig, AnthropicProvider};
use provider_adapters::openai::{OpenAiConfig, OpenAiProvider};
use provider_adapters::registry::{HealthCheckConfig, ProviderRegistry};
use provider_adapters::traits::{LlmProvider, LlmRequest, Message};
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(true)
        .init();

    println!("=== Multi-Provider Example ===\n");

    // Create registry with health monitoring
    let health_config = HealthCheckConfig {
        check_interval: Duration::from_secs(30),
        check_timeout: Duration::from_secs(5),
        failure_threshold: 3,
        success_threshold: 2,
    };

    let registry = ProviderRegistry::new(health_config);
    println!("✓ Created provider registry");

    // Register OpenAI provider
    if let Ok(openai_key) = std::env::var("OPENAI_API_KEY") {
        let openai_config = OpenAiConfig {
            api_key: openai_key,
            ..Default::default()
        };

        match OpenAiProvider::new(openai_config) {
            Ok(provider) => {
                registry
                    .register("openai-primary", Arc::new(provider))
                    .await;
                println!("✓ Registered OpenAI provider");
            }
            Err(e) => println!("✗ Failed to create OpenAI provider: {}", e),
        }
    } else {
        println!("⚠ OPENAI_API_KEY not set, skipping OpenAI provider");
    }

    // Register Anthropic provider
    if let Ok(anthropic_key) = std::env::var("ANTHROPIC_API_KEY") {
        let anthropic_config = AnthropicConfig {
            api_key: anthropic_key,
            ..Default::default()
        };

        match AnthropicProvider::new(anthropic_config) {
            Ok(provider) => {
                registry
                    .register("anthropic-primary", Arc::new(provider))
                    .await;
                println!("✓ Registered Anthropic provider");
            }
            Err(e) => println!("✗ Failed to create Anthropic provider: {}", e),
        }
    } else {
        println!("⚠ ANTHROPIC_API_KEY not set, skipping Anthropic provider");
    }

    // List registered providers
    let provider_ids = registry.list_ids().await;
    println!("\n--- Registered Providers ---");
    for id in &provider_ids {
        println!("  - {}", id);
    }

    if provider_ids.is_empty() {
        eprintln!("\n✗ No providers registered. Please set OPENAI_API_KEY or ANTHROPIC_API_KEY.");
        return Ok(());
    }

    // Start background health monitoring
    println!("\n✓ Starting background health monitoring...");
    let registry_clone = registry.clone();
    tokio::spawn(async move {
        registry_clone.start_health_checks().await;
    });

    // Perform initial health check
    println!("✓ Running initial health checks...");
    registry.check_all_now().await;
    tokio::time::sleep(Duration::from_millis(100)).await; // Give time for checks to complete

    registry.check_all_now().await;
    tokio::time::sleep(Duration::from_millis(100)).await; // Second check for healthy status

    // Display health status
    let health_info = registry.get_all_health_info().await;
    println!("\n--- Health Status ---");
    for (id, (status, last_check, last_error)) in &health_info {
        println!(
            "  {}: {:?} (last check: {:?})",
            id,
            status,
            last_check.map(|t| t.elapsed())
        );
        if let Some(error) = last_error {
            println!("    Error: {}", error);
        }
    }

    // Get healthy providers
    let healthy_providers = registry.get_healthy_providers().await;
    println!(
        "\n✓ {} healthy provider(s) available",
        healthy_providers.len()
    );

    if healthy_providers.is_empty() {
        println!("⚠ No healthy providers available for requests");
        return Ok(());
    }

    // Send a request to each healthy provider
    let request = LlmRequest {
        model: "gpt-4o-mini".to_string(), // Will be adjusted per provider
        messages: vec![Message {
            role: "user".to_string(),
            content: "Say 'Hello' in a creative way!".to_string(),
        }],
        max_tokens: Some(50),
        temperature: Some(0.9),
        stream: false,
    };

    println!("\n--- Testing Providers ---");
    for (provider_id, provider) in healthy_providers {
        println!("\nProvider: {}", provider_id);
        println!("  Model: {}", provider.metadata().name);

        // Adjust model name based on provider
        let mut provider_request = request.clone();
        if provider_id.contains("anthropic") {
            provider_request.model = "claude-3-haiku-20240307".to_string();
        }

        match provider.send(&provider_request).await {
            Ok(response) => {
                println!("  ✓ Response: {}", response.content);
                println!(
                    "  Tokens: {} prompt + {} completion = {} total",
                    response.usage.prompt_tokens,
                    response.usage.completion_tokens,
                    response.usage.total_tokens
                );
            }
            Err(e) => {
                println!("  ✗ Error: {}", e);
            }
        }
    }

    println!("\n=== Example Complete ===");
    Ok(())
}
