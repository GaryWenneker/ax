# Reinstall ax CLI from target-dev/release (no rebuild). Kills ax first, then
# copy-syncs to ~/.cargo/bin + %LOCALAPPDATA%\ax\current\{,bin\}.
# For clean release build + sync use: .\scripts\release-local.ps1
param(
    [switch]$SkipKill
)

$ErrorActionPreference = 'Stop'
$releaseScript = Join-Path $PSScriptRoot 'release-local.ps1'
& $releaseScript -SkipBuild @PSBoundParameters
exit $LASTEXITCODE
