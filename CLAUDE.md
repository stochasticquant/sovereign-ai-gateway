# Sovereign AI Gateway — Project Instructions

## Overview
Enterprise-grade AI control plane built in Rust. Enforces data sovereignty, policy-based routing, PII filtering, and audit telemetry for regulated industries (healthcare, fintech, government) across Africa.

## Architecture
- **Cargo workspace** with 9 crates under `crates/`
- **gateway-core**: shared types, config, error handling, crypto primitives
- **gateway-server**: Axum HTTP server, middleware, handlers (binary crate)
- **context-firewall**: PII detection, data classification, redaction engine
- **policy-engine**: TOML policy evaluation, hot-reload, tenant routing
- **provider-adapters**: LLM provider abstraction (OpenAI, Anthropic, Azure, local)
- **memory-service**: local semantic memory with ONNX embeddings + Qdrant
- **token-governor**: usage tracking, quota enforcement, cost estimation
- **audit-log**: structured audit logging, retention, regulatory export
- **gateway-tests**: integration and E2E test harness

## Tech Stack
- Rust (edition 2024, MSRV 1.85), Tokio, Axum, Serde
- PostgreSQL (sqlx), Qdrant (vector DB), ONNX Runtime (ort)
- Tracing + OpenTelemetry, Prometheus metrics
- Docker, Kubernetes, Helm

## Conventions
- All errors use `GatewayError` from gateway-core
- All IDs use UUIDv7 (time-sortable)
- Config: layered TOML (default → env-specific → env vars)
- Policies: TOML files in `policies/` with strict schema validation
- Tests: `cargo nextest run`. Property tests with `proptest` for firewall/policy.
- No `unsafe` without explicit justification
- No panics in the request path — all errors handled via Result
- Clippy must pass with `-D warnings`

## Commands
- Build: `cargo build`
- Test: `cargo nextest run --workspace`
- Lint: `cargo clippy --workspace --all-targets -- -D warnings`
- Format: `cargo fmt --all`
- Audit: `cargo deny check`
- Dev watch: `cargo watch -x 'clippy --workspace' -x 'nextest run'`
- Dev infra: `docker compose -f deploy/docker/docker-compose.yml up -d`
- DB migrate: `sqlx migrate run --source migrations`

## File Layout
```
crates/           — Rust workspace crates
config/           — Gateway config files (TOML)
policies/         — Routing policy files (TOML)
migrations/       — PostgreSQL migrations (sqlx)
deploy/docker/    — Dockerfile, docker-compose, prometheus config
deploy/k8s/helm/  — Helm chart for Kubernetes
.github/workflows — CI/CD pipelines
docs/             — Architecture docs and ADRs
```
