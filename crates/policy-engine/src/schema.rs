//! Policy TOML schema definition.
//!
//! Defines the structure of policy files with strict validation on load.
//! Supports versioned schemas for forward-compatible evolution.

use gateway_core::types::{DataClassification, TenantId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Complete policy document loaded from TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    /// Metadata about this policy.
    pub metadata: PolicyMetadata,

    /// Data classification routing rules.
    pub data_rules: DataRules,

    /// Provider configurations.
    pub providers: HashMap<String, ProviderConfig>,

    /// Usage quotas and rate limits.
    pub quotas: Quotas,

    /// Redaction settings.
    pub redaction: RedactionConfig,

    /// Optional tenant ID this policy applies to (if tenant-specific).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<TenantId>,
}

/// Policy metadata for versioning and documentation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyMetadata {
    /// Schema version (e.g., "1.0").
    pub version: String,

    /// Human-readable description of this policy.
    pub description: String,
}

/// Data classification to routing constraint mapping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataRules {
    /// Routing for PUBLIC data.
    pub public: RoutingConstraint,

    /// Routing for INTERNAL data.
    pub internal: RoutingConstraint,

    /// Routing for CONFIDENTIAL data.
    pub confidential: RoutingConstraint,

    /// Routing for RESTRICTED data (PII, PHI, etc.).
    pub restricted: RoutingConstraint,
}

impl DataRules {
    /// Get routing constraint for a given data classification.
    pub fn get_constraint(&self, classification: DataClassification) -> &RoutingConstraint {
        match classification {
            DataClassification::Public => &self.public,
            DataClassification::Internal => &self.internal,
            DataClassification::Confidential => &self.confidential,
            DataClassification::Restricted => &self.restricted,
        }
    }
}

/// Routing constraint based on data sensitivity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingConstraint {
    /// Can route to any provider (external or local).
    ExternalAllowed,

    /// Must stay within the same geographic region.
    RegionalOnly,

    /// Must use local/on-premises providers only.
    LocalOnly,

    /// Blocked - do not process this request.
    Blocked,
}

/// Provider configuration and capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Geographic region this provider operates in.
    pub region: String,

    /// Whether this provider is allowed for use.
    pub allowed: bool,

    /// Priority for provider selection (lower = higher priority).
    #[serde(default = "default_priority")]
    pub priority: u32,

    /// List of models available from this provider.
    #[serde(default)]
    pub models: Vec<String>,

    /// Optional custom endpoint URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,

    /// Optional authentication method override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<String>,
}

fn default_priority() -> u32 {
    100
}

/// Usage quotas and rate limiting configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quotas {
    /// Maximum tokens per single request.
    pub max_tokens_per_request: u32,

    /// Maximum tokens per day per tenant.
    pub max_tokens_per_day: u64,

    /// Burst rate limit (requests per minute).
    pub burst_requests_per_minute: u32,

    /// Optional sustained rate limit (requests per hour).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sustained_requests_per_hour: Option<u32>,
}

/// Redaction and PII handling configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionConfig {
    /// Redaction mode: reversible (hashed) or irreversible (masked).
    pub mode: RedactionMode,

    /// PII categories that trigger request blocking (never just redaction).
    pub block_categories: Vec<String>,

    /// Optional custom redaction patterns.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub custom_patterns: HashMap<String, String>,
}

/// Redaction mode for PII handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedactionMode {
    /// Redaction is irreversible (masked/removed).
    Irreversible,

    /// Redaction is reversible (hashed with tenant key).
    Reversible,

    /// No redaction (audit only).
    AuditOnly,
}

/// Result of policy validation.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether the policy is valid.
    pub valid: bool,

    /// List of validation errors (if any).
    pub errors: Vec<String>,

    /// List of validation warnings (non-fatal).
    pub warnings: Vec<String>,
}

impl ValidationResult {
    /// Create a successful validation result.
    pub fn success() -> Self {
        Self {
            valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Add an error to the validation result.
    pub fn add_error(&mut self, error: String) {
        self.valid = false;
        self.errors.push(error);
    }

    /// Add a warning to the validation result.
    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }
}

impl Policy {
    /// Validate this policy for correctness.
    pub fn validate(&self) -> ValidationResult {
        let mut result = ValidationResult::success();

        // Validate version
        if self.metadata.version.is_empty() {
            result.add_error("Policy version cannot be empty".to_string());
        }

        // Validate at least one provider is allowed
        let allowed_providers: Vec<_> = self
            .providers
            .iter()
            .filter(|(_, config)| config.allowed)
            .collect();

        if allowed_providers.is_empty() {
            result.add_error("at least one provider must be allowed".to_string());
        }

        // Validate quotas are reasonable
        if self.quotas.max_tokens_per_request == 0 {
            result.add_error("max_tokens_per_request must be > 0".to_string());
        }

        if self.quotas.max_tokens_per_request > 1_000_000 {
            result.add_warning(format!(
                "max_tokens_per_request ({}) is very high",
                self.quotas.max_tokens_per_request
            ));
        }

        if self.quotas.max_tokens_per_day == 0 {
            result.add_error("max_tokens_per_day must be > 0".to_string());
        }

        if self.quotas.burst_requests_per_minute == 0 {
            result.add_error("burst_requests_per_minute must be > 0".to_string());
        }

        // Validate provider regions
        for (name, config) in &self.providers {
            if config.region.is_empty() {
                result.add_error(format!("Provider '{}' has empty region", name));
            }

            if config.allowed && config.models.is_empty() {
                result.add_warning(format!(
                    "Provider '{}' is allowed but has no models configured",
                    name
                ));
            }
        }

        // Validate redaction block categories
        if self.redaction.mode == RedactionMode::AuditOnly
            && !self.redaction.block_categories.is_empty()
        {
            result.add_warning(
                "Redaction mode is 'audit_only' but block_categories are specified".to_string(),
            );
        }

        result
    }

