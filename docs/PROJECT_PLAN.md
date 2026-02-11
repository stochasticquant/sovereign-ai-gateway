# Sovereign AI Gateway - Project Plan

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

### Phase 1: Core Gateway (Current)
- [x] Project structure and workspace
- [x] Basic HTTP server
- [x] Database migrations
- [x] Core types and error handling
- [ ] API key authentication
- [ ] Basic request routing

### Phase 2: Policy & Firewall
- [ ] Policy engine implementation
- [ ] Policy hot-reload
- [ ] PII detection engine
- [ ] Data classification
- [ ] Redaction capabilities

### Phase 3: Provider Integration
- [ ] OpenAI adapter
- [ ] Anthropic adapter
- [ ] Azure OpenAI adapter
- [ ] Streaming support
- [ ] Error handling and retries

### Phase 4: Observability & Governance
- [ ] Audit logging implementation
- [ ] Token counting and quotas
- [ ] Usage tracking
- [ ] Prometheus metrics
- [ ] OpenTelemetry tracing

### Phase 5: Advanced Features
- [ ] Memory service with embeddings
- [ ] Semantic caching
- [ ] Multi-tenant dashboard
- [ ] Policy validation UI
- [ ] Advanced analytics

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
