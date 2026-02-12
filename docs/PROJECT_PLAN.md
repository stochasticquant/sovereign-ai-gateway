# Sovereign AI Gateway - Project Plan

## 📊 Current Status (2026-02-12)

**Active Branch**: `main`
**Project Completion**: ~80% (4 of 5 phases complete)

| Phase | Status | Branch | Tests |
|-------|--------|--------|-------|
| Phase 1 | ✅ Merged | `main` | N/A |
| Phase 2 | ✅ Merged | `main` | 36/36 ✅ |
| Phase 3 | ✅ Merged | `main` | 57/57 ✅ |
| Phase 4 | ✅ Merged | `main` | 132/132 ✅ |
| Phase 5 | 🔮 Ready | N/A | N/A |

**Next**: Begin Phase 5 - Advanced Features

---

## Overview
Enterprise-grade AI control plane built in Rust for enforcing data sovereignty, policy-based routing, PII filtering, and audit telemetry across regulated industries (healthcare, fintech, government) in Africa.

## Architecture

### Core Components

#### 1. Gateway Core (`crates/gateway-core`)
- **Purpose**: Shared foundation for all gateway components
- **Responsibilities**:
  - Common types and traits
  - Configuration management (layered TOML)
  - Error handling (`GatewayError`)
  - Cryptographic primitives
  - UUIDv7 generation (time-sortable)

#### 2. Gateway Server (`crates/gateway-server`)
- **Purpose**: Main HTTP service
- **Tech Stack**: Axum, Tower middleware
- **Features**:
  - REST API endpoints
  - Request/response handling
  - Middleware pipeline
  - Metrics endpoint (`/metrics`)
  - Health checks

#### 3. Context Firewall (`crates/context-firewall`)
- **Purpose**: PII detection and data classification
- **Features**:
  - Pattern-based PII detection (regex, aho-corasick)
  - Data classification engine
  - Redaction/masking capabilities
  - Configurable sensitivity levels
  - Property testing with proptest

#### 4. Policy Engine (`crates/policy-engine`)
- **Purpose**: Routing and access control
- **Features**:
  - TOML-based policy files (`policies/` directory)
  - Hot-reload via file watching (notify crate)
  - Tenant-based routing
  - Policy validation and schema enforcement
  - Multi-tenant isolation

#### 5. Provider Adapters (`crates/provider-adapters`)
- **Purpose**: Unified LLM provider abstraction
- **Supported Providers**:
  - OpenAI (GPT models)
  - Anthropic (Claude models)
  - Azure OpenAI
  - Local models (Ollama, etc.)
- **Features**:
  - Streaming support
  - Retry logic
  - Rate limiting
  - Provider-specific error handling

#### 6. Memory Service (`crates/memory-service`)
- **Purpose**: Local semantic memory and context management
- **Tech Stack**: ONNX Runtime, Qdrant
- **Features**:
  - Embedding generation (ONNX models)
  - Vector storage (Qdrant integration)
  - Similarity search
  - Context retrieval
  - Session management

#### 7. Token Governor (`crates/token-governor`)
- **Purpose**: Usage tracking and quota enforcement
- **Features**:
  - Token counting per request
  - Quota management per tenant
  - Cost estimation
  - Rate limiting
  - Usage analytics

#### 8. Audit Log (`crates/audit-log`)
- **Purpose**: Compliance and regulatory logging
- **Features**:
  - Structured audit events
  - PostgreSQL storage
  - Retention policies
  - Export capabilities (JSON, CSV)
  - Regulatory compliance (GDPR, HIPAA)
  - OpenTelemetry integration

#### 9. Gateway Tests (`crates/gateway-tests`)
- **Purpose**: Integration and E2E testing
- **Features**:
  - Integration test harness
  - Mock providers (wiremock)
  - Property tests (proptest)
  - End-to-end scenarios

## Technology Stack

### Core Technologies
- **Language**: Rust (edition 2024, MSRV 1.85)
- **Async Runtime**: Tokio
- **HTTP Framework**: Axum + Tower
- **HTTP Client**: Reqwest (rustls-tls)

