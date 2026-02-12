//! Pure-function policy evaluation.
//!
//! Takes request metadata + sensitivity report → produces PolicyDecision.
//! Must be deterministic and side-effect free for testability.

use crate::decision::{
    EvaluationContext, PolicyDecision, PolicyViolation, RedactionLevel, RouteDecision, Severity,
    ViolationType,
};
use crate::schema::{Policy, RoutingConstraint};
use tracing::debug;

/// Policy evaluator for making routing decisions.
pub struct PolicyEvaluator {
    policy: Policy,
}

impl PolicyEvaluator {
    /// Create a new policy evaluator with the given policy.
    pub fn new(policy: Policy) -> Self {
        Self { policy }
    }

    /// Evaluate a request against the policy.
    pub fn evaluate(&self, context: &EvaluationContext) -> PolicyDecision {
        debug!(
            "Evaluating request {} with classification {:?}",
            context.metadata.request_id, context.classification
        );

        // Step 1: Check if any PII categories are blocked
        if let Some(blocked) = self.check_blocked_pii(&context.pii_categories) {
            return PolicyDecision::Block {
                reason: format!("Request contains blocked PII category: {}", blocked),
                violation: PolicyViolation {
                    violation_type: ViolationType::BlockedPiiCategory,
                    message: format!("PII category '{}' is blocked by policy", blocked),
                    severity: Severity::Critical,
                },
            };
        }

        // Step 2: Check quotas
        if let Some(violation) = self.check_quotas(context) {
            return PolicyDecision::Block {
                reason: "Quota exceeded".to_string(),
                violation,
            };
        }

        // Step 3: Get routing constraint for this data classification
        let constraint = self
            .policy
            .data_rules
            .get_constraint(context.classification);

        // Step 4: Check if classification blocks all requests
        if matches!(constraint, RoutingConstraint::Blocked) {
            return PolicyDecision::Block {
                reason: format!(
                    "Data classification {:?} is blocked by policy",
                    context.classification
                ),
                violation: PolicyViolation {
                    violation_type: ViolationType::DataClassificationRestriction,
                    message: format!(
                        "Classification level {:?} is not allowed",
                        context.classification
                    ),
                    severity: Severity::Critical,
                },
            };
        }

        // Step 5: Select provider based on routing constraint
        match self.select_provider(context, constraint) {
            Some(route) => {
                // Step 6: Determine if redaction is needed
                if !context.pii_categories.is_empty() {
                    PolicyDecision::AllowWithRedaction {
                        provider: route.provider,
                        model: route.model,
                        redaction_level: self.get_redaction_level(&context.pii_categories),
                        pii_categories: context.pii_categories.clone(),
                    }
                } else {
                    PolicyDecision::Allow {
                        provider: route.provider,
                        model: route.model,
                    }
                }
            }
            None => PolicyDecision::Block {
                reason: format!(
                    "No allowed provider for classification {:?}",
                    context.classification
                ),
                violation: PolicyViolation {
                    violation_type: ViolationType::NoAllowedProvider,
                    message: "No providers match policy constraints".to_string(),
                    severity: Severity::Error,
                },
            },
        }
    }

    /// Check if any PII categories are blocked.
    fn check_blocked_pii(&self, pii_categories: &[String]) -> Option<String> {
        for category in pii_categories {
            if self.policy.redaction.block_categories.contains(category) {
                return Some(category.clone());
            }
        }
        None
    }

    /// Check if quotas are exceeded.
    fn check_quotas(&self, context: &EvaluationContext) -> Option<PolicyViolation> {
        // Check daily quota
        if context.current_token_usage >= self.policy.quotas.max_tokens_per_day {
            return Some(PolicyViolation {
                violation_type: ViolationType::QuotaExceeded,
                message: format!(
                    "Daily token quota exceeded ({} >= {})",
                    context.current_token_usage, self.policy.quotas.max_tokens_per_day
                ),
                severity: Severity::Error,
            });
        }

        None
    }

    /// Select a provider based on routing constraints.
    fn select_provider(
        &self,
        context: &EvaluationContext,
        constraint: &RoutingConstraint,
    ) -> Option<RouteDecision> {
        let allowed_providers = self.policy.get_allowed_providers();

        for (name, config) in allowed_providers {
            // Check if provider matches routing constraint
            let provider_allowed = match constraint {
                RoutingConstraint::ExternalAllowed => true,
                RoutingConstraint::RegionalOnly => !config.region.starts_with("local"),
                RoutingConstraint::LocalOnly => config.region.starts_with("local"),
                RoutingConstraint::Blocked => false,
            };

            if !provider_allowed {
                debug!(
                    "Provider {} (region: {}) does not match constraint {:?}",
                    name, config.region, constraint
                );
                continue;
            }

            // Check if requested model is available (if specified)
            if let Some(ref requested_model) = context.requested_model {
                if !config.models.is_empty() && !config.models.contains(requested_model) {
                    debug!(
                        "Provider {} does not support model {}",
                        name, requested_model
                    );
                    continue;
                }
            }

            // Provider matches! Return the route decision
            let selected_model = context
                .requested_model
                .clone()
                .or_else(|| config.models.first().cloned());

            return Some(RouteDecision {
                provider: name.clone(),
                region: config.region.clone(),
                model: selected_model,
                priority: config.priority,
                requires_redaction: !context.pii_categories.is_empty(),
            });
        }

        None
    }

