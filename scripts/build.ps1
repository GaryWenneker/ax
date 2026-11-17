# Build ax without fighting a running MCP server (Windows locks ax.exe).
param(
    [switch]$Install
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $root

# Always build to target-dev so a running MCP (target-ui or ~/.cargo/bin) does not lock ax.exe.
$env:CARGO_TARGET_DIR = "target-dev"

$running = Get-Process ax -ErrorAction SilentlyContinue
if ($running) {
    Write-Host "Note: ax is running (PID $($running.Id -join ', ')). Building to target-dev\release\ax.exe (MCP can stay up)." -ForegroundColor Yellow
}

cargo build --release -p ax-cli
$built = Join-Path $root "target-dev\release\ax.exe"
if (-not (Test-Path $built)) {
    Write-Error "Build failed — ax.exe not found at $built"
}

Write-Host "Built: $built" -ForegroundColor Green

if ($Install) {
    if ($running) {
        Write-Host "Skip -Install: stop ax MCP in Cursor first, then run:" -ForegroundColor Yellow
        Write-Host "  cargo install --path crates/ax-cli --force" -ForegroundColor Cyan
    } else {
        cargo install --path crates/ax-cli --force
    }
}
