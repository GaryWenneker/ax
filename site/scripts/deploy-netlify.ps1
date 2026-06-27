# Deploy ax docs + telemetry to Netlify (getax.wenneker.io)
param(
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$siteDir = Split-Path -Parent $PSScriptRoot
Push-Location $siteDir

try {
    if (-not $SkipBuild) {
        if (-not (Test-Path node_modules)) { npm install }
        npm run build
    }

    netlify deploy --prod --dir=dist --functions=netlify/functions

    $status = netlify status --json 2>$null | ConvertFrom-Json
    $url = $status.siteData.url
    Write-Host ""
    Write-Host "Deployed: $url"
    Write-Host ""
    Write-Host "Custom domain setup (one-time):"
    Write-Host "  1. Netlify UI -> Domain management -> Add getax.wenneker.io"
    Write-Host "  2. DNS at wenneker.io: CNAME getax -> $($url -replace 'https://','')"
    Write-Host ""
    Write-Host "Telemetry: https://getax.wenneker.io/v1/events"
    Write-Host "Set Netlify env: POSTHOG_KEY (required), POSTHOG_HOST (optional)"
} finally {
    Pop-Location
}