    /// Determine redaction level based on PII categories.
    fn get_redaction_level(&self, pii_categories: &[String]) -> RedactionLevel {
        // Check if any high-severity categories are present
        let high_severity = [
            "national_id",
            "medical_record",
            "credit_card",
            "bank_account",
        ];

        for category in pii_categories {
            if high_severity.contains(&category.as_str()) {
                return RedactionLevel::Full;
            }
        }

        RedactionLevel::High
    }

    /// Get the underlying policy.
    pub fn policy(&self) -> &Policy {
        &self.policy
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{
        DataRules, PolicyMetadata, ProviderConfig, Quotas, RedactionConfig, RedactionMode,
    };
    use chrono::Utc;
    use gateway_core::types::{DataClassification, RequestId, RequestMetadata, TenantId};
    use std::collections::HashMap;

    #[test]
    fn test_allow_public_data_external_provider() {
        let policy = create_test_policy();
        let evaluator = PolicyEvaluator::new(policy);

        let context = EvaluationContext {
            metadata: RequestMetadata {
                request_id: RequestId::new(),
                tenant_id: TenantId::new(),
                timestamp: Utc::now(),
                data_classification: Some(DataClassification::Public),
                source_ip: None,
            },
            classification: DataClassification::Public,
            pii_categories: vec![],
            requested_model: Some("gpt-4o".to_string()),
            current_token_usage: 0,
        };

        let decision = evaluator.evaluate(&context);

        match decision {
            PolicyDecision::Allow { provider, model } => {
                assert_eq!(provider, "openai");
                assert_eq!(model, Some("gpt-4o".to_string()));
            }
            _ => panic!("Expected Allow decision, got {:?}", decision),
        }
    }

    #[test]
    fn test_block_restricted_data_with_no_local_provider() {
        let mut policy = create_test_policy();
        // Remove local provider
        policy.providers.retain(|name, _| name != "local_llm");

        let evaluator = PolicyEvaluator::new(policy);

        let context = EvaluationContext {
            metadata: RequestMetadata {
                request_id: RequestId::new(),
                tenant_id: TenantId::new(),
                timestamp: Utc::now(),
                data_classification: Some(DataClassification::Restricted),
                source_ip: None,
            },
            classification: DataClassification::Restricted,
            pii_categories: vec!["email".to_string()],
            requested_model: None,
            current_token_usage: 0,
        };

        let decision = evaluator.evaluate(&context);

        match decision {
            PolicyDecision::Block { reason, violation } => {
                assert!(reason.contains("No allowed provider"));
                assert_eq!(violation.violation_type, ViolationType::NoAllowedProvider);
            }
            _ => panic!("Expected Block decision, got {:?}", decision),
        }
    }

    #[test]
    fn test_block_on_blocked_pii_category() {
        let policy = create_test_policy();
        let evaluator = PolicyEvaluator::new(policy);

        let context = EvaluationContext {
            metadata: RequestMetadata {
                request_id: RequestId::new(),
                tenant_id: TenantId::new(),
                timestamp: Utc::now(),
                data_classification: Some(DataClassification::Internal),
                source_ip: None,
            },
            classification: DataClassification::Internal,
            pii_categories: vec!["national_id".to_string()],
            requested_model: None,
            current_token_usage: 0,
        };

        let decision = evaluator.evaluate(&context);

        match decision {
            PolicyDecision::Block { reason, violation } => {
                assert!(reason.contains("blocked PII category"));
                assert_eq!(violation.violation_type, ViolationType::BlockedPiiCategory);
            }
            _ => panic!("Expected Block decision, got {:?}", decision),
        }
    }

    #[test]
    fn test_allow_with_redaction_for_pii() {
        let policy = create_test_policy();
        let evaluator = PolicyEvaluator::new(policy);

        let context = EvaluationContext {
            metadata: RequestMetadata {
                request_id: RequestId::new(),
                tenant_id: TenantId::new(),
                timestamp: Utc::now(),
                data_classification: Some(DataClassification::Public),
                source_ip: None,
            },
            classification: DataClassification::Public,
            pii_categories: vec!["email".to_string()],
            requested_model: None,
            current_token_usage: 0,
        };

        let decision = evaluator.evaluate(&context);

        match decision {
            PolicyDecision::AllowWithRedaction {
                provider,
                pii_categories,
                ..
            } => {
                assert!(!provider.is_empty());
                assert_eq!(pii_categories, vec!["email".to_string()]);
            }
            _ => panic!("Expected AllowWithRedaction decision, got {:?}", decision),
        }
    }

    #[test]
    fn test_block_on_quota_exceeded() {
        let policy = create_test_policy();
        let evaluator = PolicyEvaluator::new(policy);

        let context = EvaluationContext {
            metadata: RequestMetadata {
                request_id: RequestId::new(),
                tenant_id: TenantId::new(),
                timestamp: Utc::now(),
                data_classification: Some(DataClassification::Public),
                source_ip: None,
            },
            classification: DataClassification::Public,
            pii_categories: vec![],
            requested_model: None,
            current_token_usage: 2_000_000, // Exceeds quota
        };

        let decision = evaluator.evaluate(&context);

        match decision {
            PolicyDecision::Block { violation, .. } => {
                assert_eq!(violation.violation_type, ViolationType::QuotaExceeded);
            }
            _ => panic!("Expected Block decision for quota, got {:?}", decision),
        }
    }

    fn create_test_policy() -> Policy {
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
