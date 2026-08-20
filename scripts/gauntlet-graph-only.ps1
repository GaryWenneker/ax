<#
.SYNOPSIS
  One-command gauntlet for the graph-only query path (audit findings C5 and C6).

.DESCRIPTION
  Reruns every layer behind docs/audits/2026-08-19-preflight-graph-only/EVIDENCE.md
  so the report is reproducible from the repo alone. Layers:

    1. Workspace test suite (zero failures outside the recorded baseline)
    2. Graph-only gate (fail-closed source check on the query path)
    3. MCP catalog coherence (policy-named tools classified; core tools exist)
    4. Source store: schema v17 migration, snippet resolution, what the store is
       allowed to hold, status warnings
    5. Catalog payload size (the token cost of the wider default catalog)
    6. Clippy on the touched crates (zero findings outside the recorded baseline)
    7. Negative control: a real disk read must turn the gate red
    8. Mutation: 8 plausible bugs, each must be caught (scripts/mutate-graph-only.ps1)
    9. Mutation vs the property tests alone, so their kills are their own

  Every layer's exit code is checked. A crashed layer is a failure, never a pass.

  Layers 1 and 6 hold the line at zero NEW findings rather than zero findings:
  this repo carries pre-existing test flakes and clippy lints that predate the
  graph-only work, and silently "improving" them would be scope creep. Both
  baselines are listed literally below, so a reviewer can see exactly what is
  being tolerated and a new finding of the same kind in the same file still
  fails the layer (the counts are compared, not just the names).

.PARAMETER SkipSuite
  Skip layer 1 (the full workspace suite) for a fast inner loop. The summary
  records it as skipped so a partial run cannot be mistaken for a full one.

.PARAMETER SkipNegativeControl
  Skip layer 7. It temporarily appends a disk read to a query-path file and
  restores the original bytes afterwards.

.PARAMETER SkipMutation
  Skip layers 8 and 9 (several minutes). Both edit a source file, run tests, and
  restore the original bytes.

.EXAMPLE
  .\scripts\gauntlet-graph-only.ps1
