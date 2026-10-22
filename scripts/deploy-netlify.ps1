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

function Park-FunctionEmit {
    param([string]$FunctionsDir)
    $park = Join-Path ([System.IO.Path]::GetTempPath()) ("ax-fn-emit-" + [guid]::NewGuid().ToString('n'))
    New-Item -ItemType Directory -Path $park | Out-Null
    Get-ChildItem -LiteralPath $FunctionsDir -File | Where-Object {
        $_.Name -like '*.js' -or $_.Name -like '*.js.map' -or $_.Name -like '*.d.ts'
    } | ForEach-Object {
        Move-Item -LiteralPath $_.FullName -Destination $park
    }
    return $park
}

function Restore-FunctionEmit {
    param([string]$Park, [string]$FunctionsDir)
    if (-not $Park -or -not (Test-Path -LiteralPath $Park)) { return }
    Get-ChildItem -LiteralPath $Park -File | ForEach-Object {
        Move-Item -LiteralPath $_.FullName -Destination $FunctionsDir
    }
    Remove-Item -LiteralPath $Park -Force
}

Require-Command 'node'
Require-Command 'npm'
Require-Command 'netlify'

if (-not (Test-Path $siteDir)) {
    throw "Site directory not found: $siteDir"
}

# Production is reproducible only from the clean, merged main commit.
if (-not $Preview) {
    if ($SkipBuild) { throw 'Production requires a fresh build from main; -SkipBuild is preview-only.' }
    $branch = (& git -C $root branch --show-current).Trim()
    if ($LASTEXITCODE -ne 0 -or $branch -ne 'main') { throw 'Production deploys must run from main.' }
    if (& git -C $root status --porcelain) { throw 'Production requires a clean checkout.' }
    & git -C $root fetch origin main
    if ($LASTEXITCODE -ne 0) { throw 'Cannot fetch origin/main.' }
    $headSha = (& git -C $root rev-parse HEAD).Trim()
    $mainSha = (& git -C $root rev-parse origin/main).Trim()
    if ($headSha -ne $mainSha) { throw 'Local main must equal origin/main.' }
    Require-Command 'gh'
    $ci = & gh run list --repo GaryWenneker/ax --workflow ci.yml --commit $headSha --limit 20 --json status,conclusion
    if ($LASTEXITCODE -ne 0) { throw 'Cannot verify main CI.' }
    $runs = @($ci | ConvertFrom-Json)
    $green = @($runs | Where-Object { $_.status -eq 'completed' -and $_.conclusion -eq 'success' })
    if ($green.Count -lt 1) {
        throw 'Production requires successful main workspace CI.'
    }
    $published = & gh release view --repo GaryWenneker/ax --json tagName,assets
    if ($LASTEXITCODE -ne 0) { throw 'Cannot resolve published downloads.' }
    $release = $published | ConvertFrom-Json
    foreach ($asset in @('ax-win32-x64.zip', 'ax-win32-arm64.zip', 'ax-linux-x64.tar.gz', 'ax-linux-arm64.tar.gz', 'ax-darwin-x64.tar.gz', 'ax-darwin-arm64.tar.gz')) {
        if ($asset -notin $release.assets.name) { throw "Published release is missing $asset." }
    }
    [System.IO.File]::WriteAllText((Join-Path $siteDir 'public/releases/latest.txt'), "$($release.tagName)`n", (New-Object System.Text.UTF8Encoding $false))
}

$parkedEmit = $null
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
        # npm prebuild refreshes the locked dependencies even when node_modules already exists.
        Write-Step "Building Astro site with current dependencies"
        npm run build
        if ($LASTEXITCODE -ne 0) { throw 'Site build failed.' }
        npm test
        if ($LASTEXITCODE -ne 0) { throw 'Site tests failed.' }
    } elseif (-not (Test-Path $distDir)) {
        throw "dist/ not found - run without -SkipBuild first"
    }

    # Compiled .js/.d.ts next to the TypeScript sources become invalid function names.
    $parkedEmit = Park-FunctionEmit $functionsDir
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
    Restore-FunctionEmit $parkedEmit $functionsDir
    Pop-Location
}
