# Release only the already merged, validated main source through GitHub Actions.
param(
    [string]$Tag = '',
    [ValidateSet('patch', 'minor', 'major', '')][string]$Bump = '',
    [switch]$Force,
    [switch]$Wait,
    [switch]$DryRun,
    [switch]$AllowDirty,
    [string]$Repo = 'GaryWenneker/ax',
    [int]$WaitTimeoutMinutes = 25
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    if ($Bump -or $Force -or $AllowDirty) {
        throw 'Version changes must go through a PR to main. Retagging and dirty releases are forbidden.'
    }
    $branch = (& git branch --show-current).Trim()
    if ($LASTEXITCODE -ne 0 -or $branch -ne 'main') { throw 'Releases must run from main.' }
    if (& git status --porcelain) { throw 'Commit changes through a PR before releasing.' }
    & git fetch origin main
    if ($LASTEXITCODE -ne 0) { throw 'Cannot fetch origin/main.' }
    $headSha = (& git rev-parse HEAD).Trim()
    $mainSha = (& git rev-parse origin/main).Trim()
    if ($headSha -ne $mainSha) { throw 'Local main must equal origin/main. Run git pull --ff-only.' }
    if (-not (Get-Command gh -ErrorAction SilentlyContinue)) { throw 'Install GitHub CLI and run gh auth login.' }
    if ($Tag -and $Tag -notmatch '^v') { $Tag = "v$Tag" }
    $releaseArgs = @('workflow', 'run', 'release.yml', '--repo', $Repo, '--ref', 'main')
    if ($Tag) { $releaseArgs += @('-f', "tag=$Tag") }
    if ($DryRun) { Write-Host "gh $($releaseArgs -join ' ')"; return }
    & gh @releaseArgs
    if ($LASTEXITCODE -ne 0) { throw 'Release workflow dispatch failed.' }
    if ($Wait) {
        Write-Host "Follow the validated main release: https://github.com/$Repo/actions/workflows/release.yml"
    }
} finally { Pop-Location }
