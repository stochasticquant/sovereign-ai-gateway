use gateway_core::types::ProviderId;
use serde::{Deserialize, Serialize};

/// The result of evaluating a request against the policy engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PolicyDecision {
    /// Request is allowed to proceed to the specified provider.
    Allow(ProviderId),

    /// Request is allowed but must be redacted before routing.
    AllowWithRedaction {
        provider: ProviderId,
        redaction_level: RedactionLevel,
    },

    /// Request is blocked. The reason is logged and returned to the caller.
    Block(String),

    /// Request should be degraded to a fallback provider.
    Degrade {
        fallback_provider: ProviderId,
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
