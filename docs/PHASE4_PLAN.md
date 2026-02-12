# Phase 4: Observability & Governance - Implementation Plan

**Status:** ✅ Complete
**Prerequisites:** ✅ Phase 3 Complete
**Tests:** 132/132 passing

## Overview

Phase 4 focuses on production-grade observability, audit logging, quota management, and streaming support. These features are critical for operating the gateway in regulated environments (healthcare, fintech, government).

## Objectives

1. **Audit Logging** - Comprehensive request/response logging for compliance
2. **Token Governance** - Usage tracking, quota enforcement, cost tracking
3. **Metrics & Tracing** - Prometheus metrics and OpenTelemetry integration
4. **Streaming Support** - SSE for real-time LLM responses
5. **Dashboard Ready** - Metrics and logs ready for Grafana/Kibana

## Phase 4 Components

### 1. Audit Log (`crates/audit-log`) 🎯 Priority 1

**Purpose:** Compliance-grade audit trail for all gateway operations.

**Features:**
- Structured audit events (request start/end, policy decisions, errors)
- PostgreSQL storage with retention policies
- JSON/CSV export for regulatory review
- PII-aware logging (sensitive data redaction)
- Async write (non-blocking)
- Queryable by tenant, time range, event type

**Key Types:**
```rust
pub enum AuditEvent {
    RequestReceived { request_id: UuidV7, tenant_id: String, endpoint: String },
    ProviderSelected { request_id: UuidV7, provider_id: String, reason: String },
    RequestSent { request_id: UuidV7, provider: String, model: String },
    ResponseReceived { request_id: UuidV7, tokens: Usage, cost: f64 },
    PolicyViolation { request_id: UuidV7, policy: String, action: String },
    PIIDetected { request_id: UuidV7, pii_types: Vec<String>, redacted: bool },
    Error { request_id: UuidV7, error_type: String, message: String },
}
```

**Database Schema:**
```sql
CREATE TABLE audit_events (
    id UUID PRIMARY KEY,
    request_id UUID NOT NULL,
    tenant_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(50) NOT NULL,
    event_data JSONB NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    INDEX idx_tenant_time (tenant_id, timestamp),
    INDEX idx_request (request_id)
);
```

**Files to Create:**
- `crates/audit-log/src/lib.rs`
- `crates/audit-log/src/events.rs` - Event types
- `crates/audit-log/src/logger.rs` - Async logger
- `crates/audit-log/src/storage.rs` - PostgreSQL storage
- `crates/audit-log/src/export.rs` - Export utilities
- `crates/audit-log/Cargo.toml`
- `migrations/005_audit_log.sql`

**Testing:**
- Unit tests for event serialization
- Integration tests with PostgreSQL
- Retention policy tests
- Export format tests

---

### 2. Token Governor (`crates/token-governor`) 🎯 Priority 2

**Purpose:** Track usage, enforce quotas, estimate costs per tenant.

**Features:**
- Token counting per request
- Per-tenant quotas (daily, monthly)
- Cost tracking with provider pricing
- Rate limiting per tenant
- Usage analytics and reporting
- Quota alerts (approaching limit)

**Key Types:**
```rust
pub struct TenantQuota {
    pub tenant_id: String,
    pub max_tokens_daily: u64,
    pub max_tokens_monthly: u64,
    pub max_cost_daily_usd: f64,
    pub max_cost_monthly_usd: f64,
}

pub struct UsageRecord {
    pub request_id: UuidV7,
    pub tenant_id: String,
    pub timestamp: DateTime<Utc>,
    pub tokens: Usage,
    pub cost_usd: f64,
    pub provider: String,
    pub model: String,
}
```

**Database Schema:**
```sql
CREATE TABLE tenant_quotas (
    tenant_id VARCHAR(255) PRIMARY KEY,
    max_tokens_daily BIGINT,
    max_tokens_monthly BIGINT,
    max_cost_daily_usd NUMERIC(10,6),
    max_cost_monthly_usd NUMERIC(10,6)
);

CREATE TABLE usage_records (
    id UUID PRIMARY KEY,
    request_id UUID NOT NULL,
    tenant_id VARCHAR(255) NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    prompt_tokens INT,
    completion_tokens INT,
    total_tokens INT,
    cost_usd NUMERIC(10,6),
    provider VARCHAR(100),
    model VARCHAR(100),
    INDEX idx_tenant_time (tenant_id, timestamp)
);
```

**Files to Create:**
- `crates/token-governor/src/lib.rs`
- `crates/token-governor/src/quota.rs` - Quota management
- `crates/token-governor/src/tracker.rs` - Usage tracking
- `crates/token-governor/src/limiter.rs` - Rate limiting
- `crates/token-governor/src/storage.rs` - PostgreSQL storage
- `crates/token-governor/Cargo.toml`
- `migrations/006_token_governor.sql`

---

### 3. Streaming Support (`provider-adapters`) 🎯 Priority 3

**Purpose:** Real-time streaming responses from LLM providers.

**Features:**
- SSE (Server-Sent Events) parsing
- Async stream handling
- Provider-specific streaming formats
- Partial response handling
- Stream error handling

**Trait Extension:**
```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    // Existing
    async fn send(&self, request: &LlmRequest) -> Result<LlmResponse, ProviderError>;

    // NEW: Streaming support
    async fn send_stream(
        &self,
        request: &LlmRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, ProviderError>> + Send>>, ProviderError>;

    // ...
}

pub struct StreamChunk {
    pub delta: String,
    pub finish_reason: Option<String>,
    pub model: String,
}
```

**Implementation:**
- OpenAI: SSE format with `data: {...}` chunks
- Anthropic: SSE format with message deltas
- Azure: Same as OpenAI
- Local: Provider-dependent (Ollama uses SSE)

