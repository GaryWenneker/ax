# Reinstall ax CLI to ~/.cargo/bin (no rebuild). Kills ax first.
# For clean release build + install use: .\scripts\release-local.ps1
param(
    [switch]$SkipKill
)

$ErrorActionPreference = 'Stop'
$releaseScript = Join-Path $PSScriptRoot 'release-local.ps1'
& $releaseScript -SkipBuild @PSBoundParameters
exit $LASTEXITCODE
