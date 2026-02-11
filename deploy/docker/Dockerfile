# ── Stage 1: Build ────────────────────────────────────────────
FROM rust:1.93-bookworm AS builder

WORKDIR /app

# Cache dependency compilation by building with empty sources first
COPY Cargo.toml Cargo.lock ./
COPY crates/gateway-core/Cargo.toml crates/gateway-core/Cargo.toml
COPY crates/gateway-server/Cargo.toml crates/gateway-server/Cargo.toml
COPY crates/context-firewall/Cargo.toml crates/context-firewall/Cargo.toml
COPY crates/policy-engine/Cargo.toml crates/policy-engine/Cargo.toml
COPY crates/provider-adapters/Cargo.toml crates/provider-adapters/Cargo.toml
COPY crates/memory-service/Cargo.toml crates/memory-service/Cargo.toml
COPY crates/token-governor/Cargo.toml crates/token-governor/Cargo.toml
COPY crates/audit-log/Cargo.toml crates/audit-log/Cargo.toml
COPY crates/gateway-tests/Cargo.toml crates/gateway-tests/Cargo.toml

# Create dummy source files to compile dependencies
RUN for dir in gateway-core context-firewall policy-engine provider-adapters memory-service token-governor audit-log gateway-tests; do \
      mkdir -p crates/$dir/src && echo "" > crates/$dir/src/lib.rs; \
    done && \
    mkdir -p crates/gateway-server/src && echo "fn main() {}" > crates/gateway-server/src/main.rs

RUN cargo build --release --bin sovereign-gateway 2>/dev/null || true

# Copy actual source and rebuild
COPY crates/ crates/
RUN touch crates/*/src/*.rs && cargo build --release --bin sovereign-gateway

# ── Stage 2: Runtime ──────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd -r gateway && useradd -r -g gateway -s /usr/sbin/nologin gateway

COPY --from=builder /app/target/release/sovereign-gateway /usr/local/bin/sovereign-gateway
COPY config/ /etc/sovereign-gateway/config/
COPY policies/ /etc/sovereign-gateway/policies/

USER gateway
EXPOSE 8080

HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=3 \
    CMD ["/usr/local/bin/sovereign-gateway", "--health-check"]

ENTRYPOINT ["/usr/local/bin/sovereign-gateway"]
