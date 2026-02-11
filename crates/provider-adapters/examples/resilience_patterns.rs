//! Demonstration of resilience patterns: retry and circuit breaker.
//!
//! Run with: cargo run --example resilience_patterns
//!
//! This example shows how the provider adapters handle failures
//! and recover automatically using retry logic and circuit breakers.

use provider_adapters::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
use provider_adapters::retry::{with_retry, RetryConfig};
use provider_adapters::traits::ProviderError;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(true)
        .init();

    println!("=== Resilience Patterns Demo ===\n");

    demo_retry_logic().await?;
    println!("\n---\n");
    demo_circuit_breaker().await?;

    println!("\n=== Demo Complete ===");
    Ok(())
}

async fn demo_retry_logic() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Retry Logic Demo ---\n");

    let config = RetryConfig {
        max_retries: 3,
        base_delay: Duration::from_millis(50),
        max_delay: Duration::from_millis(500),
        jitter: false, // Disable for predictable demo
    };

    let attempt_count = Arc::new(AtomicU32::new(0));

    println!("Simulating operation that fails twice then succeeds...");

    let result = {
        let attempt_count = attempt_count.clone();
        with_retry(&config, move || {
            let count = attempt_count.clone();
            async move {
                let current = count.fetch_add(1, Ordering::SeqCst);
                println!("  Attempt {} at {:?}", current + 1, std::time::Instant::now());

                if current < 2 {
                    // Fail first 2 attempts
                    Err(ProviderError::Timeout)
                } else {
                    // Succeed on 3rd attempt
                    Ok("Success!")
                }
            }
        })
        .await
    };

    match result {
        Ok(msg) => {
            println!("\n✓ Operation succeeded after {} attempts", attempt_count.load(Ordering::SeqCst));
            println!("  Result: {}", msg);
        }
        Err(e) => {
            println!("\n✗ Operation failed: {}", e);
        }
    }

    // Demo non-retryable error
    println!("\n\nSimulating non-retryable error (auth failure)...");
    let attempt_count2 = Arc::new(AtomicU32::new(0));

    let result = {
        let attempt_count2 = attempt_count2.clone();
        with_retry(&config, move || {
            let count = attempt_count2.clone();
            async move {
                let current = count.fetch_add(1, Ordering::SeqCst);
                println!("  Attempt {} at {:?}", current + 1, std::time::Instant::now());
                Err::<&str, _>(ProviderError::AuthError("Invalid API key".to_string()))
            }
        })
        .await
    };

    match result {
        Err(ProviderError::AuthError(_)) => {
            println!("\n✓ Correctly failed fast without retries");
            println!("  Total attempts: {}", attempt_count2.load(Ordering::SeqCst));
        }
        _ => println!("✗ Unexpected result"),
    }

    Ok(())
}

async fn demo_circuit_breaker() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Circuit Breaker Demo ---\n");

    let config = CircuitBreakerConfig {
        failure_threshold: 3,
        failure_window: Duration::from_secs(60),
        timeout_rate_threshold: 0.5,
        recovery_timeout: Duration::from_millis(500),
        success_threshold: 2,
    };

    let circuit_breaker = CircuitBreaker::new(config);

    println!("Initial state: {:?}\n", circuit_breaker.state().await);

    // Simulate failures to open circuit
    println!("Simulating {} failures to open circuit...", 3);
    for i in 1..=3 {
        let _ = circuit_breaker.before_request().await;
        circuit_breaker
            .record_failure(&ProviderError::Timeout)
            .await;
        println!("  Failure {} recorded. State: {:?}", i, circuit_breaker.state().await);
    }

    println!("\n✓ Circuit is now OPEN (failing fast)");

    // Try to make a request while circuit is open
    println!("\nAttempting request while circuit is open...");
    match circuit_breaker.before_request().await {
        Ok(_) => println!("✗ Request allowed (unexpected)"),
        Err(e) => println!("✓ Request blocked: {}", e),
    }

    // Wait for recovery timeout
    println!("\nWaiting for recovery timeout (500ms)...");
    tokio::time::sleep(Duration::from_millis(550)).await;

    println!("\nCircuit should transition to HALF-OPEN...");
    let _ = circuit_breaker.before_request().await;
    println!("State after recovery timeout: {:?}", circuit_breaker.state().await);

    // Simulate successful requests to close circuit
    println!("\nSimulating {} successful requests...", 2);
    for i in 1..=2 {
        let _ = circuit_breaker.before_request().await;
        circuit_breaker.record_success().await;
        println!("  Success {} recorded. State: {:?}", i, circuit_breaker.state().await);
    }

    println!("\n✓ Circuit is now CLOSED (normal operation)");

    // Verify circuit accepts requests
    println!("\nAttempting request with closed circuit...");
    match circuit_breaker.before_request().await {
        Ok(_) => println!("✓ Request allowed"),
        Err(e) => println!("✗ Request blocked: {}", e),
    }

    println!("\nCircuit breaker metrics:");
    println!("  State transitions: {}", circuit_breaker.state_change_count());

    Ok(())
}
