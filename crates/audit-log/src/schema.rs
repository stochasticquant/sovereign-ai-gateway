use chrono::{DateTime, Utc};
use gateway_core::types::{DataClassification, ProviderId, RequestId, TenantId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A single audit log entry capturing everything about a gateway request.
/// This is the primary record for regulatory compliance and forensics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: Uuid,
    pub request_id: RequestId,
    pub timestamp: DateTime<Utc>,
    pub tenant_id: TenantId,
    pub data_classification: DataClassification,
    pub provider_id: ProviderId,
    pub model: String,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub region: String,
    pub risk_score: u8,
    pub decision: String,
    pub latency_ms: u64,
    pub status_code: u16,
    pub error: Option<String>,
}