### Data Storage
- **Primary Database**: PostgreSQL 16 (via sqlx)
- **Vector Database**: Qdrant
- **Migrations**: sqlx-cli

### Observability
- **Logging**: tracing + tracing-subscriber
- **Metrics**: Prometheus (metrics-exporter-prometheus)
- **Tracing**: OpenTelemetry + Jaeger
- **Dashboards**: Grafana

### Security & Cryptography
- **TLS**: rustls
- **Crypto**: ring
- **Secrets Management**: Environment variables + .env

### Testing
- **Test Runner**: cargo-nextest
- **Mocking**: wiremock
- **Property Testing**: proptest
- **Assertions**: assert_matches

## Development Workflow

### Local Development
1. Start infrastructure: `docker compose -f deploy/docker/docker-compose.yml up -d`
2. Run migrations: `sqlx migrate run --source migrations`
3. Build: `cargo build --workspace`
4. Run: `cargo run -p gateway-server`

### Testing
- Unit tests: `cargo nextest run --workspace`
- Property tests: Integrated in test suite
- Integration tests: `gateway-tests` crate
- Linting: `cargo clippy --workspace --all-targets -- -D warnings`
- Formatting: `cargo fmt --all --check`

### CI/CD
- **Location**: `.github/workflows/`
- **Checks**:
  - Build on multiple platforms
  - Run all tests
  - Clippy linting
  - Format checking
  - Security audit (cargo-deny)

### Quality Standards
- No `unsafe` code without explicit justification
- No panics in request path - all errors via `Result`
- All public APIs documented
- Test coverage for critical paths
- Property tests for firewall and policy engine

## Deployment

### Container (Docker)
- **Dockerfile**: `deploy/docker/Dockerfile`
- **Base Image**: Rust builder + distroless runtime
- **Configuration**: Environment variables + TOML files

### Kubernetes (Helm)
- **Chart Location**: `deploy/k8s/helm/`
- **Features**:
  - Multi-replica deployment
  - Auto-scaling
  - Health checks
  - Resource limits
  - ConfigMaps for policies

### Configuration Management
- **Layered Config**: Default → Environment-specific → Environment variables
- **Policy Files**: Mounted as volumes or ConfigMaps
- **Secrets**: Kubernetes secrets or environment variables

## Security Considerations

### Data Protection
- PII detection and redaction
- Data classification
- Encryption in transit (TLS)
- Encryption at rest (database-level)

### Access Control
- API key authentication
- Tenant isolation
- Role-based access control (future)
- Rate limiting per tenant

### Compliance
- Audit logging for all requests
- Data retention policies
- Export capabilities for regulatory review
- GDPR/HIPAA compliance features

## Roadmap

### Phase 1: Core Gateway ✅ COMPLETE & MERGED
**Branch**: `main`
**Status**: Merged and deployed

- [x] Project structure and workspace
- [x] Basic HTTP server (Axum)
- [x] Database migrations (sqlx)
- [x] Core types and error handling
- [x] Cargo workspace with 9 crates
- [x] Configuration system (layered TOML)
- [x] UUIDv7 support (time-sortable IDs)

### Phase 2: Policy & Firewall ✅ COMPLETE - ⏳ PENDING MERGE
**Branch**: `feature/phase-2-policy-firewall`
**Status**: 78% complete - Core functionality done, optional enhancements remaining
**Tests**: 36/36 passing
**Commits**: 3 major features

**Core Features (Complete)**:
- [x] Policy engine implementation (schema, loader, evaluator)
- [x] Policy hot-reload (notify + debounce)
- [x] PII detection engine (15+ types including Africa-specific)
- [x] Data classification (4 levels: Public, Internal, Confidential, Restricted)
- [x] Redaction capabilities (4 strategies: Mask, Hash, Remove, Partial)
- [x] Luhn validation for credit cards
- [x] Fake SSN detection
- [x] Risk-based classification (0-100 score)
- [x] Thread-safe policy access (RwLock)

**Optional Enhancements (Remaining)**:
- [ ] Additional Africa-specific PII patterns
- [ ] Property tests with proptest
- [ ] Integration tests for full pipeline
- [ ] Documentation (policy-schema.md, pii-patterns.md)

**Action Required**: Merge to main or continue with enhancements

