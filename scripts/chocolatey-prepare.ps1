# Stamp packaging/chocolatey/ax-agent placeholders from a published release.
# Runs on Windows runners (and on macOS if python3 is available).
param(
    [Parameter(Mandatory = $true)]
    [string]$Tag,
    [string]$Repo = 'GaryWenneker/ax',
    [string]$SumsFile = ''
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$py = Join-Path $root 'scripts/chocolatey_prepare.py'

$args = @($py, '--tag', $Tag, '--repo', $Repo)
if ($SumsFile) {
    $args += @('--sums-file', $SumsFile)
}

python3 @args
if ($LASTEXITCODE -ne 0) {
    throw "chocolatey_prepare.py failed with exit $LASTEXITCODE"
}
