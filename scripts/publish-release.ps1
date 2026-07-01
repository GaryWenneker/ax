# Tag and push a release (triggers .github/workflows/release.yml)
# Deprecated wrapper — use .\scripts\release-tag.ps1 instead.
param(
    [Parameter(Mandatory = $true)]
    [string]$Version,
    [switch]$Force,
    [switch]$Wait
)

$ErrorActionPreference = 'Stop'
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$args = @('-Tag', $Version)
if ($Force) { $args += '-Force' }
if ($Wait) { $args += '-Wait' }
& (Join-Path $scriptDir 'release-tag.ps1') @args
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
