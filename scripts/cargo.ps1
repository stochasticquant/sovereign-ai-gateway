#!/usr/bin/env pwsh
# Wrapper script to run cargo with the correct PATH on Windows.
# Ensures MSYS2 MinGW-w64 gcc is used instead of broken w64devkit.
#
# Usage: pwsh scripts/cargo.ps1 build --workspace
#        pwsh scripts/cargo.ps1 test --workspace
#        pwsh scripts/cargo.ps1 clippy --workspace -- -D warnings

$cargoHome = "$env:USERPROFILE\.cargo\bin"
$cargo = "$cargoHome\cargo.exe"

# Fix PATH: MSYS2 first, remove w64devkit
$env:Path = "C:\msys64\mingw64\bin;$cargoHome;$env:Path"
$pathParts = $env:Path -split ";"
$env:Path = ($pathParts | Where-Object { $_ -notmatch "w64devkit" }) -join ";"

& $cargo @args