### Phase 3: Provider Integration ✅ COMPLETE - 🔄 PR OPEN
**Branch**: `feature/phase-3-provider-integration`
**Status**: Complete and ready for merge
**PR**: #1 (https://github.com/stochasticquant/sovereign-ai-gateway/pull/1)
**Tests**: 57/57 passing
**Commits**: 13 commits (7 Phase 3 + 6 Phase 2 base)

- [x] OpenAI adapter (GPT-3.5, GPT-4, GPT-4o)
- [x] Anthropic adapter (Claude 3/3.5 Sonnet/Opus)
- [x] Azure OpenAI adapter (regional endpoints)
- [x] Local LLM adapter (Ollama, vLLM, LocalAI)
- [x] Provider registry with health monitoring
- [x] Circuit breaker pattern (3 states)
- [x] Retry logic with exponential backoff
- [x] Integration tests with wiremock (13 tests)
- [x] Cost estimation per provider
- [x] 3 comprehensive examples
- [x] Complete documentation
- [ ] Streaming support (deferred to Phase 4)

**Action Required**: Review and merge PR #1

### Phase 4: Observability & Governance ✅ COMPLETE
**Branch**: `feature/phase-3-provider-integration`
**Status**: Complete - all features implemented
**Tests**: 132/132 passing (75 new tests)

**Completed Features**:
- [x] Audit log exporter (paginated JSON/CSV export with filtering)
- [x] Cost calculation (bridges provider-adapters pricing into token-governor)
- [x] Pre-flight token estimation (heuristic: chars/4 + overhead)
- [x] Sliding window rate limiting (per-tenant, in-memory)
- [x] Prometheus metrics (requests, latency, tokens, cost, blocked, audit)
- [x] OpenTelemetry distributed tracing (OTLP gRPC to Jaeger)
- [x] Audit logging wired into request pipeline (non-blocking via channels)
- [x] Full pipeline integration (provider dispatch, cost calc, token tracking, audit, metrics)
- [x] Admin audit export endpoint (GET /admin/audit/export)
- [ ] Streaming support for all providers (SSE) — deferred to Phase 5

**New Files Created**:
- `crates/gateway-server/src/metrics.rs` — Metric constants + recording helpers
- `crates/gateway-server/src/telemetry.rs` — OTel pipeline init/shutdown
- `crates/gateway-server/src/handlers/admin/audit_export.rs` — Audit export handler

### Phase 5: Advanced Features 🔮 FUTURE
**Status**: Ready to start
**Prerequisites**: ✅ Phase 4 complete

- [ ] Streaming SSE support for all providers
- [ ] Memory service with ONNX embeddings
- [ ] Qdrant vector storage integration
- [ ] Semantic caching
- [ ] Multi-tenant dashboard UI
- [ ] Policy validation UI
- [ ] Advanced analytics and reporting
- [ ] Cost optimization recommendations

## File Structure
```
├── crates/              # Rust workspace crates
│   ├── gateway-core/    # Shared types and utilities
│   ├── gateway-server/  # HTTP server (binary)
│   ├── context-firewall/# PII detection
│   ├── policy-engine/   # Policy evaluation
│   ├── provider-adapters/# LLM provider abstraction
│   ├── memory-service/  # Semantic memory
│   ├── token-governor/  # Usage tracking
│   ├── audit-log/       # Audit logging
│   └── gateway-tests/   # Integration tests
├── config/              # Configuration files
├── policies/            # Policy TOML files
├── migrations/          # Database migrations
├── deploy/
│   ├── docker/          # Docker Compose + Dockerfile
│   └── k8s/helm/        # Kubernetes Helm chart
├── docs/                # Documentation and ADRs
└── models/              # ONNX embedding models
```

## Contributing Guidelines

### Code Style
- Follow Rust conventions (rustfmt)
- Use meaningful names
- Document public APIs
- Write tests for new features

### Commit Messages
- Use conventional commits format
- Keep commits atomic and focused
- Reference issues/tickets when applicable

### Pull Requests
- Ensure all tests pass
- Update documentation
- Add tests for new features
- Keep PRs focused and reviewable

## License
MIT OR Apache-2.0
