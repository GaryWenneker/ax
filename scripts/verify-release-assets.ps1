# Verify all six ax release archives exist before GitHub upload or getax deploy.
param(
    [string]$DistDir = (Join-Path (Split-Path -Parent $PSScriptRoot) 'dist')
)

$Required = @(
    'ax-win32-x64.zip',
    'ax-win32-arm64.zip',
    'ax-linux-x64.tar.gz',
    'ax-linux-arm64.tar.gz',
    'ax-darwin-x64.tar.gz',
    'ax-darwin-arm64.tar.gz'
)

$missing = @()
foreach ($name in $Required) {
    if (-not (Test-Path (Join-Path $DistDir $name))) {
        $missing += $name
    }
}

if ($missing.Count -gt 0) {
    $list = ($missing | ForEach-Object { "  - $_" }) -join "`n"
    Write-Error @"
Incomplete release — missing $($missing.Count)/6 required asset(s) in ${DistDir}:
$list
Required for Windows, macOS (Intel + Apple Silicon), Linux, and WSL2 (linux-*).
Run Release CI for tag v* or build all targets locally before publish.
"@
    exit 1
}

Write-Host "OK: all 6 release assets present in $DistDir" -ForegroundColor Green
