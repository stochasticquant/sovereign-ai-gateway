#!/usr/bin/env pwsh
# Install Docker Desktop for Windows via winget.
# Requires restart after installation.

$ErrorActionPreference = "Continue"

$winget = "C:\Users\$env:USERNAME\AppData\Local\Microsoft\WindowsApps\winget.exe"

if (Test-Path $winget) {
    Write-Host "Installing Docker Desktop via winget..."
    & $winget install Docker.DockerDesktop --accept-source-agreements --accept-package-agreements
    Write-Host "`nDocker Desktop installed. Please restart your machine to complete setup."
} else {
    Write-Host "winget not available. Download Docker Desktop manually from:"
    Write-Host "  https://www.docker.com/products/docker-desktop"
}
