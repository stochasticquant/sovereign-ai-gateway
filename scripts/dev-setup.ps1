#!/usr/bin/env pwsh
# Development environment setup for Sovereign AI Gateway.
# Run this script after cloning to set up your local dev environment.
#
# Prerequisites:
#   - Rust (rustup): https://rustup.rs
#   - MSYS2 with MinGW-w64 gcc: pacman -S mingw-w64-x86_64-gcc
#   - Docker Desktop: https://www.docker.com/products/docker-desktop
#
# Usage: pwsh -ExecutionPolicy Bypass -File scripts/dev-setup.ps1

$ErrorActionPreference = "Stop"
$cargoHome = "$env:USERPROFILE\.cargo\bin"
$cargo = "$cargoHome\cargo.exe"

# Ensure MSYS2 MinGW-w64 is in PATH before w64devkit
$env:Path = "C:\msys64\mingw64\bin;$cargoHome;$env:Path"
$pathParts = $env:Path -split ";"
$env:Path = ($pathParts | Where-Object { $_ -notmatch "w64devkit" }) -join ";"

Write-Host "=== Sovereign AI Gateway — Dev Setup ===" -ForegroundColor Cyan

# 1. Verify Rust
Write-Host "`n--- Rust Toolchain ---" -ForegroundColor Yellow
& $cargoHome\rustc.exe --version
& $cargoHome\cargo.exe --version

# 2. Install Rust components
Write-Host "`n--- Installing Rust components ---" -ForegroundColor Yellow
& $cargoHome\rustup.exe component add clippy rustfmt

# 3. Install cargo tools
Write-Host "`n--- Installing cargo tools ---" -ForegroundColor Yellow
$tools = @(
    @{ name = "cargo-deny"; args = @("install", "cargo-deny", "--locked") },
    @{ name = "cargo-watch"; args = @("install", "cargo-watch", "--locked") },
    @{ name = "cargo-nextest"; args = @("install", "cargo-nextest", "--locked") }
)

foreach ($tool in $tools) {
    Write-Host "  Installing $($tool.name)..."
    & $cargo $tool.args 2>&1 | Out-Null
}

# 4. Build workspace
Write-Host "`n--- Building workspace ---" -ForegroundColor Yellow
& $cargo build --workspace

# 5. Run checks
Write-Host "`n--- Running checks ---" -ForegroundColor Yellow
& $cargo fmt --all -- --check
& $cargo clippy --workspace --all-targets -- -D warnings
& $cargo test --workspace

# 6. Docker services
Write-Host "`n--- Docker Services ---" -ForegroundColor Yellow
$docker = Get-Command docker -ErrorAction SilentlyContinue
if ($docker) {
    Write-Host "Starting dev infrastructure..."
    docker compose -f deploy/docker/docker-compose.yml up -d
} else {
    Write-Host "Docker not found. Install Docker Desktop to run dev infrastructure."
    Write-Host "  https://www.docker.com/products/docker-desktop"
}

Write-Host "`n=== Setup complete! ===" -ForegroundColor Green
Write-Host "Run 'cargo watch -x clippy -x test' to start developing."
