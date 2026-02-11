# Environment Setup Status

**Last Updated**: 2026-02-11
**Platform**: Linux (Ubuntu/Debian-based)
**Status**: ✅ Fully Operational

## System Requirements

### Software Versions
- **Rust**: 1.93.0 (stable) - ✅ Installed
  - MSRV: 1.85+ (requirement met)
  - Toolchain: stable-x86_64-unknown-linux-gnu
  - Components: clippy, rustfmt, rust-src, rust-docs
- **Docker**: 29.2.1 - ✅ Installed
- **Docker Compose**: v5.0.2 - ✅ Installed

### Development Tools
- **cargo-nextest**: 0.9.126 - ✅ Installed
- **sqlx-cli**: 0.8.6 - ✅ Installed (with PostgreSQL support)
- **cargo-watch**: Not installed (optional)

## Infrastructure Services

### Running Services (Docker Compose)
All services started via: `docker compose -f deploy/docker/docker-compose.yml up -d`

| Service | Status | Ports | Health | Purpose |
|---------|--------|-------|--------|---------|
| PostgreSQL 16 | ✅ Running | 5432 | Healthy | Primary database |
| Qdrant | ✅ Running | 6333-6334 | Starting | Vector database |
| Jaeger | ✅ Running | 4317-4318, 16686 | Running | Distributed tracing |
| Grafana | ✅ Running | 3000 | Running | Monitoring dashboard |
| Prometheus | ❌ Not Running | 9090 | Failed | Metrics collection |

**Note on Prometheus**: Failed to start due to OneDrive bind mount limitations on Linux. This is a known issue with OneDrive's FUSE implementation not properly supporting Docker bind mounts. Prometheus is optional for local development.

### Database Status
- **Migrations Applied**: ✅ All 4 migrations successful
  1. `001_create_tenants.sql`
  2. `002_create_api_keys.sql`
  3. `003_create_audit_log.sql`
  4. `004_create_usage_tracking.sql`
- **Connection String**: `postgres://gateway:gateway_dev_password@localhost:5432/sovereign_gateway`

## Environment Configuration

### Environment Variables
Configuration file: `.env` (created from `.env.example`)

```bash
# Gateway configuration
GATEWAY_SERVER__HOST=127.0.0.1
GATEWAY_SERVER__PORT=8080

# Database
DATABASE_URL=postgres://gateway:gateway_dev_password@localhost:5432/sovereign_gateway

# Telemetry
RUST_LOG=sovereign_gateway=debug,tower_http=debug
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317

# Provider API keys (add your own)
# OPENAI_API_KEY=sk-...
# ANTHROPIC_API_KEY=sk-ant-...
```

### Cargo Configuration
Special configuration required for OneDrive on Linux:

**File**: `.cargo/config.toml`
```toml
[build]
# Use a target directory outside OneDrive to avoid permission issues on Linux
target-dir = "/tmp/sovereign-gateway-target"
```

**Reason**: OneDrive on Linux uses FUSE, which doesn't properly support executable permissions required by Cargo build scripts. Moving the target directory outside OneDrive resolves this issue.

## Build Status

### Workspace Compilation
- **Status**: ✅ Success
- **Build Time**: ~3 minutes (first build)
- **Profile**: dev (unoptimized + debuginfo)
- **Crates Compiled**: All 9 workspace crates + dependencies

### Code Quality Checks
- **Clippy**: ✅ Passed (no warnings)
- **Format**: Not checked yet
- **Audit**: Not run yet

### Test Status
- **Test Runner**: cargo-nextest
- **Tests Run**: 3/3 passed
- **Duration**: ~0.006s
- **Results**:
  - ✅ `gateway-tests::smoke_test::workspace_compiles`
  - ✅ `gateway-core::crypto::tests::test_sha256_deterministic`
  - ✅ `gateway-core::crypto::tests::test_sha256_different_inputs`

## Known Issues & Workarounds

### 1. OneDrive Executable Permissions
**Issue**: OneDrive on Linux doesn't support executable permissions for Cargo build scripts.

**Workaround**: Use `.cargo/config.toml` to redirect build artifacts to `/tmp/sovereign-gateway-target`.

**Impact**: Build artifacts are not persisted after system reboot (stored in `/tmp`).

**Alternative**: Move the entire project outside of OneDrive to a native Linux filesystem (e.g., `~/projects/`).

### 2. Prometheus Bind Mount Failure
**Issue**: Docker cannot create bind mounts for files inside OneDrive directory structure.

**Workaround**: Prometheus disabled for local development. Core services (PostgreSQL, Qdrant, Jaeger) are sufficient for development.

**Impact**: No Prometheus metrics collection during local development. Grafana still works for visualization if metrics are pushed via other means.

