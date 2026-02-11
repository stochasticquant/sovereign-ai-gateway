//! Retry logic with exponential backoff and jitter.
//!
//! Only retries transient errors (5xx, timeouts, connection errors).
//! Never retries 4xx client errors since they won't succeed on retry.

use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};

use crate::traits::ProviderError;

/// Configuration for retry behavior.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (0 = no retries).
    pub max_retries: u32,
    /// Base delay for exponential backoff.
    pub base_delay: Duration,
    /// Maximum delay between retries (cap for exponential growth).
    pub max_delay: Duration,
    /// Whether to add random jitter to delays (reduces thundering herd).
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            jitter: true,
        }
    }
}

/// Executes an async operation with retry logic.
///
/// Returns the successful result or the last error encountered.
pub async fn with_retry<F, Fut, T>(config: &RetryConfig, mut operation: F) -> Result<T, ProviderError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, ProviderError>>,
{
    let mut attempt = 0;

    loop {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    debug!(attempt, "Operation succeeded after retry");
                }
                return Ok(result);
            }
            Err(err) => {
                // Check if error is retryable
                if !is_retryable(&err) {
                    debug!(?err, "Error is not retryable, failing immediately");
                    return Err(err);
                }

                // Check if we've exhausted retries
                if attempt >= config.max_retries {
                    warn!(
                        attempt,
                        ?err,
                        "Exhausted all retry attempts, returning last error"
                    );
                    return Err(err);
                }

                // Calculate delay with exponential backoff
                let delay = calculate_delay(config, attempt);
                warn!(
                    attempt,
                    ?err,
                    delay_ms = delay.as_millis(),
                    "Operation failed, retrying after delay"
                );

                attempt += 1;

                // Wait before retry
                sleep(delay).await;
            }
        }
    }
}

/// Determines if an error should be retried.
///
/// Retryable errors:
/// - Timeouts
/// - Rate limiting (with backoff)
/// - 5xx server errors
///
/// Non-retryable errors:
/// - Authentication failures (4xx)
/// - Malformed requests (4xx)
fn is_retryable(error: &ProviderError) -> bool {
    match error {
        ProviderError::Timeout => true,
        ProviderError::RateLimited => true,
        ProviderError::RequestFailed(_) => true, // Network errors are retryable
        ProviderError::ApiError { status, .. } => {
            // Retry on 5xx server errors, not on 4xx client errors
            *status >= 500 && *status < 600
        }
        ProviderError::AuthError(_) => false, // Don't retry auth failures
    }
}

/// Calculates the delay for a given retry attempt using exponential backoff.
///
/// Formula: min(base_delay * 2^attempt, max_delay) [+ jitter]
fn calculate_delay(config: &RetryConfig, attempt: u32) -> Duration {
    // Calculate exponential backoff: base * 2^attempt
    let exponential_delay = config
        .base_delay
        .saturating_mul(2_u32.saturating_pow(attempt));

    // Cap at max_delay
    let capped_delay = exponential_delay.min(config.max_delay);

    // Add jitter if enabled (0-50% random variation)
    if config.jitter {
        add_jitter(capped_delay)
    } else {
        capped_delay
    }
}

/// Adds random jitter (0-50%) to a duration to prevent thundering herd.
fn add_jitter(duration: Duration) -> Duration {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let jitter_factor = rng.gen_range(0.5..1.0);
    duration.mul_f64(jitter_factor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_retryable() {
        // Retryable errors
        assert!(is_retryable(&ProviderError::Timeout));
        assert!(is_retryable(&ProviderError::RateLimited));
        assert!(is_retryable(&ProviderError::RequestFailed(
            "connection reset".to_string()
        )));
        assert!(is_retryable(&ProviderError::ApiError {
            status: 500,
            message: "Internal Server Error".to_string(),
        }));
        assert!(is_retryable(&ProviderError::ApiError {
            status: 503,
            message: "Service Unavailable".to_string(),
        }));

        // Non-retryable errors
        assert!(!is_retryable(&ProviderError::AuthError(
            "invalid key".to_string()
        )));
        assert!(!is_retryable(&ProviderError::ApiError {
            status: 400,
            message: "Bad Request".to_string(),
        }));
        assert!(!is_retryable(&ProviderError::ApiError {
            status: 401,
            message: "Unauthorized".to_string(),
        }));
        assert!(!is_retryable(&ProviderError::ApiError {
            status: 404,
            message: "Not Found".to_string(),
        }));
    }

    #[test]
    fn test_calculate_delay() {
        let config = RetryConfig {
            max_retries: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            jitter: false, // Disable jitter for predictable tests
        };

        // Attempt 0: 100ms * 2^0 = 100ms
        assert_eq!(calculate_delay(&config, 0), Duration::from_millis(100));

        // Attempt 1: 100ms * 2^1 = 200ms
        assert_eq!(calculate_delay(&config, 1), Duration::from_millis(200));

        // Attempt 2: 100ms * 2^2 = 400ms
        assert_eq!(calculate_delay(&config, 2), Duration::from_millis(400));

        // Attempt 3: 100ms * 2^3 = 800ms
        assert_eq!(calculate_delay(&config, 3), Duration::from_millis(800));

        // Verify capping at max_delay
        let high_attempt_config = RetryConfig {
            max_retries: 10,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(2),
            jitter: false,
        };
        // Attempt 5: 100ms * 2^5 = 3200ms, but capped at 2000ms
        assert_eq!(
            calculate_delay(&high_attempt_config, 5),
            Duration::from_secs(2)
        );
    }

    #[tokio::test]
    async fn test_with_retry_success_on_first_attempt() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let config = RetryConfig::default();
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = with_retry(&config, move || {
            let count = call_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ProviderError>(42)
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_with_retry_success_after_retries() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let config = RetryConfig {
            max_retries: 3,
            base_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            jitter: false,
        };
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = with_retry(&config, move || {
            let count = call_count_clone.clone();
            async move {
                let current = count.fetch_add(1, Ordering::SeqCst) + 1;
                if current < 3 {
                    Err(ProviderError::Timeout)
                } else {
                    Ok::<_, ProviderError>(42)
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_with_retry_exhausted() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let config = RetryConfig {
            max_retries: 2,
            base_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            jitter: false,
        };
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = with_retry(&config, move || {
            let count = call_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Err::<i32, _>(ProviderError::Timeout)
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(call_count.load(Ordering::SeqCst), 3); // Initial attempt + 2 retries
    }

    #[tokio::test]
    async fn test_with_retry_non_retryable_error() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let config = RetryConfig::default();
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = with_retry(&config, move || {
            let count = call_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Err::<i32, _>(ProviderError::AuthError("invalid key".to_string()))
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(call_count.load(Ordering::SeqCst), 1); // No retries for auth errors
    }
}
