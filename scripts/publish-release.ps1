# Dispatch a release (runs .github/workflows/release.yml).
# Deprecated wrapper — use .\scripts\release-tag.ps1 instead.
param(
    [Parameter(Mandatory = $true)]
    [string]$Version,
    [switch]$Wait
)

$ErrorActionPreference = 'Stop'
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$releaseArgs = @{ Tag = $Version }
if ($Wait) { $releaseArgs.Wait = $true }
& (Join-Path $scriptDir 'release-tag.ps1') @releaseArgs
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
