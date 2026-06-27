# Deploy ax telemetry worker (Cloudflare Wrangler)
# Requires: Node.js 20+, wrangler login OR CLOUDFLARE_API_TOKEN
param(
    [switch]$Dev
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$worker = Join-Path $root 'telemetry-worker'

if (-not (Test-Path $worker)) {
    throw "telemetry-worker not found at $worker"
}

Push-Location $worker
try {
    if (-not (Test-Path node_modules)) {
        npm ci
    }
    npm run check
    if ($Dev) {
        Write-Host "Starting wrangler dev..."
        npx wrangler dev
    } else {
        if (-not $env:CLOUDFLARE_API_TOKEN) {
            Write-Host "Tip: run 'wrangler login' or set CLOUDFLARE_API_TOKEN"
        }
        if (-not (Get-Command wrangler -ErrorAction SilentlyContinue)) {
            npx wrangler deploy
        } else {
            wrangler deploy
        }
        Write-Host "Deployed. Set client AX_TELEMETRY_ENDPOINT if not using telemetry.getax.dev"
    }
} finally {
    Pop-Location
}
