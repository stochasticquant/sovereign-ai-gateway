//! Provider registry with health monitoring.
//!
//! Central registry for managing all configured LLM providers.
//! Tracks health status and automatically excludes unhealthy providers
//! from routing decisions.
//!
//! # Architecture
//!
//! - **Registration**: Providers are registered at startup with unique IDs
//! - **Health Monitoring**: Background task periodically checks all providers
//! - **Status Tracking**: Maintains health status and last check time per provider
//! - **Smart Routing**: Only returns healthy providers for request routing
//!
//! # Example
//!
//! ```no_run
//! use provider_adapters::registry::{ProviderRegistry, HealthCheckConfig};
//! use provider_adapters::openai::{OpenAiProvider, OpenAiConfig};
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create registry with health monitoring
//! let registry = ProviderRegistry::new(HealthCheckConfig::default());
//!
//! // Register providers
//! let openai_config = OpenAiConfig {
//!     api_key: "sk-...".to_string(),
//!     ..Default::default()
//! };
//! let provider = Arc::new(OpenAiProvider::new(openai_config)?);
//! registry.register("openai-primary", provider).await;
//!
//! // Start background health monitoring
//! let registry_clone = registry.clone();
//! tokio::spawn(async move {
//!     registry_clone.start_health_checks().await;
//! });
//!
//! // Get healthy providers for routing
//! let healthy = registry.get_healthy_providers().await;
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::traits::{LlmProvider, ProviderError};

/// Configuration for health check behavior.
#[derive(Debug, Clone)]
pub struct HealthCheckConfig {
    /// Interval between health check cycles.
    pub check_interval: Duration,
    /// Timeout for individual health check requests.
    pub check_timeout: Duration,
    /// Number of consecutive failures before marking unhealthy.
    pub failure_threshold: u32,
    /// Number of consecutive successes needed to mark healthy again.
    pub success_threshold: u32,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(30),
            check_timeout: Duration::from_secs(5),
            failure_threshold: 3,
            success_threshold: 2,
        }
    }
}

/// Health status of a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    /// Provider is healthy and available for requests.
    Healthy,
    /// Provider is unhealthy and should not receive requests.
    Unhealthy,
    /// Health status is unknown (not yet checked).
    Unknown,
}

/// Internal tracking for provider health.
#[derive(Debug, Clone)]
struct ProviderHealth {
    status: HealthStatus,
    consecutive_failures: u32,
    consecutive_successes: u32,
    last_check: Option<Instant>,
    last_error: Option<String>,
}

impl Default for ProviderHealth {
    fn default() -> Self {
        Self {
            status: HealthStatus::Unknown,
            consecutive_failures: 0,
            consecutive_successes: 0,
            last_check: None,
            last_error: None,
        }
    }
}

/// A registered provider with its health tracking.
struct RegisteredProvider {
    provider: Arc<dyn LlmProvider>,
    health: ProviderHealth,
}

/// Central registry for LLM providers with health monitoring.
///
/// Thread-safe registry that manages multiple provider instances,
/// tracks their health status, and runs periodic health checks.
#[derive(Clone)]
pub struct ProviderRegistry {
    config: HealthCheckConfig,
    providers: Arc<RwLock<HashMap<String, RegisteredProvider>>>,
}

