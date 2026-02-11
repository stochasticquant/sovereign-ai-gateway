use gateway_core::types::TenantId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A chunk of memory stored in the vector database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryChunk {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub encrypted_text: Vec<u8>,
    pub embedding: Vec<f32>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub ttl_seconds: Option<u64>,
}

// TODO(phase-5): Vector storage trait abstraction.
// Implementations: Qdrant adapter, in-memory HNSW for single-node.
// Each tenant gets a separate collection/namespace.