#>
[CmdletBinding()]
param(
    [switch]$SkipSuite,
    [switch]$SkipNegativeControl,
    [switch]$SkipMutation
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Continue'

$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot

$results = [System.Collections.Generic.List[object]]::new()

# Crates whose lints this change is answerable for.
$ClippyCrates = @('ax-context', 'ax-db', 'ax-core', 'ax-mcp', 'ax-extraction', 'ax-cli')
$ClippyPackages = @($ClippyCrates | ForEach-Object { '-p', $_ })

# Findings outside these crates are dropped. Compiling the crates above also
# compiles workspace dependencies (ax-lsp, ax-share, ...), and rustc replays
# their warnings only for units cargo actually rebuilds — so counting them would
# make this layer depend on the build cache instead of on the code.
$ClippyScope = '^crates\\(' + ($ClippyCrates -join '|') + ')\\'

# Pre-existing test failures. Recorded 2026-08-19 on the graph-only branch.
#   pricing_sync::tests::upsert_and_history - order-dependent: it shares the
#   AX_USAGE_DB env var and the cached pool with its neighbours, so it fails in
#   a parallel --workspace run and passes when run alone. ax-usage is untouched
#   by this change.
$SuiteBaseline = @('pricing_sync::tests::upsert_and_history')

# Pre-existing lint findings, as "<file> [<lint>]" = count. All sit on lines
# outside this change's diff; several are in files it edited, which is why the
# baseline is per-file-per-lint counts and not a file allowlist.
$ClippyBaseline = @{
    'crates\ax-context\src\directory.rs [clippy::invalid_regex]'                = 1
    'crates\ax-db\src\queries.rs [clippy::len_zero]'                            = 1
    'crates\ax-extraction\src\languages\common.rs [clippy::only_used_in_recursion]' = 1
    'crates\ax-extraction\src\languages\csharp.rs [unused_imports]'             = 1
    'crates\ax-extraction\src\languages\refs.rs [clippy::manual_map]'           = 1
    'crates\ax-extraction\src\languages\refs.rs [clippy::too_many_arguments]'   = 7
    'crates\ax-extraction\src\orchestrator.rs [clippy::derivable_impls]'        = 1
    'crates\ax-extraction\src\orchestrator.rs [clippy::type_complexity]'        = 1
    'crates\ax-extraction\src\parse_pool.rs [clippy::new_without_default]'      = 1
    'crates\ax-extraction\src\test_mapper.rs [clippy::double_ended_iterator_last]' = 1
}

function Add-Result {
    param(
        [Parameter(Mandatory)][string]$Layer,
        [Parameter(Mandatory)][string]$Status,
        [Parameter(Mandatory)][string]$Detail
    )
    $results.Add([pscustomobject]@{ Layer = $Layer; Status = $Status; Detail = $Detail })
}

# Run one cargo invocation. Returns exit code; prints everything it produced.
function Invoke-Cargo {
    param([Parameter(Mandatory)][string[]]$CargoArgs)
    Write-Host "> cargo $($CargoArgs -join ' ')" -ForegroundColor DarkGray
    $out = & cargo @CargoArgs 2>&1
    $code = $LASTEXITCODE
    $out | ForEach-Object { Write-Host $_ }
    return @{ Code = $code; Output = $out }
}

# A layer is a list of cargo invocations; all must succeed.
function Invoke-Layer {
    param(
        [Parameter(Mandatory)][string]$Layer,
        [Parameter(Mandatory)][object[]]$Invocations
    )
    Write-Host ""
    Write-Host "=== $Layer ===" -ForegroundColor Cyan

    $summaries = @()
    foreach ($inv in $Invocations) {
        $r = Invoke-Cargo -CargoArgs $inv
        $summaries += ($r.Output | Select-String -Pattern 'test result:|catalog:' |
            ForEach-Object { $_.Line.Trim() })
        if ($r.Code -ne 0) {
            Add-Result -Layer $Layer -Status 'FAIL' -Detail "exit $($r.Code) on: cargo $($inv -join ' ')"
            return $false
        }
    }
    $detail = ($summaries -join ' | ')
    if (-not $detail) { $detail = 'ok' }
    Add-Result -Layer $Layer -Status 'PASS' -Detail $detail
    return $true
}

# Layer 1. Fails on any test failure that is not in $SuiteBaseline, and on a
# nonzero exit we cannot explain by test failures (a compile error must not pass).
function Invoke-SuiteLayer {
    $layer = '1. Workspace suite'
    Write-Host ""
    Write-Host "=== $layer ===" -ForegroundColor Cyan

    # ax-web's build script shells out to npm. When npm is not resolvable from
    # cmd.exe the whole workspace fails to build for a reason unrelated to the
    # Rust code under test, so skip the frontend bundle and say so out loud.
    $webNote = ''
    & cmd /c "npm --version" *> $null
    if ($LASTEXITCODE -ne 0) {
        $env:AX_SKIP_WEB_BUILD = '1'
        $webNote = ' [AX_SKIP_WEB_BUILD=1: npm not resolvable from cmd, web-ui bundle not rebuilt]'
        Write-Host "npm not resolvable from cmd.exe - setting AX_SKIP_WEB_BUILD=1" -ForegroundColor Yellow
    }

    $r = Invoke-Cargo -CargoArgs @('test', '--workspace')

    $failedTests = @($r.Output |
        Select-String -Pattern '^test (.+) \.\.\. FAILED$' |
        ForEach-Object { $_.Matches[0].Groups[1].Value.Trim() } |
        Sort-Object -Unique)
    $passLines = @($r.Output | Select-String -Pattern '^test result:' |
        ForEach-Object { $_.Line.Trim() })

    if ($r.Code -ne 0 -and $failedTests.Count -eq 0) {
        Add-Result -Layer $layer -Status 'FAIL' `
            -Detail "exit $($r.Code) with no test failures parsed - build or harness error$webNote"
        return
    }
    if ($passLines.Count -eq 0) {
        Add-Result -Layer $layer -Status 'FAIL' `
            -Detail "no 'test result:' lines - the suite never ran$webNote"
        return
    }

    $new = @($failedTests | Where-Object { $SuiteBaseline -notcontains $_ })
    if ($new.Count -gt 0) {
        Write-Host "NEW test failures (not in baseline):" -ForegroundColor Red
        $new | ForEach-Object { Write-Host "  $_" -ForegroundColor Red }
        Add-Result -Layer $layer -Status 'FAIL' `
            -Detail "$($new.Count) new failure(s): $($new -join ', ')$webNote"
        return
    }

    $suites = $passLines.Count
    $detail = "$suites test binaries, 0 new failures"
    if ($failedTests.Count -gt 0) {
        $detail += "; $($failedTests.Count) baseline failure(s): $($failedTests -join ', ')"
    }
    Add-Result -Layer $layer -Status 'PASS' -Detail ($detail + $webNote)
}

# Layer 6. Clippy in JSON mode so findings can be attributed per file and lint.
# The exit code alone is useless here: deny-by-default lints make it nonzero on
# a clean run, so the verdict comes from the parsed findings instead.
function Invoke-ClippyLayer {
    $layer = '6. Clippy'
    Write-Host ""
    Write-Host "=== $layer ===" -ForegroundColor Cyan

    $cargoArgs = @('clippy') + $ClippyPackages +
        @('--all-targets', '--no-deps', '--message-format=json')
    Write-Host "> cargo $($cargoArgs -join ' ')" -ForegroundColor DarkGray

    $raw = & cargo @cargoArgs 2>&1
    $msgs = @($raw | Where-Object { $_ -is [string] -and $_.StartsWith('{') } |
        ForEach-Object { try { $_ | ConvertFrom-Json } catch { } } |
        Where-Object { $_ })

    if ($msgs.Count -eq 0) {
        $raw | ForEach-Object { Write-Host $_ }
        Add-Result -Layer $layer -Status 'FAIL' -Detail 'clippy produced no JSON - it never ran'
        return
    }
    if (-not ($msgs | Where-Object { $_.reason -eq 'build-finished' })) {
        $raw | ForEach-Object { Write-Host $_ }
        Add-Result -Layer $layer -Status 'FAIL' -Detail 'no build-finished record - clippy did not complete'
        return
    }

    $diags = @($msgs | Where-Object { $_.reason -eq 'compiler-message' })

    # A rustc error (missing code, or an E#### code) is a compile failure, not a
    # lint, and can never be baselined away.
    $hard = @($diags | Where-Object {
        $_.message.level -eq 'error' -and
        -not ($_.message.code.code -like 'clippy::*') -and
        (-not $_.message.code.code -or $_.message.code.code -match '^E\d+$')
    })
    if ($hard.Count -gt 0) {
        $hard | ForEach-Object { Write-Host $_.message.rendered -ForegroundColor Red }
        Add-Result -Layer $layer -Status 'FAIL' -Detail "$($hard.Count) compile error(s)"
        return
    }

    # Unique file:line:lint, so the same finding reported for lib and lib-test
    # counts once.
    $sigs = @($diags | Where-Object { $_.message.code.code } | ForEach-Object {
        $span = $_.message.spans | Where-Object { $_.is_primary } | Select-Object -First 1
        if ($span -and $span.file_name -match $ClippyScope) {
            [pscustomobject]@{
                Key  = "$($span.file_name) [$($_.message.code.code)]"
                Uniq = "$($span.file_name):$($span.line_start) [$($_.message.code.code)]"
                Text = $_.message.rendered
            }
        }
    } | Group-Object Uniq | ForEach-Object { $_.Group[0] })

    $groups = @($sigs | Group-Object Key)
    $newFindings = @()
    foreach ($g in $groups) {
        $allowed = if ($ClippyBaseline.ContainsKey($g.Name)) { $ClippyBaseline[$g.Name] } else { 0 }
        if ($g.Count -gt $allowed) {
            $newFindings += "$($g.Name): $($g.Count) found, $allowed baselined"
            $g.Group | Select-Object -Skip $allowed | ForEach-Object {
                Write-Host $_.Text -ForegroundColor Red
            }
        }
    }

    if ($newFindings.Count -gt 0) {
        Add-Result -Layer $layer -Status 'FAIL' -Detail ($newFindings -join ' | ')
        return
    }

    $total = ($sigs | Measure-Object).Count
    Write-Host "$total finding(s), all within the recorded baseline" -ForegroundColor Green
    Add-Result -Layer $layer -Status 'PASS' `
        -Detail "0 new findings ($total pre-existing, all baselined)"
}

$gateArgs = @('test', '-p', 'ax-context', '--test', 'no_query_time_disk_reads')
$probeFile = Join-Path $repoRoot 'crates/ax-context/src/explore.rs'

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host "cargo not found on PATH — cannot run the gauntlet." -ForegroundColor Red
    Pop-Location
    exit 1
}

if ($SkipSuite) {
    Add-Result -Layer '1. Workspace suite' -Status 'SKIPPED' -Detail '-SkipSuite requested'
}
else {
    Invoke-SuiteLayer
}

Invoke-Layer '2. Graph-only gate' @(
    $gateArgs,
    # The policy half of the same guarantee: ax_guard must refuse a query-path
    # disk read, and must stay off the indexer that has to read files.
    @('test', '-p', 'ax-policy', '--lib', 'guard')
) | Out-Null

Invoke-Layer '3. MCP catalog coherence' @(
    @('test', '-p', 'ax-mcp', '--lib', 'tool_filter'),
    @('test', '-p', 'ax-mcp', '--test', 'new_tools_smoke')
) | Out-Null

Invoke-Layer '4. Source store' @(
    @('test', '-p', 'ax-db', '--test', 'migration_v17'),
    @('test', '-p', 'ax-db', '--test', 'source_store_coverage'),
    @('test', '-p', 'ax-context', '--lib', 'source_store'),
    @('test', '-p', 'ax-extraction', '--test', 'source_store_write_path'),
    @('test', '-p', 'ax-core', '--lib', 'stats_format')
) | Out-Null

Invoke-Layer '5. Catalog payload size' @(
    , @('test', '-p', 'ax-mcp', '--test', 'catalog_payload_size', '--', '--nocapture')
) | Out-Null

Invoke-ClippyLayer

# Layer 7: prove the gate can fail. Without this, a green gate might be green
# because it checks nothing (see EVIDENCE for the vacuity proof).
# Restores the file from an in-memory copy, so it is safe on a dirty tree.
if ($SkipNegativeControl) {
    Add-Result -Layer '7. Negative control' -Status 'SKIPPED' -Detail '-SkipNegativeControl requested'
}
else {
    Write-Host ""
    Write-Host "=== 7. Negative control ===" -ForegroundColor Cyan

    $original = [System.IO.File]::ReadAllBytes($probeFile)
    $probe = "`nfn gauntlet_negative_control_probe(p: &std::path::Path) -> String {`n    std::fs::read_to_string(p).unwrap_or_default()`n}`n"
    $probeBytes = [System.Text.Encoding]::UTF8.GetBytes($probe)

    try {
        [System.IO.File]::WriteAllBytes($probeFile, $original + $probeBytes)
        $r = Invoke-Cargo -CargoArgs $gateArgs
        $probeCode = $r.Code
    }
    finally {
        [System.IO.File]::WriteAllBytes($probeFile, $original)
    }

    $restored = [System.IO.File]::ReadAllBytes($probeFile)
    if (Compare-Object $original $restored -SyncWindow 0) {
        Add-Result -Layer '7. Negative control' -Status 'FAIL' -Detail "could not restore $probeFile"
    }
    elseif ($probeCode -eq 0) {
        Add-Result -Layer '7. Negative control' -Status 'FAIL' -Detail 'gate PASSED with a real disk read present — the gate is not enforcing'
    }
    else {
        Write-Host "gate correctly failed on the injected disk read (exit $probeCode)" -ForegroundColor Green
        Add-Result -Layer '7. Negative control' -Status 'PASS' -Detail "gate failed as required (exit $probeCode); file restored byte-for-byte"
    }
}

# Layers 8-9: mutation testing. No mutation tool is installed, so this is the
# manual procedure — see the runner for how it proves it applied each mutant.
if ($SkipMutation) {
    Add-Result -Layer '8. Mutation (full)' -Status 'SKIPPED' -Detail '-SkipMutation requested'
    Add-Result -Layer '9. Mutation (properties only)' -Status 'SKIPPED' -Detail '-SkipMutation requested'
}
else {
    $mutateScript = Join-Path $PSScriptRoot 'mutate-graph-only.ps1'
    foreach ($run in @(
            @{ Layer = '8. Mutation (full)'; PropertiesOnly = $false },
            @{ Layer = '9. Mutation (properties only)'; PropertiesOnly = $true }
        )) {
        Write-Host ""
        Write-Host "=== $($run.Layer) ===" -ForegroundColor Cyan
        if (-not (Test-Path $mutateScript)) {
            Add-Result -Layer $run.Layer -Status 'FAIL' -Detail "missing $mutateScript"
            continue
        }
        # Splat a hashtable: passing an empty array positionally binds to -Only.
        $mutParams = @{}
        if ($run.PropertiesOnly) { $mutParams['PropertiesOnly'] = $true }
        $out = & $mutateScript @mutParams 2>&1
        $code = $LASTEXITCODE
        $out | ForEach-Object { Write-Host $_ }
        $scoreMatch = @($out | Select-String -Pattern '^\d+/\d+ mutants killed')
        $score = if ($scoreMatch.Count -gt 0) { $scoreMatch[-1].Line.Trim() } else { $null }
        if ($code -ne 0) {
            Add-Result -Layer $run.Layer -Status 'FAIL' -Detail "$score (exit $code)".Trim()
        }
        elseif (-not $score) {
            Add-Result -Layer $run.Layer -Status 'FAIL' -Detail 'no mutation score reported'
        }
        else {
            Add-Result -Layer $run.Layer -Status 'PASS' -Detail $score
        }
    }
}

Write-Host ""
Write-Host "===== Gauntlet summary =====" -ForegroundColor Cyan
$results | Format-Table -AutoSize -Wrap | Out-String -Width 200 | Write-Host

$failed = @($results | Where-Object { $_.Status -eq 'FAIL' })
$skipped = @($results | Where-Object { $_.Status -eq 'SKIPPED' })

Pop-Location

if ($failed.Count -gt 0) {
    Write-Host "GAUNTLET FAILED ($($failed.Count) layer(s))" -ForegroundColor Red
    exit 1
}
if ($skipped.Count -gt 0) {
    Write-Host "GAUNTLET PASSED with $($skipped.Count) skipped layer(s) - not a full run" -ForegroundColor Yellow
    exit 0
}
Write-Host "GAUNTLET PASSED (all layers)" -ForegroundColor Green
exit 0
