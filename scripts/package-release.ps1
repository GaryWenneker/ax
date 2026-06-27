# Package ax release archive on Windows (local maintainer).
param(
    [Parameter(Mandatory = $true)]
    [string]$Bundle,
    [Parameter(Mandatory = $true)]
    [string]$RustTarget
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$dist = Join-Path $root 'dist'
$stage = Join-Path $dist "ax-$Bundle"
$bin = Join-Path $root "target\$RustTarget\release\ax.exe"

if (-not (Test-Path $bin)) {
    throw "Binary not found: $bin — run: cargo build --release -p ax-cli --target $RustTarget"
}

New-Item -ItemType Directory -Force -Path $dist | Out-Null
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item $bin (Join-Path $stage 'ax.exe')

$zip = Join-Path $dist "ax-$Bundle.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path $stage -DestinationPath $zip -Force
Remove-Item -Recurse -Force $stage
Write-Host "Created $zip"
