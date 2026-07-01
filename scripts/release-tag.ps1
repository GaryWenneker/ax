# Complete ax release orchestrator — sync site pointers, push main, tag, optional CI verify.
#
# Prerequisites:
#   - All crate Cargo.toml versions match the release tag (or use -Bump patch|minor|major)
#   - git remote origin -> GitHub with push access
#   - On branch main (recommended)
#
# Usage:
#   .\scripts\release-tag.ps1                    # tag from crates/ax-cli/Cargo.toml (e.g. v2.0.6)
#   .\scripts\release-tag.ps1 -Tag v2.0.7
#   .\scripts\release-tag.ps1 -Bump patch        # 2.0.6 -> 2.0.7, sync site, tag, push
#   .\scripts\release-tag.ps1 -Force             # retag current HEAD without prompts
#   .\scripts\release-tag.ps1 -Wait              # poll GitHub Actions + verify release assets/binary
#   .\scripts\release-tag.ps1 -DryRun            # print planned steps only
#
# After CI (~8-15 min without -Wait):
#   Windows:         irm https://getax.wenneker.io/install.ps1 | iex
#   macOS/Linux/WSL: curl -fsSL https://getax.wenneker.io/install.sh | sh
#
param(
    [string]$Tag = '',
    [ValidateSet('patch', 'minor', 'major', '')]
    [string]$Bump = '',
    [switch]$Force,
    [switch]$Wait,
    [switch]$DryRun,
    [switch]$AllowDirty,
    [string]$Repo = 'GaryWenneker/ax',
    [int]$WaitTimeoutMinutes = 25
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root

$ReleaseSitePaths = @(
    'site/public/releases/latest.txt'
    'site/public/install.ps1'
    'site/public/install.sh'
)

function Write-Step {
    param([string]$Message)
    Write-Host ""
    Write-Host "==> $Message" -ForegroundColor Cyan
}

function Write-Plan {
    param([string]$Message)
    Write-Host "[dry-run] $Message" -ForegroundColor Yellow
}

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
        } catch { }
    }

    throw @"
git not found. Install Git for Windows or add it to PATH:
  https://git-scm.com/download/win
"@
}

function Invoke-Git {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$GitArgs)
    if ($DryRun) {
        Write-Plan "git $($GitArgs -join ' ')"
        return
    }
    & $script:GitExe @GitArgs
    if ($LASTEXITCODE -ne 0) {
        throw "git $($GitArgs -join ' ') failed (exit $LASTEXITCODE)"
    }
}

function Write-Utf8NoBom {
    param([string]$Path, [string]$Content)
    $dir = Split-Path -Parent $Path
    if ($dir -and -not (Test-Path $dir)) {
        if ($DryRun) { Write-Plan "mkdir $dir"; return }
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
    }
    if ($DryRun) { Write-Plan "write $Path"; return }
    $utf8 = New-Object System.Text.UTF8Encoding $false
    [System.IO.File]::WriteAllText($Path, $Content, $utf8)
}

function Get-AxCargoVersion {
    $match = Select-String -Path (Join-Path $root 'crates\ax-cli\Cargo.toml') -Pattern '^version = "(.+)"' |
        Select-Object -First 1
    if (-not $match) { throw 'Could not read version from crates/ax-cli/Cargo.toml' }
    return $match.Matches[0].Groups[1].Value
}

function Get-NextVersion {
    param([string]$Current, [string]$Kind)
    $parts = $Current.Split('.')
    if ($parts.Count -ne 3) { throw "Expected semver x.y.z, got $Current" }
    [int]$major = $parts[0]
    [int]$minor = $parts[1]
    [int]$patch = $parts[2]
    switch ($Kind) {
        'major' { return "$(($major + 1)).0.0" }
        'minor' { return "$major.$(($minor + 1)).0" }
        'patch' { return "$major.$minor.$(($patch + 1))" }
        default { throw "Unknown bump kind: $Kind" }
    }
}

