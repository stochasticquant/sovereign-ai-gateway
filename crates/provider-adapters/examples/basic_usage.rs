//! Basic usage example for provider adapters.
//!
//! Run with: cargo run --example basic_usage
//!
//! Requires: OPENAI_API_KEY environment variable

use provider_adapters::openai::{OpenAiConfig, OpenAiProvider};
use provider_adapters::traits::{LlmProvider, LlmRequest, Message};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for observability
    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(true)
        .init();

    // Get API key from environment
    let api_key = std::env::var("OPENAI_API_KEY")
        .expect("OPENAI_API_KEY environment variable not set");

    // Configure OpenAI provider with defaults
    let config = OpenAiConfig {
        api_key,
        ..Default::default()
    };

    println!("Creating OpenAI provider...");
    let provider = OpenAiProvider::new(config)?;

    // Check provider health
    println!("Checking provider health...");
    provider.health_check().await?;
    println!("✓ Provider is healthy");

    // Create a simple request
    let request = LlmRequest {
        model: "gpt-4o-mini".to_string(),
        messages: vec![Message {
            role: "user".to_string(),
            content: "What is the capital of France? Answer in one word.".to_string(),
        }],
        max_tokens: Some(10),
        temperature: Some(0.7),
        stream: false,
    };

    println!("\nSending request...");
    let response = provider.send(&request).await?;

    println!("\n--- Response ---");
    println!("Model: {}", response.model);
    println!("Content: {}", response.content);
    println!("\n--- Usage ---");
    println!("Prompt tokens: {}", response.usage.prompt_tokens);
    println!("Completion tokens: {}", response.usage.completion_tokens);
    println!("Total tokens: {}", response.usage.total_tokens);

    if let Some(cost) = response.estimated_cost_usd {
        println!("\n--- Cost Estimation ---");
        println!("Estimated cost: ${:.6} USD", cost);
    }

    Ok(())
}
