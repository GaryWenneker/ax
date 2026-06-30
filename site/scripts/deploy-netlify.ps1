# Back-compat wrapper — canonical script: ..\..\scripts\deploy-netlify.ps1
param(
    [switch]$SkipBuild,
    [switch]$Preview,
    [switch]$SkipFunctionsCache
)

$repoScript = Join-Path (Split-Path -Parent (Split-Path -Parent $PSScriptRoot)) 'scripts\deploy-netlify.ps1'
& $repoScript @PSBoundParameters
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
