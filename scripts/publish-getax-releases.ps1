# Compatibility wrapper: all production releases run from validated main in CI.
param([Parameter(Mandatory = $true)][string]$Tag)
$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'release-tag.ps1') -Tag $Tag
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
