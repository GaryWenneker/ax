# Build, package, and optionally install a local release (dev workflow).
#
# Usage:
#   .\scripts\ship-local.ps1                    # build + package win32-x64 zip in dist/
#   .\scripts\ship-local.ps1 -Upgrade           # also run ax upgrade --local
#   .\scripts\ship-local.ps1 -Bump patch        # set the next version locally first (2.1.0 -> 2.1.1)
#   .\scripts\ship-local.ps1 -SkipBuild         # package existing release binary only
#
param(
    [ValidateSet('patch', 'minor', 'major', '')]
    [string]$Bump = '',
    [switch]$Upgrade,
    [switch]$SkipBuild,
    [switch]$SkipKill
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$env:CARGO_TARGET_DIR = 'target-dev'

function Write-Step {
    param([string]$Message)
    Write-Host ""
    Write-Host "==> $Message" -ForegroundColor Cyan
}

function Get-AxCargoVersion {
    $match = Select-String -Path (Join-Path $root 'crates\ax-cli\Cargo.toml') -Pattern '^version = "(.+)"' |
        Select-Object -First 1
    if (-not $match) { throw 'Could not read version from crates/ax-cli/Cargo.toml' }
    return $match.Matches[0].Groups[1].Value
}

function Get-NextVersion {
    param([string]$Current, [string]$Kind)
    $parts = $Current.Split('.')
    if ($parts.Count -ne 3) { throw "Expected semver x.y.z, got $Current" }
    [int]$major = $parts[0]
    [int]$minor = $parts[1]
    [int]$patch = $parts[2]
    switch ($Kind) {
        'major' { return "$(($major + 1)).0.0" }
        'minor' { return "$major.$(($minor + 1)).0" }
        'patch' { return "$major.$minor.$(($patch + 1))" }
        default { throw "Unknown bump kind: $Kind" }
    }
}

function Set-AxLocalVersion {
    param([string]$Version)
    Write-Step "Setting workspace version to $Version (local build only)"
    $python = if (Get-Command python3 -ErrorAction SilentlyContinue) { 'python3' } else { 'python' }
    & $python (Join-Path $PSScriptRoot 'set-version.py') $Version
    if ($LASTEXITCODE -ne 0) { throw 'set-version.py failed' }
    cargo update --workspace --offline
    if ($LASTEXITCODE -ne 0) { throw 'cargo update --workspace --offline failed' }
    Write-Host 'Version changes reach a release only through a PR to main; release-tag.ps1 refuses a local bump.' -ForegroundColor Yellow
}

if ($Bump) {
    $next = Get-NextVersion -Current (Get-AxCargoVersion) -Kind $Bump
    Set-AxLocalVersion -Version $next
}

$version = Get-AxCargoVersion
Write-Step "Ship local v$version"

if (-not $SkipKill) {
    & (Join-Path $PSScriptRoot 'release-local.ps1') -SkipBuild -SkipInstall
}

if (-not $SkipBuild) {
    Write-Step 'cargo build --release -p ax-cli'
    cargo build --release -p ax-cli
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

Write-Step 'Package win32-x64 release archive'
& (Join-Path $PSScriptRoot 'package-release.ps1') -Bundle win32-x64 -RustTarget x86_64-pc-windows-msvc
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$archive = Join-Path $root 'dist\ax-win32-x64.zip'
if (-not (Test-Path $archive)) {
    throw "Expected archive at $archive"
}

Write-Host ""
Write-Host "Packaged: $archive" -ForegroundColor Green
Write-Host "Version:  $version" -ForegroundColor Green
Write-Host ""
Write-Host "Local upgrade:" -ForegroundColor Yellow
Write-Host "  ax upgrade --local"
Write-Host "  ax upgrade --local dist\ax-win32-x64.zip"
Write-Host ""
Write-Host "Publish release (GitHub + getax), after the version is merged to main through a PR:" -ForegroundColor Yellow
Write-Host "  .\scripts\release-tag.ps1 -Wait"

if ($Upgrade) {
    Write-Step 'ax upgrade --local'
    ax upgrade --local
}
