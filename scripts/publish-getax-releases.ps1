# Upload dist/ax-* release archives to getax.wenneker.io
param(
    [Parameter(Mandatory = $true)]
    [string]$Tag
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$dist = Join-Path $root 'dist'
$site = Join-Path $root 'site'
$releaseDir = Join-Path $site "public\releases\$Tag"

$archives = Get-ChildItem -Path $dist -Filter 'ax-*' -ErrorAction SilentlyContinue
if (-not $archives) {
    throw "No archives in $dist — run Release CI or build locally first."
}

& (Join-Path $PSScriptRoot 'verify-release-assets.ps1') -DistDir $dist

New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null
Copy-Item -Path (Join-Path $dist 'ax-*') -Destination $releaseDir -Force
if (Test-Path (Join-Path $dist 'SHA256SUMS')) {
    Copy-Item (Join-Path $dist 'SHA256SUMS') $releaseDir
}
$utf8 = New-Object System.Text.UTF8Encoding $false
[System.IO.File]::WriteAllText((Join-Path $site 'public\releases\latest.txt'), $Tag, $utf8)
Copy-Item (Join-Path $root 'install.sh') (Join-Path $site 'public\install.sh') -Force
Copy-Item (Join-Path $root 'install.ps1') (Join-Path $site 'public\install.ps1') -Force

Write-Host "Staged $($archives.Count) archives under site/public/releases/$Tag/"

Push-Location $site
try {
    if (-not (Test-Path 'node_modules')) { npm ci }
    npm run build
    netlify deploy --prod --dir=dist --message="Release $Tag binaries"
} finally {
    Pop-Location
}

Write-Host "Published $Tag to https://getax.wenneker.io/releases/$Tag/" -ForegroundColor Green
