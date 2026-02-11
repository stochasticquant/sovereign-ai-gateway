//! Per-provider circuit breaker for fault tolerance.
//!
//! Implements the circuit breaker pattern with three states:
//! - **Closed**: Normal operation, requests pass through
//! - **Open**: Too many failures, requests fail fast without calling provider
//! - **Half-Open**: Testing if provider has recovered
//!
//! State transitions:
//! ```text
//! Closed ──[failure threshold]──> Open
//!   ↑                               │
//!   │                               │ [timeout]
//!   │                               ↓
//!   └────[success]──── Half-Open
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::traits::ProviderError;

/// Configuration for circuit breaker behavior.
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening the circuit.
    pub failure_threshold: u32,
    /// Time window for counting failures.
    pub failure_window: Duration,
    /// Timeout rate (0.0-1.0) that triggers circuit opening.
    pub timeout_rate_threshold: f64,
    /// How long to wait before attempting recovery (Half-Open state).
    pub recovery_timeout: Duration,
    /// Number of successful requests needed in Half-Open to close circuit.
    pub success_threshold: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            failure_window: Duration::from_secs(60),
            timeout_rate_threshold: 0.5, // 50% timeout rate
            recovery_timeout: Duration::from_secs(30),
            success_threshold: 2,
        }
    }
}

/// Circuit breaker state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation - requests pass through.
    Closed,
    /// Too many failures - requests fail fast.
    Open,
    /// Testing recovery - limited requests pass through.
    HalfOpen,
}

/// Internal state tracking for the circuit breaker.
#[derive(Debug)]
struct CircuitStats {
    state: CircuitState,
    failure_count: u32,
    success_count: u32,
    last_failure_time: Option<Instant>,
    opened_at: Option<Instant>,
}

impl Default for CircuitStats {
    fn default() -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            success_count: 0,
            last_failure_time: None,
            opened_at: None,
        }
    }
}

/// Circuit breaker for fault tolerance.
///
/// Tracks failure rates and automatically opens the circuit to prevent
/// cascading failures when a provider is unhealthy.
#[derive(Clone)]
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    stats: Arc<RwLock<CircuitStats>>,
    state_changes: Arc<AtomicU64>, // Metric: count of state transitions
}

impl CircuitBreaker {
    /// Creates a new circuit breaker with the given configuration.
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            stats: Arc::new(RwLock::new(CircuitStats::default())),
            state_changes: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Creates a circuit breaker with default configuration.
    pub fn default() -> Self {
        Self::new(CircuitBreakerConfig::default())
    }

    /// Checks if a request can proceed.
    ///
    /// Returns `Ok(())` if request can proceed, `Err` if circuit is open.
    pub async fn before_request(&self) -> Result<(), ProviderError> {
        let mut stats = self.stats.write().await;

        // Check if we should transition from Open -> Half-Open
        if stats.state == CircuitState::Open {
            if let Some(opened_at) = stats.opened_at {
                if opened_at.elapsed() >= self.config.recovery_timeout {
                    info!("Circuit breaker transitioning Open -> Half-Open (recovery attempt)");
                    stats.state = CircuitState::HalfOpen;
                    stats.success_count = 0;
                    self.state_changes.fetch_add(1, Ordering::Relaxed);
                } else {
                    // Circuit still open, fail fast
                    return Err(ProviderError::RequestFailed(
                        "Circuit breaker is open".to_string(),
                    ));
                }
            }
        }

        // Reset failure count if outside failure window
        if let Some(last_failure) = stats.last_failure_time {
            if last_failure.elapsed() >= self.config.failure_window {
                stats.failure_count = 0;
            }
        }

        Ok(())
    }

    /// Records a successful request.
    pub async fn record_success(&self) {
        let mut stats = self.stats.write().await;

        match stats.state {
            CircuitState::Closed => {
                // Normal operation - no action needed
            }
            CircuitState::HalfOpen => {
                // In Half-Open, count successes to potentially close circuit
                stats.success_count += 1;
                if stats.success_count >= self.config.success_threshold {
                    info!(
                        successes = stats.success_count,
                        "Circuit breaker transitioning Half-Open -> Closed (recovered)"
                    );
                    stats.state = CircuitState::Closed;
                    stats.failure_count = 0;
                    stats.success_count = 0;
                    stats.opened_at = None;
                    self.state_changes.fetch_add(1, Ordering::Relaxed);
                }
            }
            CircuitState::Open => {
                // Should not happen (before_request should prevent this)
                warn!("Received success in Open state - this should not happen");
            }
        }
    }

