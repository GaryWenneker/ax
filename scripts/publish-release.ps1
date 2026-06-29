# Tag and push a release (triggers .github/workflows/release.yml)
param(
    [Parameter(Mandatory = $true)]
    [string]$Version
)

$ErrorActionPreference = 'Stop'
if ($Version -notmatch '^v') {
    $Version = "v$Version"
}

Write-Host "Creating tag $Version..."
git tag -a $Version -m "Release $Version"
git push origin $Version
Write-Host "Pushed $Version — watch GitHub Actions 'Release' workflow."