impl ProviderRegistry {
    /// Creates a new provider registry with the given health check configuration.
    pub fn new(config: HealthCheckConfig) -> Self {
        Self {
            config,
            providers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Registers a provider with a unique ID.
    ///
    /// If a provider with the same ID already exists, it will be replaced.
    pub async fn register(&self, id: impl Into<String>, provider: Arc<dyn LlmProvider>) {
        let id = id.into();
        let mut providers = self.providers.write().await;

        info!(
            provider_id = %id,
            provider_name = %provider.metadata().name,
            "Registering provider"
        );

        providers.insert(
            id.clone(),
            RegisteredProvider {
                provider,
                health: ProviderHealth::default(),
            },
        );
    }

    /// Unregisters a provider by ID.
    ///
    /// Returns `true` if the provider was found and removed.
    pub async fn unregister(&self, id: &str) -> bool {
        let mut providers = self.providers.write().await;
        let removed = providers.remove(id).is_some();

        if removed {
            info!(provider_id = %id, "Unregistered provider");
        } else {
            warn!(provider_id = %id, "Attempted to unregister unknown provider");
        }

        removed
    }

    /// Gets a provider by ID, regardless of health status.
    pub async fn get(&self, id: &str) -> Option<Arc<dyn LlmProvider>> {
        let providers = self.providers.read().await;
        providers.get(id).map(|rp| rp.provider.clone())
    }

    /// Gets all registered provider IDs.
    pub async fn list_ids(&self) -> Vec<String> {
        let providers = self.providers.read().await;
        providers.keys().cloned().collect()
    }

    /// Gets all healthy providers (status = Healthy).
    ///
    /// Returns a map of provider ID -> provider instance.
    /// Only includes providers that have passed recent health checks.
    pub async fn get_healthy_providers(&self) -> HashMap<String, Arc<dyn LlmProvider>> {
        let providers = self.providers.read().await;
        providers
            .iter()
            .filter(|(_, rp)| rp.health.status == HealthStatus::Healthy)
            .map(|(id, rp)| (id.clone(), rp.provider.clone()))
            .collect()
    }

    /// Gets the health status of a specific provider.
    pub async fn get_health_status(&self, id: &str) -> Option<HealthStatus> {
        let providers = self.providers.read().await;
        providers.get(id).map(|rp| rp.health.status)
    }

    /// Gets detailed health information for all providers.
    ///
    /// Returns a map of provider ID -> (status, last_check, last_error).
    pub async fn get_all_health_info(&self) -> HashMap<String, (HealthStatus, Option<Instant>, Option<String>)> {
        let providers = self.providers.read().await;
        providers
            .iter()
            .map(|(id, rp)| {
                (
                    id.clone(),
                    (
                        rp.health.status,
                        rp.health.last_check,
                        rp.health.last_error.clone(),
                    ),
                )
            })
            .collect()
    }

    /// Manually triggers a health check for a specific provider.
    ///
    /// Returns the new health status.
    pub async fn check_provider_health(&self, id: &str) -> Result<HealthStatus, ProviderError> {
        let provider = {
            let providers = self.providers.read().await;
            providers
                .get(id)
                .map(|rp| rp.provider.clone())
                .ok_or_else(|| {
                    ProviderError::RequestFailed(format!("Provider '{}' not found", id))
                })?
        };

        // Perform health check with timeout
        let health_result = tokio::time::timeout(
            self.config.check_timeout,
            provider.health_check(),
        )
        .await;

        // Update health status based on result
        let mut providers = self.providers.write().await;
        if let Some(rp) = providers.get_mut(id) {
            rp.health.last_check = Some(Instant::now());

            match health_result {
                Ok(Ok(())) => {
                    // Health check succeeded
                    rp.health.consecutive_successes += 1;
                    rp.health.consecutive_failures = 0;
                    rp.health.last_error = None;

                    // Update status if enough consecutive successes
                    let previous_status = rp.health.status;
                    if rp.health.consecutive_successes >= self.config.success_threshold {
                        rp.health.status = HealthStatus::Healthy;
                    }

                    if previous_status != HealthStatus::Healthy && rp.health.status == HealthStatus::Healthy {
                        info!(
                            provider_id = %id,
                            consecutive_successes = rp.health.consecutive_successes,
                            "Provider is now healthy"
                        );
                    }

                    Ok(rp.health.status)
                }
                Ok(Err(err)) => {
                    // Health check failed
                    let error_msg = format!("{}", err);

                    rp.health.consecutive_failures += 1;
                    rp.health.consecutive_successes = 0;
                    rp.health.last_error = Some(error_msg.clone());

                    // Update status if enough consecutive failures
                    let previous_status = rp.health.status;
                    if rp.health.consecutive_failures >= self.config.failure_threshold {
                        rp.health.status = HealthStatus::Unhealthy;
                    }

                    if previous_status != HealthStatus::Unhealthy && rp.health.status == HealthStatus::Unhealthy {
                        warn!(
                            provider_id = %id,
                            consecutive_failures = rp.health.consecutive_failures,
                            error = %error_msg,
                            "Provider is now unhealthy"
                        );
                    }

                    Err(ProviderError::RequestFailed(error_msg))
                }
                Err(_) => {
                    // Health check timed out
                    let error_msg = "Health check timed out".to_string();

                    rp.health.consecutive_failures += 1;
                    rp.health.consecutive_successes = 0;
                    rp.health.last_error = Some(error_msg.clone());

                    // Update status if enough consecutive failures
                    let previous_status = rp.health.status;
                    if rp.health.consecutive_failures >= self.config.failure_threshold {
                        rp.health.status = HealthStatus::Unhealthy;
                    }

                    if previous_status != HealthStatus::Unhealthy && rp.health.status == HealthStatus::Unhealthy {
                        warn!(
                            provider_id = %id,
                            consecutive_failures = rp.health.consecutive_failures,
                            error = %error_msg,
                            "Provider is now unhealthy"
                        );
                    }

                    Err(ProviderError::RequestFailed(error_msg))
                }
            }
        } else {
            Err(ProviderError::RequestFailed(format!(
                "Provider '{}' not found",
                id
            )))
        }
    }

    /// Runs a single health check cycle for all providers.
    async fn check_all_providers(&self) {
        let provider_ids = self.list_ids().await;

        debug!(
            provider_count = provider_ids.len(),
            "Starting health check cycle"
        );

        // Check all providers concurrently
        let mut check_tasks = Vec::new();
        for id in provider_ids {
            let registry = self.clone();
            check_tasks.push(tokio::spawn(async move {
                if let Err(err) = registry.check_provider_health(&id).await {
                    debug!(
                        provider_id = %id,
                        error = %err,
                        "Health check failed"
                    );
                }
            }));
        }

        // Wait for all checks to complete
        for task in check_tasks {
            if let Err(err) = task.await {
                error!(error = %err, "Health check task panicked");
            }
        }

        // Log summary
        let health_info = self.get_all_health_info().await;
        let healthy_count = health_info
            .values()
            .filter(|(status, _, _)| *status == HealthStatus::Healthy)
            .count();
        let unhealthy_count = health_info
            .values()
            .filter(|(status, _, _)| *status == HealthStatus::Unhealthy)
            .count();

        debug!(
            total = health_info.len(),
            healthy = healthy_count,
            unhealthy = unhealthy_count,
            "Health check cycle completed"
        );
    }

    /// Starts the background health check loop.
    ///
    /// This method runs indefinitely and should be spawned as a background task.
    /// It periodically checks all registered providers according to the
    /// configured `check_interval`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use provider_adapters::registry::{ProviderRegistry, HealthCheckConfig};
    /// # async fn example() {
    /// let registry = ProviderRegistry::new(HealthCheckConfig::default());
    /// let registry_clone = registry.clone();
    ///
    /// // Spawn background task
    /// tokio::spawn(async move {
    ///     registry_clone.start_health_checks().await;
    /// });
    /// # }
    /// ```
    pub async fn start_health_checks(&self) {
        info!(
            interval_secs = self.config.check_interval.as_secs(),
            "Starting background health check loop"
        );

        loop {
            self.check_all_providers().await;
            sleep(self.config.check_interval).await;
        }
    }

    /// Performs an immediate health check on all providers and waits for completion.
    ///
    /// Useful for initial health verification at startup.
    pub async fn check_all_now(&self) {
        info!("Performing immediate health check on all providers");
        self.check_all_providers().await;
    }
}

impl std::fmt::Debug for ProviderRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderRegistry")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::{LlmRequest, LlmResponse, ProviderMetadata};
    use async_trait::async_trait;

    /// Mock provider for testing.
    #[derive(Clone)]
    struct MockProvider {
        id: String,
        healthy: Arc<RwLock<bool>>,
    }

    impl MockProvider {
        fn new(id: &str, healthy: bool) -> Self {
            Self {
                id: id.to_string(),
                healthy: Arc::new(RwLock::new(healthy)),
            }
        }

        async fn set_healthy(&self, healthy: bool) {
            *self.healthy.write().await = healthy;
        }
    }

    #[async_trait]
    impl LlmProvider for MockProvider {
        async fn send(&self, _request: &LlmRequest) -> Result<LlmResponse, ProviderError> {
            unimplemented!("Not needed for health check tests")
        }

        async fn health_check(&self) -> Result<(), ProviderError> {
            if *self.healthy.read().await {
                Ok(())
            } else {
                Err(ProviderError::RequestFailed("Provider is down".to_string()))
            }
        }

        fn metadata(&self) -> &ProviderMetadata {
            Box::leak(Box::new(ProviderMetadata {
                id: self.id.clone(),
                name: format!("Mock Provider {}", self.id),
                region: "test".to_string(),
                models: vec!["mock-model".to_string()],
            }))
        }
    }

    #[tokio::test]
    async fn test_register_and_get_provider() {
        let registry = ProviderRegistry::new(HealthCheckConfig::default());
        let provider = Arc::new(MockProvider::new("test", true));

        registry.register("test-provider", provider.clone()).await;

        let retrieved = registry.get("test-provider").await;
        assert!(retrieved.is_some());
    }

    #[tokio::test]
    async fn test_unregister_provider() {
        let registry = ProviderRegistry::new(HealthCheckConfig::default());
        let provider = Arc::new(MockProvider::new("test", true));

        registry.register("test-provider", provider).await;
        assert!(registry.get("test-provider").await.is_some());

        let removed = registry.unregister("test-provider").await;
        assert!(removed);
        assert!(registry.get("test-provider").await.is_none());
    }

    #[tokio::test]
    async fn test_health_check_healthy_provider() {
        let config = HealthCheckConfig {
            success_threshold: 2,
            failure_threshold: 3,
            ..Default::default()
        };
        let registry = ProviderRegistry::new(config);
        let provider = Arc::new(MockProvider::new("test", true));

        registry.register("test-provider", provider).await;

        // First check - still unknown (needs 2 successes)
        let status = registry.check_provider_health("test-provider").await.unwrap();
        assert_eq!(status, HealthStatus::Unknown);

        // Second check - should become healthy
        let status = registry.check_provider_health("test-provider").await.unwrap();
        assert_eq!(status, HealthStatus::Healthy);

        // Verify it appears in healthy providers
        let healthy = registry.get_healthy_providers().await;
        assert_eq!(healthy.len(), 1);
        assert!(healthy.contains_key("test-provider"));
    }

    #[tokio::test]
    async fn test_health_check_unhealthy_provider() {
        let config = HealthCheckConfig {
            success_threshold: 2,
            failure_threshold: 2,
            check_timeout: Duration::from_millis(100),
            ..Default::default()
        };
        let registry = ProviderRegistry::new(config);
        let provider = Arc::new(MockProvider::new("test", false));

        registry.register("test-provider", provider).await;

        // First failure
        let result = registry.check_provider_health("test-provider").await;
        assert!(result.is_err());
        let status = registry.get_health_status("test-provider").await.unwrap();
        assert_eq!(status, HealthStatus::Unknown);

        // Second failure - should become unhealthy
        let result = registry.check_provider_health("test-provider").await;
        assert!(result.is_err());
        let status = registry.get_health_status("test-provider").await.unwrap();
        assert_eq!(status, HealthStatus::Unhealthy);

        // Should not appear in healthy providers
        let healthy = registry.get_healthy_providers().await;
        assert_eq!(healthy.len(), 0);
    }

    #[tokio::test]
    async fn test_provider_recovery() {
        let config = HealthCheckConfig {
            success_threshold: 2,
            failure_threshold: 2,
            ..Default::default()
        };
        let registry = ProviderRegistry::new(config);
        let provider = Arc::new(MockProvider::new("test", false));

        registry.register("test-provider", provider.clone()).await;

        // Make provider unhealthy
        let _ = registry.check_provider_health("test-provider").await;
        let _ = registry.check_provider_health("test-provider").await;
        assert_eq!(
            registry.get_health_status("test-provider").await.unwrap(),
            HealthStatus::Unhealthy
        );

        // Fix the provider
        provider.set_healthy(true).await;

        // First success after recovery
        let _ = registry.check_provider_health("test-provider").await;
        assert_eq!(
            registry.get_health_status("test-provider").await.unwrap(),
            HealthStatus::Unhealthy
        ); // Still unhealthy

        // Second success - should become healthy
        let _ = registry.check_provider_health("test-provider").await;
        assert_eq!(
            registry.get_health_status("test-provider").await.unwrap(),
            HealthStatus::Healthy
        );
    }

    #[tokio::test]
    async fn test_list_ids() {
        let registry = ProviderRegistry::new(HealthCheckConfig::default());

        registry
            .register("provider-1", Arc::new(MockProvider::new("1", true)))
            .await;
        registry
            .register("provider-2", Arc::new(MockProvider::new("2", true)))
            .await;
        registry
            .register("provider-3", Arc::new(MockProvider::new("3", true)))
            .await;

        let ids = registry.list_ids().await;
        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&"provider-1".to_string()));
        assert!(ids.contains(&"provider-2".to_string()));
        assert!(ids.contains(&"provider-3".to_string()));
    }

    #[tokio::test]
    async fn test_get_all_health_info() {
        let registry = ProviderRegistry::new(HealthCheckConfig::default());
        let provider = Arc::new(MockProvider::new("test", true));

        registry.register("test-provider", provider).await;

        // Initial state
        let info = registry.get_all_health_info().await;
        assert_eq!(info.len(), 1);
        let (status, last_check, last_error) = &info["test-provider"];
        assert_eq!(*status, HealthStatus::Unknown);
        assert!(last_check.is_none());
        assert!(last_error.is_none());

        // After health check
        let _ = registry.check_provider_health("test-provider").await;
        let info = registry.get_all_health_info().await;
        let (status, last_check, last_error) = &info["test-provider"];
        assert_eq!(*status, HealthStatus::Unknown); // Still unknown (needs 2 successes)
        assert!(last_check.is_some());
        assert!(last_error.is_none());
    }

    #[tokio::test]
    async fn test_check_all_now() {
        let config = HealthCheckConfig {
            success_threshold: 2,
            ..Default::default()
        };
        let registry = ProviderRegistry::new(config);

        registry
            .register("provider-1", Arc::new(MockProvider::new("1", true)))
            .await;
        registry
            .register("provider-2", Arc::new(MockProvider::new("2", true)))
            .await;

        // Run two cycles to make all providers healthy
        registry.check_all_now().await;
        registry.check_all_now().await;

        let healthy = registry.get_healthy_providers().await;
        assert_eq!(healthy.len(), 2);
    }
}