**Files to Modify:**
- `crates/provider-adapters/src/traits.rs` - Add streaming trait method
- `crates/provider-adapters/src/openai.rs` - Implement streaming
- `crates/provider-adapters/src/anthropic.rs` - Implement streaming
- `crates/provider-adapters/src/azure.rs` - Implement streaming
- `crates/provider-adapters/src/local.rs` - Implement streaming
- `crates/provider-adapters/examples/streaming.rs` - New example

---

### 4. Prometheus Metrics (`gateway-server`) 🎯 Priority 4

**Purpose:** Production observability with Prometheus/Grafana.

**Metrics:**
```rust
// Request metrics
http_requests_total (counter) - by endpoint, method, status
http_request_duration_seconds (histogram) - by endpoint

// Provider metrics
provider_requests_total (counter) - by provider, model, status
provider_request_duration_seconds (histogram) - by provider
provider_cost_usd_total (counter) - by provider, model
provider_tokens_total (counter) - by provider, model, type (prompt/completion)

// Circuit breaker metrics
circuit_breaker_state (gauge) - by provider (0=closed, 1=open, 2=half-open)
circuit_breaker_state_changes_total (counter) - by provider

// Health check metrics
provider_health_check_success_total (counter) - by provider
provider_health_check_duration_seconds (histogram) - by provider

// Quota metrics
tenant_quota_usage_tokens (gauge) - by tenant, period
tenant_quota_usage_cost_usd (gauge) - by tenant, period
tenant_quota_exceeded_total (counter) - by tenant, quota_type
```

**Files to Modify:**
- `crates/gateway-server/src/metrics.rs` - Metric definitions
- `crates/gateway-server/src/middleware/metrics.rs` - Metrics middleware
- `deploy/docker/prometheus.yml` - Prometheus config

---

### 5. OpenTelemetry Tracing (`gateway-server`) 🎯 Priority 5

**Purpose:** Distributed tracing with Jaeger.

**Traces:**
- Gateway request span
  - Policy evaluation span
  - PII detection span
  - Provider selection span
  - Provider request span
    - Retry attempts (child spans)
    - Circuit breaker check
  - Response processing span
  - Audit logging span

**Files to Modify:**
- `crates/gateway-core/src/telemetry.rs` - Telemetry setup
- `crates/gateway-server/src/main.rs` - Initialize tracing
- `crates/gateway-server/src/handlers/completions.rs` - Add spans

---

## Implementation Order

### Session 1: Audit Log Foundation ⚡
1. Create `audit-log` crate structure
2. Define audit event types
3. Implement async logger
4. Add PostgreSQL storage
5. Write unit tests

### Session 2: Audit Log Integration ⚡
1. Add database migration
2. Integrate with gateway-server
3. Add export utilities
4. Integration tests
5. Documentation

### Session 3: Token Governor ⚡
1. Create `token-governor` crate
2. Define quota types and storage
3. Implement usage tracking
4. Add rate limiting
5. Unit tests

### Session 4: Token Governor Integration ⚡
1. Database migration
2. Integrate with gateway-server
3. Add quota middleware
4. Integration tests
5. Usage dashboard (optional)

### Session 5: Streaming Support ⚡
1. Update traits for streaming
2. Implement OpenAI streaming
3. Implement Anthropic streaming
4. Add streaming example
5. Integration tests

### Session 6: Metrics & Tracing ⚡
1. Add Prometheus metrics
2. Create metrics middleware
3. Add OpenTelemetry tracing
4. Update Prometheus config
5. Test with Grafana (optional)

---

## Testing Strategy

### Audit Log
- ✅ Unit tests for event serialization
- ✅ PostgreSQL integration tests
- ✅ Retention policy tests
- ✅ Export format tests

### Token Governor
- ✅ Quota enforcement tests
- ✅ Usage tracking accuracy
- ✅ Rate limiting tests
- ✅ Cost calculation tests

### Streaming
- ✅ SSE parsing tests
- ✅ Stream error handling
- ✅ Partial response tests
- ✅ Provider-specific format tests

### Metrics
- ✅ Metric accuracy tests
- ✅ Prometheus scraping tests
- ✅ Histogram bucket tests

---

## Dependencies

### New Crate Dependencies
```toml
# Audit log
sqlx = { workspace = true }
chrono = { workspace = true }

# Token governor
sqlx = { workspace = true }
tower-governor = "0.4" # Rate limiting

# Streaming
eventsource-stream = "0.2" # SSE parsing
tokio-stream = { workspace = true }

# Metrics
metrics = { workspace = true }
metrics-exporter-prometheus = { workspace = true }

# Tracing
tracing-opentelemetry = { workspace = true }
opentelemetry = { workspace = true }
opentelemetry_sdk = { workspace = true }
opentelemetry-otlp = { workspace = true }
```

---

## Success Criteria

### Phase 4 Complete When:
- ✅ Audit log captures all gateway events
- ✅ Quotas enforced per tenant (tokens + cost)
- ✅ All 4 providers support streaming
- ✅ Prometheus metrics exported
- ✅ OpenTelemetry traces working
- ✅ All tests passing (100+ tests)
- ✅ Documentation complete
- ✅ Dashboard examples provided

---

## Future Enhancements (Phase 5)

- Memory service with embeddings
- Semantic caching
- Multi-tenant dashboard UI
- Policy validation UI
- Advanced analytics
- Cost optimization recommendations

---

**Next Action:** Start with Session 1 - Audit Log Foundation

**Ready to proceed?** Let me know which component to start with, or if you'd like to begin with the Audit Log!
