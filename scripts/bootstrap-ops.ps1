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
        git commit -m "ax v0.1.0 - Rust code intelligence CLI and MCP"
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
    if (-not $Tag) { $Tag = "v0.1.0" }
    if ($Tag -notmatch '^v') { $Tag = "v$Tag" }
    $exists = git tag -l $Tag
    if (-not $exists) {
        git tag -a $Tag -m "Release $Tag"
    }
    git push origin $Tag
    Write-Host "Pushed tag $Tag - GitHub Actions Release workflow should build binaries."
    Write-Host "https://github.com/$Repo/actions"
}

Write-Host ""
Write-Host "Next (optional):"
Write-Host "  Telemetry: set CLOUDFLARE_API_TOKEN + POSTHOG_KEY repo secrets, run Deploy telemetry worker workflow"
Write-Host "  Or local:    .\scripts\deploy-telemetry.ps1 -Dev"
Write-Host "  Docs site:   enable GitHub Pages (source: GitHub Actions) after first site workflow run"
Write-Host "  Pages setup: .\scripts\enable-github-pages.ps1"

Pop-Location
