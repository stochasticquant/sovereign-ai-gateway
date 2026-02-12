//! Policy file loader with hot-reload capability.
//!
//! Watches policy directory via `notify` crate and reloads policies atomically
//! when files change. Validates new policies before applying to prevent invalid
//! configurations from breaking the gateway.

use crate::schema::Policy;
use gateway_core::error::GatewayError;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Policy loader with hot-reload support.
pub struct PolicyLoader {
    /// Directory containing policy TOML files.
    policy_dir: PathBuf,

    /// Currently loaded policies, keyed by filename.
    policies: Arc<RwLock<HashMap<String, Policy>>>,

    /// File system watcher for hot-reload.
    #[allow(dead_code)]
    watcher: Option<RecommendedWatcher>,
}

impl PolicyLoader {
    /// Create a new policy loader for the given directory.
    pub fn new(policy_dir: impl Into<PathBuf>) -> Result<Self, GatewayError> {
        let policy_dir = policy_dir.into();

        if !policy_dir.exists() {
            return Err(GatewayError::ConfigError(format!(
                "Policy directory does not exist: {}",
                policy_dir.display()
            )));
        }

        if !policy_dir.is_dir() {
            return Err(GatewayError::ConfigError(format!(
                "Policy path is not a directory: {}",
                policy_dir.display()
            )));
        }

        Ok(Self {
            policy_dir,
            policies: Arc::new(RwLock::new(HashMap::new())),
            watcher: None,
        })
    }

    /// Load all policies from the policy directory.
    pub async fn load_all(&mut self) -> Result<usize, GatewayError> {
        info!("Loading policies from {}", self.policy_dir.display());

        let entries = std::fs::read_dir(&self.policy_dir).map_err(|e| {
            GatewayError::ConfigError(format!(
                "Failed to read policy directory {}: {}",
                self.policy_dir.display(),
                e
            ))
        })?;

        let mut loaded_count = 0;
        let mut new_policies = HashMap::new();

        for entry in entries {
            let entry = entry.map_err(|e| {
                GatewayError::ConfigError(format!("Failed to read directory entry: {}", e))
            })?;

            let path = entry.path();

            // Skip if not a TOML file
            if !path.extension().map(|e| e == "toml").unwrap_or(false) {
                continue;
            }

            // Skip if in examples subdirectory (those are templates)
            if path.components().any(|c| c.as_os_str() == "examples") {
                debug!("Skipping example policy: {}", path.display());
                continue;
            }

            match self.load_policy(&path).await {
                Ok(policy) => {
                    let filename = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();

                    info!(
                        "Loaded policy: {} ({})",
                        filename, policy.metadata.description
                    );
                    new_policies.insert(filename, policy);
                    loaded_count += 1;
                }
                Err(e) => {
                    error!("Failed to load policy {}: {}", path.display(), e);
                    // Continue loading other policies
                }
            }
        }

        // Atomically replace all policies
        let mut policies = self.policies.write().await;
        *policies = new_policies;

        info!("Loaded {} policies successfully", loaded_count);
        Ok(loaded_count)
    }

    /// Load a single policy from a file.
    pub async fn load_policy(&self, path: &Path) -> Result<Policy, GatewayError> {
        debug!("Loading policy from {}", path.display());

        let content = tokio::fs::read_to_string(path).await.map_err(|e| {
            GatewayError::ConfigError(format!(
                "Failed to read policy file {}: {}",
                path.display(),
                e
            ))
        })?;

        let policy: Policy = toml::from_str(&content).map_err(|e| {
            GatewayError::ConfigError(format!(
                "Failed to parse policy TOML {}: {}",
                path.display(),
                e
            ))
        })?;

        // Validate the policy
        let validation = policy.validate();
        if !validation.valid {
            return Err(GatewayError::ConfigError(format!(
                "Policy validation failed for {}: {}",
                path.display(),
                validation.errors.join(", ")
            )));
        }

        // Log warnings but don't fail
        for warning in &validation.warnings {
            warn!("Policy {} warning: {}", path.display(), warning);
        }

        Ok(policy)
    }

