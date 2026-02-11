use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a tenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TenantId(pub Uuid);

impl TenantId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for TenantId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TenantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a provider.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProviderId(pub String);

impl std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Data classification levels, ordered by sensitivity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataClassification {
    Public,
    Internal,
    Confidential,
    Restricted,
}

/// Geographic region for data sovereignty enforcement.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Region(pub String);

impl std::fmt::Display for Region {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A unique request identifier for tracing and correlation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RequestId(pub String);

impl RequestId {
    pub fn new() -> Self {
        Self(Uuid::now_v7().to_string())
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Metadata attached to every request flowing through the gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestMetadata {
    pub request_id: RequestId,
    pub tenant_id: TenantId,
    pub timestamp: DateTime<Utc>,
    pub data_classification: Option<DataClassification>,
    pub source_ip: Option<String>,
}