    /// Get provider configuration by name.
    pub fn get_provider(&self, name: &str) -> Option<&ProviderConfig> {
        self.providers.get(name)
    }

    /// Get all allowed providers, sorted by priority.
    pub fn get_allowed_providers(&self) -> Vec<(&String, &ProviderConfig)> {
        let mut providers: Vec<_> = self
            .providers
            .iter()
            .filter(|(_, config)| config.allowed)
            .collect();

        providers.sort_by_key(|(_, config)| config.priority);
        providers
    }

    /// Check if a provider is allowed for a given data classification.
    pub fn is_provider_allowed(
        &self,
        provider_name: &str,
        classification: DataClassification,
    ) -> bool {
        let Some(provider) = self.providers.get(provider_name) else {
            return false;
        };

        if !provider.allowed {
            return false;
        }

        let constraint = self.data_rules.get_constraint(classification);

        match constraint {
            RoutingConstraint::Blocked => false,
            RoutingConstraint::ExternalAllowed => true,
            RoutingConstraint::RegionalOnly => {
                // Would need region matching logic here
                // For now, allow if provider is regional
                !provider.region.starts_with("local")
            }
            RoutingConstraint::LocalOnly => provider.region.starts_with("local"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_validation_success() {
        let policy = create_valid_policy();
        let result = policy.validate();
        assert!(result.valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_policy_validation_no_providers() {
        let mut policy = create_valid_policy();
        for config in policy.providers.values_mut() {
            config.allowed = false;
        }

        let result = policy.validate();
        assert!(!result.valid);
        assert!(result
            .errors
            .iter()
            .any(|e| e.contains("at least one provider")));
    }

    #[test]
    fn test_policy_validation_zero_quotas() {
        let mut policy = create_valid_policy();
        policy.quotas.max_tokens_per_request = 0;

        let result = policy.validate();
        assert!(!result.valid);
        assert!(result
            .errors
            .iter()
            .any(|e| e.contains("max_tokens_per_request")));
    }

    #[test]
    fn test_get_allowed_providers_sorted_by_priority() {
        let policy = create_valid_policy();
        let providers = policy.get_allowed_providers();

        assert!(!providers.is_empty());
        // Verify they're sorted by priority
        for i in 1..providers.len() {
            assert!(providers[i - 1].1.priority <= providers[i].1.priority);
        }
    }

    #[test]
    fn test_routing_constraint_for_classification() {
        let policy = create_valid_policy();

        assert_eq!(
            policy.data_rules.get_constraint(DataClassification::Public),
            &RoutingConstraint::ExternalAllowed
        );
        assert_eq!(
            policy
                .data_rules
                .get_constraint(DataClassification::Restricted),
            &RoutingConstraint::LocalOnly
        );
    }

    fn create_valid_policy() -> Policy {
        let mut providers = HashMap::new();
        providers.insert(
            "local_llm".to_string(),
            ProviderConfig {
                region: "local".to_string(),
                allowed: true,
                priority: 1,
                models: vec!["llama-3".to_string()],
                endpoint: None,
                auth: None,
            },
        );
        providers.insert(
            "openai".to_string(),
            ProviderConfig {
                region: "us-east-1".to_string(),
                allowed: true,
                priority: 2,
                models: vec!["gpt-4o".to_string()],
                endpoint: None,
                auth: None,
            },
        );

        Policy {
            metadata: PolicyMetadata {
                version: "1.0".to_string(),
                description: "Test policy".to_string(),
            },
            data_rules: DataRules {
                public: RoutingConstraint::ExternalAllowed,
                internal: RoutingConstraint::RegionalOnly,
                confidential: RoutingConstraint::LocalOnly,
                restricted: RoutingConstraint::LocalOnly,
            },
            providers,
            quotas: Quotas {
                max_tokens_per_request: 4096,
                max_tokens_per_day: 1_000_000,
                burst_requests_per_minute: 60,
                sustained_requests_per_hour: None,
            },
            redaction: RedactionConfig {
                mode: RedactionMode::Irreversible,
                block_categories: vec!["national_id".to_string()],
                custom_patterns: HashMap::new(),
            },
            tenant_id: None,
        }
    }
}