    /// Start watching the policy directory for changes.
    pub async fn start_watching(&mut self) -> Result<(), GatewayError> {
        let policies = Arc::clone(&self.policies);
        let policy_dir = self.policy_dir.clone();

        let (tx, mut rx) = tokio::sync::mpsc::channel(100);

        // Create file watcher
        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            match res {
                Ok(event) => {
                    if let Err(e) = tx.blocking_send(event) {
                        error!("Failed to send file event: {}", e);
                    }
                }
                Err(e) => error!("File watch error: {}", e),
            }
        })
        .map_err(|e| GatewayError::ConfigError(format!("Failed to create file watcher: {}", e)))?;

        watcher
            .watch(&self.policy_dir, RecursiveMode::NonRecursive)
            .map_err(|e| {
                GatewayError::ConfigError(format!(
                    "Failed to watch policy directory {}: {}",
                    self.policy_dir.display(),
                    e
                ))
            })?;

        info!(
            "Started watching policy directory: {}",
            self.policy_dir.display()
        );
        self.watcher = Some(watcher);

        // Spawn background task to handle file events
        tokio::spawn(async move {
            // Debounce timer to avoid reloading too frequently
            let mut debounce_timer;
            const DEBOUNCE_DURATION: Duration = Duration::from_millis(500);

            while let Some(event) = rx.recv().await {
                // Only care about write/create/remove events
                match event.kind {
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                        // Check if event is for a TOML file
                        let is_toml = event.paths.iter().any(|p| {
                            p.extension().map(|e| e == "toml").unwrap_or(false)
                                && !p.components().any(|c| c.as_os_str() == "examples")
                        });

                        if !is_toml {
                            continue;
                        }

                        // Debounce: wait for changes to settle
                        debounce_timer = Some(tokio::time::Instant::now());
                        tokio::time::sleep(DEBOUNCE_DURATION).await;

                        // Check if another event came in during debounce
                        if let Some(timer) = debounce_timer {
                            if timer.elapsed() < DEBOUNCE_DURATION {
                                continue; // More events coming, wait
                            }
                        }

                        info!("Policy file changed, reloading...");

                        // Reload all policies
                        match Self::reload_policies(&policy_dir, &policies).await {
                            Ok(count) => {
                                info!("Successfully reloaded {} policies", count);
                            }
                            Err(e) => {
                                error!("Failed to reload policies: {}", e);
                            }
                        }
                    }
                    _ => {} // Ignore other events
                }
            }
        });

        Ok(())
    }

    /// Reload all policies from disk (used by hot-reload watcher).
    async fn reload_policies(
        policy_dir: &Path,
        policies: &Arc<RwLock<HashMap<String, Policy>>>,
    ) -> Result<usize, GatewayError> {
        let entries = std::fs::read_dir(policy_dir).map_err(|e| {
            GatewayError::ConfigError(format!(
                "Failed to read policy directory {}: {}",
                policy_dir.display(),
                e
            ))
        })?;

        let mut loaded_count = 0;
        let mut new_policies = HashMap::new();

        for entry in entries {
            let entry = entry.map_err(|e| {
                GatewayError::ConfigError(format!("Failed to read directory entry: {}", e))
            })?;

            let path = entry.path();

            if !path.extension().map(|e| e == "toml").unwrap_or(false) {
                continue;
            }

            if path.components().any(|c| c.as_os_str() == "examples") {
                continue;
            }

            match Self::load_policy_sync(&path) {
                Ok(policy) => {
                    let filename = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();

                    new_policies.insert(filename, policy);
                    loaded_count += 1;
                }
                Err(e) => {
                    error!("Failed to reload policy {}: {}", path.display(), e);
                }
            }
        }

        // Atomically replace all policies
        let mut policies_lock = policies.write().await;
        *policies_lock = new_policies;

        Ok(loaded_count)
    }

    /// Load a policy synchronously (for use in sync context).
    fn load_policy_sync(path: &Path) -> Result<Policy, GatewayError> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            GatewayError::ConfigError(format!(
                "Failed to read policy file {}: {}",
                path.display(),
                e
            ))
        })?;

        let policy: Policy = toml::from_str(&content).map_err(|e| {
            GatewayError::ConfigError(format!(
                "Failed to parse policy TOML {}: {}",
                path.display(),
                e
            ))
        })?;

        let validation = policy.validate();
        if !validation.valid {
            return Err(GatewayError::ConfigError(format!(
                "Policy validation failed for {}: {}",
                path.display(),
                validation.errors.join(", ")
            )));
        }

        Ok(policy)
    }

    /// Get a copy of all currently loaded policies.
    pub async fn get_policies(&self) -> HashMap<String, Policy> {
        self.policies.read().await.clone()
    }

    /// Get a specific policy by filename.
    pub async fn get_policy(&self, filename: &str) -> Option<Policy> {
        self.policies.read().await.get(filename).cloned()
    }

    /// Get the number of loaded policies.
    pub async fn policy_count(&self) -> usize {
        self.policies.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_load_valid_policy() {
        let temp_dir = TempDir::new().unwrap();
        let policy_path = temp_dir.path().join("test.toml");

        let policy_content = r#"
[metadata]
version = "1.0"
description = "Test policy"

[data_rules]
public = "external_allowed"
internal = "regional_only"
confidential = "local_only"
restricted = "local_only"

[providers.local_llm]
region = "local"
allowed = true
priority = 1
models = ["llama-3"]

[quotas]
max_tokens_per_request = 4096
max_tokens_per_day = 1000000
burst_requests_per_minute = 60

[redaction]
mode = "irreversible"
block_categories = ["national_id"]
        "#;

        fs::write(&policy_path, policy_content).unwrap();

        let loader = PolicyLoader::new(temp_dir.path()).unwrap();
        let policy = loader.load_policy(&policy_path).await.unwrap();

        assert_eq!(policy.metadata.version, "1.0");
        assert_eq!(policy.metadata.description, "Test policy");
        assert!(policy.providers.contains_key("local_llm"));
    }

    #[tokio::test]
    async fn test_load_all_policies() {
        let temp_dir = TempDir::new().unwrap();

        // Create two policy files
        for i in 1..=2 {
            let policy_path = temp_dir.path().join(format!("policy{}.toml", i));
            let policy_content = format!(
                r#"
[metadata]
version = "1.0"
description = "Test policy {}"

[data_rules]
public = "external_allowed"
internal = "local_only"
confidential = "local_only"
restricted = "local_only"

[providers.local]
region = "local"
allowed = true
models = ["test"]

[quotas]
max_tokens_per_request = 100
max_tokens_per_day = 1000
burst_requests_per_minute = 10

[redaction]
mode = "irreversible"
block_categories = []
                "#,
                i
            );
            fs::write(&policy_path, policy_content).unwrap();
        }

        let mut loader = PolicyLoader::new(temp_dir.path()).unwrap();
        let count = loader.load_all().await.unwrap();

        assert_eq!(count, 2);
        assert_eq!(loader.policy_count().await, 2);
    }

    #[tokio::test]
    async fn test_reject_invalid_policy() {
        let temp_dir = TempDir::new().unwrap();
        let policy_path = temp_dir.path().join("invalid.toml");

        // Policy with no allowed providers (invalid)
        let policy_content = r#"
[metadata]
version = "1.0"
description = "Invalid policy"

[data_rules]
public = "local_only"
internal = "local_only"
confidential = "local_only"
restricted = "local_only"

[providers.openai]
region = "us"
allowed = false

[quotas]
max_tokens_per_request = 100
max_tokens_per_day = 1000
burst_requests_per_minute = 10

[redaction]
mode = "irreversible"
block_categories = []
        "#;

        fs::write(&policy_path, policy_content).unwrap();

        let loader = PolicyLoader::new(temp_dir.path()).unwrap();
        let result = loader.load_policy(&policy_path).await;

        assert!(result.is_err());
    }
}
