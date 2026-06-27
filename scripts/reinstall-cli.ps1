# Reinstall ax CLI to ~/.cargo/bin after a local build.
# Usage: .\scripts\reinstall-cli.ps1

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $root

Write-Host "Stopping ax daemon (if running)..." -ForegroundColor Cyan
& ax daemon stop 2>$null
Start-Sleep -Milliseconds 500

Write-Host "Installing ax-cli to cargo bin..." -ForegroundColor Cyan
cargo install --path crates/ax-cli --force
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$bin = (Get-Command ax -ErrorAction Stop).Source
$ver = & ax --version
Write-Host "$ver" -ForegroundColor Green
Write-Host "Installed: $bin" -ForegroundColor Green
