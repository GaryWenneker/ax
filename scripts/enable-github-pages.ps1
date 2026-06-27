# Enable GitHub Pages (workflow source) for a new repo.
# Usage: .\scripts\enable-github-pages.ps1 -Repo GaryWenneker/ax
param(
    [string]$Repo = "GaryWenneker/ax"
)

$ErrorActionPreference = 'Stop'
gh api -X POST "repos/$Repo/pages" -f build_type=workflow
Write-Host "GitHub Pages enabled: https://$($Repo.Split('/')[0]).github.io/ax/"
