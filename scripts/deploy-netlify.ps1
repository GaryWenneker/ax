# Deploy ax docs site + Netlify Functions to getax.wenneker.io
#
# Usage:
#   .\scripts\deploy-netlify.ps1              # build + production deploy
#   .\scripts\deploy-netlify.ps1 -SkipBuild   # deploy existing dist/
#   .\scripts\deploy-netlify.ps1 -Preview     # draft deploy (no production)
#
param(
    [switch]$SkipBuild,
    [switch]$Preview,
    [switch]$SkipFunctionsCache
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$siteDir = Join-Path $root 'site'
$distDir = Join-Path $siteDir 'dist'
$functionsDir = Join-Path $siteDir 'netlify/functions'

function Require-Command {
    param([string]$Name)
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "$Name is not installed. Install Node.js 22+ and run: npm install -g netlify-cli"
    }
}

function Write-Step {
    param([string]$Message)
    Write-Host ""
    Write-Host "==> $Message" -ForegroundColor Cyan
}

Require-Command 'node'
Require-Command 'npm'
Require-Command 'netlify'

if (-not (Test-Path $siteDir)) {
    throw "Site directory not found: $siteDir"
}

Push-Location $siteDir
try {
    Write-Step "Checking Netlify link"
    $statusJson = netlify status --json 2>$null
    if (-not $statusJson) {
        throw @"
This folder is not linked to a Netlify site.
From $siteDir run once:
  netlify link
"@
    }
    $status = $statusJson | ConvertFrom-Json
    $siteName = $status.siteData.name
    $siteUrl = $status.siteData.url
    Write-Host "Site: $siteName ($siteUrl)" -ForegroundColor DarkGray

    if (-not $SkipBuild) {
        Write-Step "Installing npm dependencies (if needed)"
        if (-not (Test-Path 'node_modules')) {
            npm ci
        }

        Write-Step "Building Astro site"
        npm run build
    } elseif (-not (Test-Path $distDir)) {
        throw "dist/ not found — run without -SkipBuild first"
    }

    Write-Step $(if ($Preview) { 'Deploying preview (draft)' } else { 'Deploying production' })

    $deployArgs = @(
        'deploy',
        '--dir=dist',
        "--functions=$functionsDir"
    )
    if ($Preview) {
        $deployArgs += '--draft'
    } else {
        $deployArgs += '--prod'
    }
    if ($SkipFunctionsCache) {
        $deployArgs += '--skip-functions-cache'
    }

    & netlify @deployArgs
    if ($LASTEXITCODE -ne 0) {
        throw "netlify deploy failed (exit $LASTEXITCODE)"
    }

    Write-Host ""
    Write-Host "Deploy complete." -ForegroundColor Green
    if ($Preview) {
        Write-Host "Preview URL: see netlify output above (draft deploy)." -ForegroundColor Yellow
    } else {
        Write-Host "Production: https://getax.wenneker.io" -ForegroundColor Green
        Write-Host "Telemetry ingest: https://getax.wenneker.io/v1/events"
        Write-Host "Admin dashboard: https://getax.wenneker.io/admin/dashboard/"
    }

    Write-Host ""
    Write-Host "Required Netlify env vars (Site settings -> Environment variables):" -ForegroundColor DarkGray
    Write-Host "  POSTHOG_KEY, POSTHOG_PERSONAL_KEY, POSTHOG_PROJECT_ID, ADMIN_SECRET"
} finally {
    Pop-Location
}
