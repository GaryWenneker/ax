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

function Find-AxBinary {
    param([string]$Root, [string]$RustTarget)
    $candidates = @(
        $(if ($env:CARGO_TARGET_DIR) { Join-Path $env:CARGO_TARGET_DIR "$RustTarget\release\ax.exe" })
        (Join-Path $Root "target-dev\$RustTarget\release\ax.exe")
        (Join-Path $Root "target\$RustTarget\release\ax.exe")
        (Join-Path $Root 'target-dev\release\ax.exe')
        (Join-Path $Root 'target\release\ax.exe')
    ) | Where-Object { $_ }
    foreach ($path in $candidates) {
        if (Test-Path $path) { return $path }
    }
    return $null
}

$bin = Find-AxBinary -Root $root -RustTarget $RustTarget
if (-not $bin) {
    throw "Binary not found. Run: cargo build --release -p ax-cli --target $RustTarget"
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
