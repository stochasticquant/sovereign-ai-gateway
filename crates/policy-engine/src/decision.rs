//! Policy evaluation decision types.
//!
//! Represents the outcome of policy evaluation and routing decisions.

use gateway_core::types::{DataClassification, RequestMetadata};
use serde::{Deserialize, Serialize};

/// The result of evaluating a request against the policy engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PolicyDecision {
    /// Request is allowed to proceed to the specified provider.
    Allow {
        provider: String,
        model: Option<String>,
    },

    /// Request is allowed but must be redacted before routing.
    AllowWithRedaction {
        provider: String,
        model: Option<String>,
        redaction_level: RedactionLevel,
        pii_categories: Vec<String>,
    },

    /// Request is blocked. The reason is logged and returned to the caller.
    Block { reason: String, violation: PolicyViolation },

    /// Request should be degraded to a fallback provider.
    Degrade {
        fallback_provider: String,
        reason: String,
    },
}

/// How aggressively PII should be redacted before forwarding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedactionLevel {
    /// Remove only high-severity PII (national IDs, medical records).
    High,
    /// Remove all detected PII.
    Full,
}

/// Details about a policy violation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyViolation {
    /// Type of violation.
    pub violation_type: ViolationType,
    /// Human-readable description.
    pub message: String,
    /// Severity level.
    pub severity: Severity,
}

/// Types of policy violations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationType {
    /// Data classification too high for any allowed provider.
    DataClassificationRestriction,
    /// Request contains blocked PII categories.
    BlockedPiiCategory,
    /// Quota exceeded.
    QuotaExceeded,
    /// No allowed providers match the request.
    NoAllowedProvider,
    /// Request model not allowed by policy.
    ModelNotAllowed,
    /// Region restriction violation.
    RegionRestriction,
}

/// Severity of a policy violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Complete evaluation context for a request.
#[derive(Debug, Clone)]
pub struct EvaluationContext {
    /// Request metadata.
    pub metadata: RequestMetadata,

    /// Detected data classification.
    pub classification: DataClassification,

    /// Detected PII categories.
    pub pii_categories: Vec<String>,

    /// Requested model (if specified).
    pub requested_model: Option<String>,

    /// Tenant's current token usage (for quota checks).
    pub current_token_usage: u64,
}

/// Route decision details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDecision {
    /// Selected provider name.
    pub provider: String,

    /// Provider region.
    pub region: String,

    /// Selected model.
    pub model: Option<String>,

    /// Provider priority that was selected.
    pub priority: u32,

    /// Whether redaction is required.
    pub requires_redaction: bool,
}
