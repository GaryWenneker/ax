# Tag and push a release — triggers GitHub Actions to build all six OS binaries.
#
# Prerequisites:
#   - Commit and push your changes to main first
#   - git remote origin → GitHub (HTTPS or SSH credentials configured)
#
# Usage:
#   .\scripts\release-tag.ps1              # tags v2.0.6 (default)
#   .\scripts\release-tag.ps1 -Tag v2.0.7
#   .\scripts\release-tag.ps1 -Force       # re-push existing tag without prompt
#
# After CI completes (~15 min):
#   Windows:  irm https://getax.wenneker.io/install.ps1 | iex
#   macOS/Linux/WSL: curl -fsSL https://getax.wenneker.io/install.sh | sh
#
param(
    [string]$Tag = "v2.0.6",
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root

function Resolve-GitExe {
    $cmd = Get-Command git -ErrorAction SilentlyContinue
    if ($cmd -and $cmd.Source) { return $cmd.Source }

    foreach ($candidate in @(
            "$env:ProgramFiles\Git\cmd\git.exe"
            "$env:ProgramFiles\Git\bin\git.exe"
            "${env:ProgramFiles(x86)}\Git\cmd\git.exe"
            "$env:LOCALAPPDATA\Programs\Git\cmd\git.exe"
        )) {
        if ($candidate -and (Test-Path -LiteralPath $candidate)) { return $candidate }
    }

    $whereExe = Join-Path $env:SystemRoot 'System32\where.exe'
    if (Test-Path -LiteralPath $whereExe) {
        try {
            $found = & $whereExe git 2>$null | Select-Object -First 1
            if ($found -and (Test-Path -LiteralPath $found.Trim())) {
                return $found.Trim()
            }
        } catch {
            # where unavailable — common paths above are enough on most machines
        }
    }

    throw @"
git not found. Install Git for Windows or add it to PATH:
  https://git-scm.com/download/win
Common location: C:\Program Files\Git\cmd\git.exe
"@
}

function Invoke-Git {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$GitArgs)
    & $script:GitExe @GitArgs
    if ($LASTEXITCODE -ne 0) {
        throw "git $($GitArgs -join ' ') failed (exit $LASTEXITCODE)"
    }
}

$script:GitExe = Resolve-GitExe

if ($Tag -notmatch '^v') { $Tag = "v$Tag" }

try {
    Invoke-Git remote get-url origin | Out-Null
} catch {
    throw "No git remote 'origin'. Add: git remote add origin https://github.com/GaryWenneker/ax.git"
}

$version = $Tag.TrimStart('v')
$cargoVer = Select-String -Path "crates\ax-cli\Cargo.toml" -Pattern '^version = "(.+)"' | ForEach-Object { $_.Matches[0].Groups[1].Value }
if ($cargoVer -ne $version) {
    throw "Cargo.toml version is v$cargoVer but tag is $Tag. Align versions before releasing."
}

$branch = Invoke-Git rev-parse --abbrev-ref HEAD
if ($branch -ne 'main') {
    Write-Warning "Not on main (on $branch). Push main before tagging."
}

$latestPath = Join-Path $root 'site\public\releases\latest.txt'
$latestCurrent = if (Test-Path $latestPath) { (Get-Content $latestPath -Raw).Trim() } else { '' }
if ($latestCurrent -ne $Tag) {
    $utf8 = New-Object System.Text.UTF8Encoding $false
    [System.IO.File]::WriteAllText($latestPath, "$Tag`n", $utf8)
    Invoke-Git add $latestPath
    Invoke-Git commit -m "Publish $Tag release mirror to getax CDN"
    Write-Host "Updated latest.txt to $Tag and committed."
}

$unpushed = & $GitExe log "origin/$branch..HEAD" --oneline 2>$null
if ($LASTEXITCODE -eq 0 -and $unpushed) {
    Write-Host "Unpushed commits on $branch - pushing first..."
    Invoke-Git push origin $branch
}

$existing = & $GitExe tag -l $Tag 2>$null
if ($existing) {
    Write-Host "Tag $Tag already exists locally."
    if (-not $Force) {
        $ans = Read-Host "Re-push tag to origin? (y/N)"
        if ($ans -ne 'y') { Pop-Location; exit 0 }
    }
    Invoke-Git push origin $Tag --force
} else {
    Invoke-Git tag -a $Tag -m "Release $Tag - install.ps1 / install.sh all platforms"
    Invoke-Git push origin $Tag
}

Write-Host ""
Write-Host "Release workflow: https://github.com/GaryWenneker/ax/actions/workflows/release.yml" -ForegroundColor Cyan
Write-Host "When green, install:" -ForegroundColor Green
Write-Host "  Windows:         irm https://getax.wenneker.io/install.ps1 | iex"
Write-Host '  macOS/Linux/WSL: curl -fsSL https://getax.wenneker.io/install.sh | sh'

Pop-Location