function Set-WorkspaceVersion {
    param([string]$Version)
    Write-Step "Setting workspace version to $Version"
    $cargoFiles = Get-ChildItem -Path (Join-Path $root 'crates') -Recurse -Filter Cargo.toml
    foreach ($file in $cargoFiles) {
        $text = [System.IO.File]::ReadAllText($file.FullName)
        $updated = [regex]::Replace($text, '(?m)^version = ".*"', "version = `"$Version`"", 1)
        if ($updated -eq $text) { continue }
        if ($DryRun) {
            Write-Plan "bump $($file.FullName) -> $Version"
        } else {
            Write-Utf8NoBom -Path $file.FullName -Content $updated
        }
    }

    $pkgPath = Join-Path $root 'crates\ax-web\web-ui\package.json'
    if (Test-Path $pkgPath) {
        $json = Get-Content $pkgPath -Raw
        $updatedJson = [regex]::Replace($json, '(?<="version"\s*:\s*")[^"]+', $Version, 1)
        if ($DryRun) {
            Write-Plan "bump $pkgPath -> $Version"
        } else {
            Write-Utf8NoBom -Path $pkgPath -Content $updatedJson
        }
    }

    if (-not $DryRun) {
        $cargo = Get-Command cargo -ErrorAction SilentlyContinue
        if ($cargo) {
            Write-Host 'Updating Cargo.lock...' -ForegroundColor DarkGray
            & cargo generate-lockfile
            if ($LASTEXITCODE -ne 0) { throw 'cargo generate-lockfile failed' }
        } else {
            Write-Warning 'cargo not on PATH — commit Cargo.lock manually after bump'
        }
    }
}

function Test-WorkspaceVersions {
    param([string]$Expected)
    $bad = @()
    Get-ChildItem -Path (Join-Path $root 'crates') -Recurse -Filter Cargo.toml | ForEach-Object {
        $v = (Select-String -Path $_.FullName -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
        if ($v -ne $Expected) { $bad += "$($_.Directory.Name)=$v" }
    }
    if ($bad.Count -gt 0) {
        throw @"
Workspace version mismatch - all crates must be $Expected before release:
  $($bad -join "`n  ")
Use -Bump patch|minor|major or edit Cargo.toml files manually.
"@
    }
}

function Sync-ReleaseSiteFiles {
    param([string]$ReleaseTag)
    Write-Step "Syncing site/public release files for $ReleaseTag"

    Write-Utf8NoBom -Path (Join-Path $root 'site\public\releases\latest.txt') -Content "$ReleaseTag`n"

    foreach ($name in @('install.ps1', 'install.sh')) {
        $src = Join-Path $root $name
        $dst = Join-Path $root "site\public\$name"
        if (-not (Test-Path $src)) { throw "Missing $src" }
        if ($DryRun) {
            Write-Plan "copy $src -> $dst"
        } else {
            Copy-Item -Force $src $dst
        }
    }
}

function Get-DirtyPaths {
    $lines = & $script:GitExe status --porcelain 2>$null
    if ($LASTEXITCODE -ne 0) { return @() }
    foreach ($line in $lines) {
        if ($line.Length -lt 4) { continue }
        $path = $line.Substring(3).Trim()
        if ($path -match ' -> ') { $path = ($path -split ' -> ')[0].Trim() }
        $path
    }
}

function Assert-WorkingTreeReady {
    param([string]$ReleaseTag)
    $dirty = @(Get-DirtyPaths)
    if ($dirty.Count -eq 0) { return }

    $allowed = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($p in $ReleaseSitePaths) { [void]$allowed.Add(($p -replace '/', '\')) }
    if ($Bump) {
        Get-ChildItem -Path (Join-Path $root 'crates') -Recurse -Filter Cargo.toml | ForEach-Object {
            $rel = $_.FullName.Substring($root.Length + 1)
            [void]$allowed.Add($rel)
        }
        [void]$allowed.Add('Cargo.lock')
        [void]$allowed.Add('crates\ax-web\web-ui\package.json')
    }

    $blocked = @()
    foreach ($path in $dirty) {
        $norm = $path -replace '/', '\'
        if (-not $allowed.Contains($norm)) { $blocked += $path }
    }

    if ($blocked.Count -gt 0 -and -not $AllowDirty) {
        if ($DryRun) {
            Write-Warning "Would block: $($blocked.Count) uncommitted path(s) outside release files (use -AllowDirty to override)"
            return
        }
        throw @"
Uncommitted changes outside release files - commit or stash first:
  $($blocked -join "`n  ")
Or pass -AllowDirty to proceed anyway (not recommended).
"@
    }
    if ($blocked.Count -gt 0) {
        Write-Warning "Proceeding with other uncommitted files (-AllowDirty): $($blocked.Count) path(s)"
    }
}

function Invoke-ReleaseCommit {
    param([string]$ReleaseTag)
    $toStage = @()
    foreach ($rel in $ReleaseSitePaths) {
        if (Test-Path (Join-Path $root $rel)) { $toStage += $rel }
    }
    if ($Bump) {
        Get-ChildItem -Path (Join-Path $root 'crates') -Recurse -Filter Cargo.toml | ForEach-Object {
            $toStage += $_.FullName.Substring($root.Length + 1) -replace '\\', '/'
        }
        if (Test-Path (Join-Path $root 'Cargo.lock')) { $toStage += 'Cargo.lock' }
        $pkg = 'crates/ax-web/web-ui/package.json'
        if (Test-Path (Join-Path $root $pkg)) { $toStage += $pkg }
    }

    $needsCommit = $false
    foreach ($rel in ($toStage | Select-Object -Unique)) {
        $status = & $script:GitExe status --porcelain -- $rel 2>$null
        if ($status) { $needsCommit = $true; break }
    }
    if (-not $needsCommit) {
        Write-Host 'Release files already committed.' -ForegroundColor DarkGray
        return
    }

    Write-Step "Committing release files"
    foreach ($rel in ($toStage | Select-Object -Unique)) {
        Invoke-Git add -- $rel
    }
    $msg = if ($Bump) { "Release $ReleaseTag" } else { "Publish $ReleaseTag release mirror to getax CDN" }
    Invoke-Git commit -m $msg
}

function Invoke-PushMain {
    param([string]$Branch)
    Invoke-Git fetch origin $Branch
    $ahead = & $script:GitExe rev-list --count "origin/$Branch..HEAD" 2>$null
    if ($LASTEXITCODE -ne 0) { $ahead = '1' }
    if ([int]$ahead -gt 0) {
        Write-Step "Pushing $Branch ($ahead commit(s))"
        Invoke-Git push origin $Branch
    } else {
        Write-Host "main is up to date on origin." -ForegroundColor DarkGray
    }
}

function Invoke-TagPush {
    param([string]$ReleaseTag)
    $localTag = & $script:GitExe tag -l $ReleaseTag 2>$null
    $remoteTag = & $script:GitExe ls-remote --tags origin "refs/tags/$ReleaseTag" 2>$null

    if ($remoteTag -and -not $Force) {
        $ans = Read-Host "Tag $ReleaseTag exists on origin. Move it to current HEAD? (y/N)"
        if ($ans -ne 'y') {
            Write-Host 'Aborted.'
            Pop-Location
            exit 0
        }
        $Force = $true
    }

    if ($localTag -or $remoteTag) {
        if ($Force) {
            Write-Step "Retagging $ReleaseTag on current HEAD"
            if ($localTag) { Invoke-Git tag -d $ReleaseTag }
            if ($remoteTag) { Invoke-Git push origin ":refs/tags/$ReleaseTag" }
        } else {
            Write-Step "Pushing existing local tag $ReleaseTag"
            Invoke-Git push origin $ReleaseTag
            return
        }
    } else {
        Write-Step "Creating tag $ReleaseTag"
    }

    Invoke-Git tag -a $ReleaseTag -m "Release $ReleaseTag - install.ps1 / install.sh all platforms"
    Invoke-Git push origin $ReleaseTag
}

function Wait-ReleaseWorkflow {
    param([string]$ReleaseTag)
    Write-Step "Waiting for GitHub Actions release workflow (max ${WaitTimeoutMinutes}m)"
    $deadline = (Get-Date).AddMinutes($WaitTimeoutMinutes)
    $runUrl = $null
    Start-Sleep -Seconds 10

    while ((Get-Date) -lt $deadline) {
        try {
            $resp = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/actions/workflows/release.yml/runs?per_page=5" -TimeoutSec 20
            $run = $resp.workflow_runs | Where-Object { $_.head_branch -eq $ReleaseTag } | Select-Object -First 1
            if ($run) {
                $runUrl = $run.html_url
                if ($run.status -eq 'completed') {
                    if ($run.conclusion -eq 'success') {
                        Write-Host "CI succeeded: $runUrl" -ForegroundColor Green
                        return
                    }
                    throw "Release workflow failed ($($run.conclusion)): $runUrl"
                }
                Write-Host "CI $($run.status)... $runUrl" -ForegroundColor DarkGray
            } else {
                Write-Host 'Waiting for workflow run to appear...' -ForegroundColor DarkGray
            }
        } catch {
            Write-Host "Poll error: $($_.Exception.Message)" -ForegroundColor DarkYellow
        }
        Start-Sleep -Seconds 15
    }

    if ($runUrl) { throw "Timeout waiting for release CI: $runUrl" }
    throw 'Timeout waiting for release CI to start'
}

function Test-GithubReleaseAssets {
    param([string]$ReleaseTag)
    Write-Step "Verifying GitHub release $ReleaseTag"
    $rel = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/tags/$ReleaseTag" -TimeoutSec 30
    if ($rel.assets.Count -lt 6) {
        throw "Release $ReleaseTag has $($rel.assets.Count)/6+ assets"
    }
    Write-Host "Release has $($rel.assets.Count) assets." -ForegroundColor Green

    $url = "https://github.com/$Repo/releases/download/$ReleaseTag/ax-win32-x64.zip"
    for ($i = 1; $i -le 12; $i++) {
        try {
            $resp = Invoke-WebRequest -Uri $url -Method Head -TimeoutSec 20 -UseBasicParsing
            if ($resp.StatusCode -eq 200) { break }
        } catch {
            if ($i -eq 12) { throw "Download not reachable: $url" }
            Start-Sleep -Seconds 5
        }
    }
}

function Test-ReleaseBinaryVersion {
    param([string]$ReleaseTag, [string]$ExpectedVersion)
    if (-not $IsWindows -and $env:OS -notmatch 'Windows') {
        Write-Host 'Skipping local binary version check (not on Windows).' -ForegroundColor DarkGray
        return
    }
    Write-Step "Verifying ax-win32-x64.zip reports version $ExpectedVersion"
    $tmp = Join-Path $env:TEMP ("ax-release-verify-" + [guid]::NewGuid().ToString())
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    try {
        $zip = Join-Path $tmp 'ax.zip'
        $url = "https://github.com/$Repo/releases/download/$ReleaseTag/ax-win32-x64.zip"
        Invoke-WebRequest -Uri $url -OutFile $zip -TimeoutSec 120 -UseBasicParsing
        Expand-Archive -Path $zip -DestinationPath $tmp -Force
        $exe = Get-ChildItem -Path $tmp -Filter ax.exe -Recurse | Select-Object -First 1
        if (-not $exe) { throw 'ax.exe not found in release zip' }
        $reported = (& $exe.FullName version 2>&1 | Out-String).Trim()
        if ($reported -notmatch [regex]::Escape($ExpectedVersion)) {
            throw "Binary reports '$reported' but tag is v$ExpectedVersion"
        }
        Write-Host "Binary version OK: $reported" -ForegroundColor Green
    } finally {
        Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
    }
}

try {
    $script:GitExe = Resolve-GitExe

    try { Invoke-Git remote get-url origin | Out-Null }
    catch { throw "No git remote 'origin'. Add: git remote add origin https://github.com/$Repo.git" }

    $branch = if ($DryRun) { 'main' } else { (& $script:GitExe rev-parse --abbrev-ref HEAD).Trim() }
    if ($branch -ne 'main') {
        Write-Warning "Not on main (on $branch). Release tags should be cut from main."
    }

    if ($Bump) {
        $current = Get-AxCargoVersion
        $next = Get-NextVersion -Current $current -Kind $Bump
        Set-WorkspaceVersion -Version $next
    }

    if (-not $Tag) {
        $Tag = 'v' + (Get-AxCargoVersion)
    } elseif ($Tag -notmatch '^v') {
        $Tag = "v$Tag"
    }

    $version = $Tag.TrimStart('v')
    Test-WorkspaceVersions -Expected $version

    Write-Step "Release plan: $Tag (Cargo.toml $version)"
    Sync-ReleaseSiteFiles -ReleaseTag $Tag
    Assert-WorkingTreeReady -ReleaseTag $Tag
    Invoke-ReleaseCommit -ReleaseTag $Tag
    Invoke-PushMain -Branch $branch
    Invoke-TagPush -ReleaseTag $Tag

    $actionsUrl = "https://github.com/$Repo/actions/workflows/release.yml"
    Write-Host ""
    Write-Host "Release workflow: $actionsUrl" -ForegroundColor Cyan

    if ($Wait -and -not $DryRun) {
        Wait-ReleaseWorkflow -ReleaseTag $Tag
        Test-GithubReleaseAssets -ReleaseTag $Tag
        Test-ReleaseBinaryVersion -ReleaseTag $Tag -ExpectedVersion $version
        Write-Host ""
        Write-Host 'Release verified. Install:' -ForegroundColor Green
    } else {
        Write-Host "When CI is green (~8-15 min), install:" -ForegroundColor Green
    }

    Write-Host '  Windows:         irm https://getax.wenneker.io/install.ps1 | iex'
    Write-Host '  macOS/Linux/WSL: curl -fsSL https://getax.wenneker.io/install.sh | sh'
    Write-Host "  Pin version:     `$env:AX_VERSION = '$Tag'; irm https://getax.wenneker.io/install.ps1 | iex"
}
finally {
    Pop-Location
}