    /// Records a failed request.
    pub async fn record_failure(&self, error: &ProviderError) {
        let mut stats = self.stats.write().await;

        // Only count retryable failures (timeouts, 5xx) for circuit breaking
        if !is_circuit_breaking_error(error) {
            return;
        }

        stats.failure_count += 1;
        stats.last_failure_time = Some(Instant::now());

        match stats.state {
            CircuitState::Closed => {
                // Check if we should open the circuit
                if stats.failure_count >= self.config.failure_threshold {
                    warn!(
                        failures = stats.failure_count,
                        "Circuit breaker transitioning Closed -> Open (failure threshold reached)"
                    );
                    stats.state = CircuitState::Open;
                    stats.opened_at = Some(Instant::now());
                    stats.success_count = 0;
                    self.state_changes.fetch_add(1, Ordering::Relaxed);
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in Half-Open immediately opens circuit again
                warn!("Circuit breaker transitioning Half-Open -> Open (recovery failed)");
                stats.state = CircuitState::Open;
                stats.opened_at = Some(Instant::now());
                stats.success_count = 0;
                self.state_changes.fetch_add(1, Ordering::Relaxed);
            }
            CircuitState::Open => {
                // Already open - no action needed
            }
        }
    }

    /// Returns the current circuit state.
    pub async fn state(&self) -> CircuitState {
        self.stats.read().await.state
    }

    /// Returns metrics about state transitions (for observability).
    pub fn state_change_count(&self) -> u64 {
        self.state_changes.load(Ordering::Relaxed)
    }

    /// Manually resets the circuit breaker to Closed state.
    ///
    /// Useful for testing or manual intervention.
    pub async fn reset(&self) {
        let mut stats = self.stats.write().await;
        info!("Manually resetting circuit breaker to Closed state");
        stats.state = CircuitState::Closed;
        stats.failure_count = 0;
        stats.success_count = 0;
        stats.last_failure_time = None;
        stats.opened_at = None;
    }
}

/// Determines if an error should count towards circuit breaker failure threshold.
///
/// Only transient errors (timeouts, 5xx) trigger circuit breaking.
/// Client errors (4xx, auth) do not, since they won't be fixed by circuit breaking.
fn is_circuit_breaking_error(error: &ProviderError) -> bool {
    match error {
        ProviderError::Timeout => true,
        ProviderError::RequestFailed(_) => true,
        ProviderError::ApiError { status, .. } => *status >= 500 && *status < 600,
        ProviderError::RateLimited => false, // Rate limiting is handled separately
        ProviderError::AuthError(_) => false, // Auth errors won't be fixed by circuit breaking
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_starts_closed() {
        let cb = CircuitBreaker::default();
        assert_eq!(cb.state().await, CircuitState::Closed);
        assert!(cb.before_request().await.is_ok());
    }

    #[tokio::test]
    async fn test_circuit_opens_after_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);

        // Record 3 failures
        for _ in 0..3 {
            cb.record_failure(&ProviderError::Timeout).await;
        }

        assert_eq!(cb.state().await, CircuitState::Open);
        assert!(cb.before_request().await.is_err());
    }

    #[tokio::test]
    async fn test_circuit_ignores_non_circuit_breaking_errors() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);

        // Record auth errors (should not count)
        for _ in 0..5 {
            cb.record_failure(&ProviderError::AuthError("invalid".to_string()))
                .await;
        }

        assert_eq!(cb.state().await, CircuitState::Closed);
        assert!(cb.before_request().await.is_ok());
    }

    #[tokio::test]
    async fn test_circuit_transitions_to_half_open() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            recovery_timeout: Duration::from_millis(50),
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);

        // Open the circuit
        cb.record_failure(&ProviderError::Timeout).await;
        cb.record_failure(&ProviderError::Timeout).await;
        assert_eq!(cb.state().await, CircuitState::Open);

        // Wait for recovery timeout
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Next request should transition to Half-Open
        assert!(cb.before_request().await.is_ok());
        assert_eq!(cb.state().await, CircuitState::HalfOpen);
    }

    #[tokio::test]
    async fn test_circuit_closes_after_successes_in_half_open() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            recovery_timeout: Duration::from_millis(50),
            success_threshold: 2,
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);

        // Open the circuit
        cb.record_failure(&ProviderError::Timeout).await;
        cb.record_failure(&ProviderError::Timeout).await;
        assert_eq!(cb.state().await, CircuitState::Open);

        // Wait and transition to Half-Open
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(cb.before_request().await.is_ok());
        assert_eq!(cb.state().await, CircuitState::HalfOpen);

        // Record successful requests
        cb.record_success().await;
        cb.record_success().await;

        // Should now be Closed
        assert_eq!(cb.state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_reopens_on_failure_in_half_open() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            recovery_timeout: Duration::from_millis(50),
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);

        // Open the circuit
        cb.record_failure(&ProviderError::Timeout).await;
        cb.record_failure(&ProviderError::Timeout).await;

        // Wait and transition to Half-Open
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(cb.before_request().await.is_ok());

        // Failure in Half-Open should reopen circuit
        cb.record_failure(&ProviderError::Timeout).await;
        assert_eq!(cb.state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn test_manual_reset() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);

        // Open the circuit
        cb.record_failure(&ProviderError::Timeout).await;
        cb.record_failure(&ProviderError::Timeout).await;
        assert_eq!(cb.state().await, CircuitState::Open);

        // Manual reset
        cb.reset().await;
        assert_eq!(cb.state().await, CircuitState::Closed);
        assert!(cb.before_request().await.is_ok());
    }

    #[tokio::test]
    async fn test_state_change_metrics() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            recovery_timeout: Duration::from_millis(50),
            success_threshold: 2,
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);

        let initial_count = cb.state_change_count();

        // Closed -> Open
        cb.record_failure(&ProviderError::Timeout).await;
        cb.record_failure(&ProviderError::Timeout).await;
        assert_eq!(cb.state_change_count(), initial_count + 1);

        // Open -> Half-Open
        tokio::time::sleep(Duration::from_millis(60)).await;
        let _ = cb.before_request().await;
        assert_eq!(cb.state_change_count(), initial_count + 2);

        // Half-Open -> Closed
        cb.record_success().await;
        cb.record_success().await;
        assert_eq!(cb.state_change_count(), initial_count + 3);
    }
}
