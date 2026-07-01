# One-shot ops bootstrap: git repo, GitHub push, optional release tag.
param(
    [string]$Repo = "GaryWenneker/ax",
    [string]$Tag = "",
    [switch]$SkipTag
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root

function Ensure-Git {
    if (-not (Test-Path .git)) {
        git init -b main
        Write-Host "Initialized git repository (branch main)."
    }
    git add -A
    $status = git status --porcelain
    if ($status) {
        git commit -m "ax v2.0.6 - database-first policy engine, release pipeline"
        Write-Host "Committed working tree."
    } else {
        Write-Host "Nothing to commit."
    }
}

function Ensure-GithubRepo {
    $remote = $null
    try {
        $remote = git remote get-url origin
    } catch {
        $remote = $null
    }
    if (-not $remote) {
        gh repo create $Repo --public --source=. --remote=origin --push
        Write-Host "Created and pushed $Repo"
        return
    }
    Write-Host "Remote origin: $remote"
    git push -u origin main
}

Ensure-Git
Ensure-GithubRepo

if (-not $SkipTag) {
    $releaseScript = Join-Path $PSScriptRoot 'release-tag.ps1'
    if (-not $Tag) { $Tag = 'v' + (Select-String -Path (Join-Path $root 'crates\ax-cli\Cargo.toml') -Pattern '^version = "(.+)"' | ForEach-Object { $_.Matches[0].Groups[1].Value }) }
    & $releaseScript -Tag $Tag -Force
}

Write-Host ""
Write-Host "Next (optional):"
Write-Host "  Telemetry: set CLOUDFLARE_API_TOKEN + POSTHOG_KEY repo secrets, run Deploy telemetry worker workflow"
Write-Host "  Or local:    .\scripts\deploy-telemetry.ps1 -Dev"
Write-Host "  Docs site:   enable GitHub Pages (source: GitHub Actions) after first site workflow run"
Write-Host "  Pages setup: .\scripts\enable-github-pages.ps1"

Pop-Location