**Alternative**:
- Copy `prometheus.yml` to `/tmp` and update docker-compose.yml
- Use Prometheus in remote/staging environment only

### 3. Windows PowerShell Scripts
**Issue**: Repository contains Windows-specific PowerShell scripts not needed on Linux.

**Resolution**: Added to `.gitignore`:
- `install_rust.ps1`
- `install_tools.ps1`
- `verify_rust.ps1`
- `find_msvc.ps1`
- `fix_linker.ps1`
- `install_msvc.ps1`

These files can be safely deleted or ignored.

## Quick Start Commands

### Infrastructure Management
```bash
# Start all services
docker compose -f deploy/docker/docker-compose.yml up -d

# Check service status
docker ps

# View logs
docker compose -f deploy/docker/docker-compose.yml logs -f

# Stop all services
docker compose -f deploy/docker/docker-compose.yml down

# Stop and remove volumes (CAUTION: deletes data)
docker compose -f deploy/docker/docker-compose.yml down -v
```

### Database Management
```bash
# Run migrations
sqlx migrate run --source migrations

# Revert last migration
sqlx migrate revert --source migrations

# Check database connection
psql postgres://gateway:gateway_dev_password@localhost:5432/sovereign_gateway -c "SELECT version();"
```

### Build & Development
```bash
# Build entire workspace
cargo build --workspace

# Build specific crate
cargo build -p gateway-server

# Build release version
cargo build --workspace --release

# Run gateway server
cargo run -p gateway-server

# Watch mode (requires cargo-watch)
cargo install cargo-watch
cargo watch -x 'clippy --workspace' -x 'nextest run'
```

### Testing
```bash
# Run all tests
cargo nextest run --workspace

# Run tests for specific crate
cargo nextest run -p gateway-core

# Run specific test
cargo nextest run test_sha256_deterministic

# Run with output
cargo nextest run --workspace --no-capture
```

### Code Quality
```bash
# Run clippy (linter)
cargo clippy --workspace --all-targets -- -D warnings

# Format code
cargo fmt --all

# Check formatting
cargo fmt --all --check

# Security audit
cargo install cargo-deny
cargo deny check
```

## Service Access

### Local URLs
- **Gateway Server**: http://localhost:8080 (when running)
- **Jaeger UI**: http://localhost:16686
- **Grafana**: http://localhost:3000 (admin/admin)
- **Qdrant Dashboard**: http://localhost:6333/dashboard
- **PostgreSQL**: localhost:5432

### Default Credentials
- **PostgreSQL**:
  - User: `gateway`
  - Password: `gateway_dev_password`
  - Database: `sovereign_gateway`
- **Grafana**:
  - User: `admin`
  - Password: `admin`

## Troubleshooting

### Build Failures
```bash
# Clean build artifacts
rm -rf /tmp/sovereign-gateway-target

# Rebuild from scratch
cargo clean  # This will clean the configured target dir
cargo build --workspace
```

### Database Connection Issues
```bash
# Check if PostgreSQL is running
docker ps | grep postgres

# Check PostgreSQL logs
docker logs sg-postgres

# Test connection
psql $DATABASE_URL -c "SELECT 1;"
```

### Docker Issues
```bash
# Restart Docker daemon
sudo systemctl restart docker

# Check Docker status
sudo systemctl status docker

# View Docker logs
journalctl -u docker -f
```

### Permission Issues (OneDrive)
If you encounter persistent permission issues:
1. Move project outside OneDrive: `mv ~/OneDrive/Documents/LLM/Projects/AI_Gateway ~/projects/`
2. Update `.cargo/config.toml` to use local target directory or remove it
3. Rebuild project

## Next Steps

### Recommended Actions
1. ✅ Environment setup complete
2. ⏭️ Configure provider API keys in `.env` (OpenAI, Anthropic)
3. ⏭️ Implement API key authentication
4. ⏭️ Build core routing logic
5. ⏭️ Add policy engine implementation

### Optional Enhancements
- Install `cargo-watch` for auto-rebuild on file changes
- Set up IDE integration (rust-analyzer)
- Configure pre-commit hooks for formatting/linting
- Set up local Prometheus (outside OneDrive)
- Install additional Rust tools (cargo-audit, cargo-expand, etc.)

## Maintenance

### Regular Tasks
- Update Rust toolchain: `rustup update`
- Update dependencies: `cargo update`
- Run security audit: `cargo deny check`
- Check for outdated dependencies: `cargo outdated` (requires cargo-outdated)
- Clean old Docker volumes: `docker system prune -a`

### Backup Considerations
- Database is ephemeral (Docker volume) - export data if needed
- Build artifacts in `/tmp` are cleared on reboot
- Configuration files (`.env`, `config/`, `policies/`) should be backed up
