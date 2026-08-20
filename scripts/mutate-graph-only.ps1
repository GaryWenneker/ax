<#
.SYNOPSIS
  Manual mutation testing for the graph-only query path (audit findings C5, C6).

.DESCRIPTION
  No mutation tool is installed in this workspace (no cargo-mutants), so this is
  the manual procedure: introduce one plausible bug at a time and require the
  suite to catch it. A surviving mutant means the tests assert less than they
  appear to.

  Two rules this runner holds itself to, because a hand-rolled mutation runner
  can otherwise report kills it never performed:

    1. It proves it applied each mutant. The edit is verified to have changed the
       file before the tests run; if the anchor text is not found, that is a hard
       failure, never a skipped mutant.
    2. It proves it restored each file, byte for byte, before moving on.

  Fail-closed throughout: an unreadable file, a missing anchor, or a test command
  that cannot start is a failure of the run, not a pass.

.PARAMETER Only
  Run a single mutant by name (substring match), for debugging a survivor.

.PARAMETER PropertiesOnly
  Re-run the mutants that have a property-test target against those properties
  alone. A kill is credited to whichever test fails first, so a green full-suite
  mutation run says nothing about which layer did the catching; this narrows the
  target so the property tests have to earn their claim. Mutants with no property
  target are reported as N/A rather than silently passing.

.EXAMPLE
  .\scripts\mutate-graph-only.ps1

.EXAMPLE
  .\scripts\mutate-graph-only.ps1 -PropertiesOnly
#>
[CmdletBinding()]
param(
    [string]$Only,
    [switch]$PropertiesOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Continue'

$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot

# Skip the frontend bundle: these mutants are all in Rust and ax-web's build
# script shells out to npm.
$env:AX_SKIP_WEB_BUILD = '1'

# Each mutant: a real bug someone could plausibly write, the file it lives in,
# the exact text to replace, and the test target that must notice.
$mutants = @(
    @{
        Name   = 'classify: blank graph hash counts as fresh'
        File   = 'crates/ax-context/src/source_store.rs'
        From   = '.is_some_and(|h| !lookup.indexed_hash.is_empty() && h == lookup.indexed_hash);'
        To     = '.is_some_and(|h| h == lookup.indexed_hash);'
        Test   = @('test', '-p', 'ax-context', '--lib', 'source_store')
        Props  = @('test', '-p', 'ax-context', '--lib', 'source_store::tests::properties')
        Why    = 'A failed parse stores a blank hash; treating blank as a match would present unverified text as current.'
    },
    @{
        Name   = 'classify: every stored row is fresh'
        File   = 'crates/ax-context/src/source_store.rs'
        From   = '.is_some_and(|h| !lookup.indexed_hash.is_empty() && h == lookup.indexed_hash);'
        To     = '.is_some();'
        Test   = @('test', '-p', 'ax-context', '--lib', 'source_store')
        Props  = @('test', '-p', 'ax-context', '--lib', 'source_store::tests::properties')
        Why    = 'The core failure mode: stale source served without a stale label.'
    },
    @{
        Name   = 'line_bounds: end line not clamped to EOF'
        File   = 'crates/ax-context/src/source_store.rs'
        From   = '    let end = end.min(line_count);'
        To     = '    let end = end;'
        Test   = @('test', '-p', 'ax-context', '--lib', 'source_store')
        Props  = @('test', '-p', 'ax-context', '--lib', 'source_store::tests::properties')
        Why    = 'An end line past EOF must clamp, not silently yield an empty snippet.'
    },
    @{
        Name   = 'numbered_slice: line numbers off by one'
        File   = 'crates/ax-context/src/source_store.rs'
        From   = '.map(|(i, line)| format!("{}{}{}", start + i + 1, sep, line))'
        To     = '.map(|(i, line)| format!("{}{}{}", start + i, sep, line))'
        Test   = @('test', '-p', 'ax-context', '--lib', 'source_store')
        Props  = @('test', '-p', 'ax-context', '--lib', 'source_store::tests::properties')
        Why    = 'Wrong line numbers send an agent to the wrong place — worse than no snippet.'
    },
    @{
        Name   = 'store scope: keep build output again'
        File   = 'crates/ax-extraction/src/orchestrator.rs'
        From   = '            if self.is_extractable(&full_path, opts, &ext_map, &plugin_exts) {'
        To     = '            if true {'
        Test   = @('test', '-p', 'ax-extraction', '--test', 'source_store_write_path')
        Why    = 'The regression that put 90 MB of object-file text into ax.db.'
    },
    @{
        Name   = 'prune: delete claimed source instead of orphans'
        File   = 'crates/ax-db/src/queries.rs'
        From   = '"DELETE FROM file_contents WHERE path NOT IN (SELECT path FROM files)"'
        To     = '"DELETE FROM file_contents WHERE path IN (SELECT path FROM files)"'
        Test   = @('test', '-p', 'ax-extraction', '--test', 'source_store_write_path')
        Why    = 'An inverted prune would wipe exactly the source snippets need.'
    },
    @{
        Name   = 'coverage: count every file row again'
        File   = 'crates/ax-db/src/queries.rs'
        From   = @'
"SELECT COUNT(*) FROM files WHERE language != 'unknown' AND size <= ?",
'@
        To     = @'
"SELECT COUNT(*) FROM files WHERE ?1 = ?1",
'@
        Test   = @('test', '-p', 'ax-db', '--test', 'source_store_coverage')
        Why    = 'Counting assets and build output makes ax_status nag forever about a gap no re-index can close.'
    },
    @{
        Name   = 'coverage warning: measure against every file row'
        File   = 'crates/ax-core/src/stats_format.rs'
        From   = @'
    if stats.source_expected_files <= 0
        || stats.source_stored_files >= stats.source_expected_files
    {
'@
        To     = @'
    if stats.file_count <= 0 || stats.source_stored_files >= stats.file_count {
'@
        Test   = @('test', '-p', 'ax-core', '--lib', 'stats_format')
        Why    = 'The defect this fixed: a fully covered store reported as a permanent gap.'
    },
    @{
        Name   = 'guard: forbid-content ignores the rule globs again'
        File   = 'crates/ax-policy/src/guard.rs'
        From   = @'
                    if op == GuardOp::Write
                        && (rule.globs.is_empty() || any_glob_matches(&rule.globs, &rel))
                    {
'@
        To     = @'
                    if op == GuardOp::Write {
'@
        Test   = @('test', '-p', 'ax-policy', '--lib', 'guard')
        Why    = 'An unscoped ban blocks the indexer that must read files, so the guard gets switched off.'
    },
    @{
        Name   = 'guard: forbid-content needs globs to fire'
        File   = 'crates/ax-policy/src/guard.rs'
        From   = '&& (rule.globs.is_empty() || any_glob_matches(&rule.globs, &rel))'
        To     = '&& (!rule.globs.is_empty() && any_glob_matches(&rule.globs, &rel))'
        Test   = @('test', '-p', 'ax-policy', '--lib', 'guard')
        Why    = 'The opposite drift: the secrets rule carries no globs, so this would silence it everywhere.'
    },
    @{
        Name   = 'catalog: graph reads hidden again (pre-C6 behaviour)'
        File   = 'crates/ax-mcp/src/tool_filter.rs'
        # Anchored on is_core_tool's body rather than a CORE_TOOLS entry, because
        # tool names appear in both CORE_TOOLS and POLICY_REFERENCED_TOOLS and a
        # plain text replace would hit both, mutating two things at once.
        From   = @'
pub fn is_core_tool(name: &str) -> bool {
    CORE_TOOLS.contains(&name)
'@
        To     = @'
pub fn is_core_tool(name: &str) -> bool {
    CORE_TOOLS.contains(&name) && !name.starts_with("ax_se")
'@
        Test   = @('test', '-p', 'ax-mcp', '--lib', 'tool_filter')
        Why    = 'Finding C6 itself: a graph tool the rules mandate, missing from the catalog.'
    },
    @{
        Name   = 'catalog: heavy ops advertised by default'
        File   = 'crates/ax-mcp/src/tool_filter.rs'
        From   = @'
pub fn is_core_tool(name: &str) -> bool {
    CORE_TOOLS.contains(&name)
'@
        To     = @'
pub fn is_core_tool(name: &str) -> bool {
    true
'@
        Test   = @('test', '-p', 'ax-mcp', '--lib', 'tool_filter')
        Why    = 'The opposite drift: the lean filter stops gating anything and every turn pays for the full menu.'
    }
)

if ($Only) {
    $mutants = @($mutants | Where-Object { $_.Name -like "*$Only*" })
    if ($mutants.Count -eq 0) {
        Write-Host "No mutant matches '$Only'" -ForegroundColor Red
        Pop-Location
        exit 1
    }
}

$results = [System.Collections.Generic.List[object]]::new()

foreach ($m in $mutants) {
    Write-Host ""
    Write-Host "=== MUTANT: $($m.Name) ===" -ForegroundColor Cyan
    Write-Host "    $($m.Why)" -ForegroundColor DarkGray

    $testArgs = $m.Test
    if ($PropertiesOnly) {
        if (-not $m.ContainsKey('Props')) {
            Write-Host "    no property target — not a claim the properties make" -ForegroundColor DarkGray
            $results.Add([pscustomobject]@{
                Mutant = $m.Name; Status = 'N/A'; Detail = 'no property-test target for this mutant'
            })
            continue
        }
        $testArgs = $m.Props
    }

    $path = Join-Path $repoRoot $m.File
    if (-not (Test-Path $path)) {
        $results.Add([pscustomobject]@{ Mutant = $m.Name; Status = 'ERROR'; Detail = "missing file: $($m.File)" })
        continue
    }

    $originalBytes = [System.IO.File]::ReadAllBytes($path)
    # Match on LF-normalized text so multi-line anchors work regardless of the
    # file's line endings. The restore below writes the original bytes back, so
    # normalizing here cannot leak into the working tree.
    $originalText = [System.Text.Encoding]::UTF8.GetString($originalBytes).Replace("`r`n", "`n")
    $m.From = $m.From.Replace("`r`n", "`n")
    $m.To = $m.To.Replace("`r`n", "`n")

    if (-not $originalText.Contains($m.From)) {
        # Fail closed: an anchor that no longer matches means this mutant was
        # never applied, so any pass below would be meaningless.
        $results.Add([pscustomobject]@{
            Mutant = $m.Name; Status = 'ERROR'
            Detail = 'anchor text not found — mutant not applied (update the script)'
        })
        continue
    }

    $mutatedText = $originalText.Replace($m.From, $m.To)
    if ($mutatedText -eq $originalText) {
        $results.Add([pscustomobject]@{ Mutant = $m.Name; Status = 'ERROR'; Detail = 'replacement was a no-op' })
        continue
    }

    $applied = $false
    $exit = $null
    try {
        [System.IO.File]::WriteAllBytes($path, [System.Text.Encoding]::UTF8.GetBytes($mutatedText))
        # Proof of application: the file on disk really contains the bug now, and
        # really differs from what we read. (Not "no longer contains the anchor" —
        # a mutant that appends to the anchor still contains it.)
        $onDisk = [System.Text.Encoding]::UTF8.GetString([System.IO.File]::ReadAllBytes($path)).Replace("`r`n", "`n")
        $applied = $onDisk.Contains($m.To) -and $onDisk -ne $originalText
        if (-not $applied) {
            throw 'mutant did not survive the write — refusing to report a kill'
        }
        Write-Host "    applied to $($m.File)" -ForegroundColor Yellow
        Write-Host "> cargo $($testArgs -join ' ')" -ForegroundColor DarkGray
        & cargo @($testArgs) 2>&1 | Out-Null
        $exit = $LASTEXITCODE
    }
    catch {
        $results.Add([pscustomobject]@{ Mutant = $m.Name; Status = 'ERROR'; Detail = $_.Exception.Message })
    }
    finally {
        [System.IO.File]::WriteAllBytes($path, $originalBytes)
    }

    $restored = [System.IO.File]::ReadAllBytes($path)
    $restoredOk = -not (Compare-Object $originalBytes $restored -SyncWindow 0)

    if (-not $restoredOk) {
        $results.Add([pscustomobject]@{
            Mutant = $m.Name; Status = 'ERROR'
            Detail = "COULD NOT RESTORE $($m.File) — fix before trusting anything else"
        })
        continue
    }
    if (-not $applied) { continue }

    if ($null -eq $exit) {
        $results.Add([pscustomobject]@{ Mutant = $m.Name; Status = 'ERROR'; Detail = 'test command never ran' })
    }
    elseif ($exit -eq 0) {
        Write-Host "    SURVIVED — the tests did not notice this bug" -ForegroundColor Red
        $results.Add([pscustomobject]@{ Mutant = $m.Name; Status = 'SURVIVED'; Detail = 'tests passed with the bug present' })
    }
    else {
        Write-Host "    killed (exit $exit), file restored" -ForegroundColor Green
        $results.Add([pscustomobject]@{ Mutant = $m.Name; Status = 'KILLED'; Detail = "tests failed as required (exit $exit)" })
    }
}

Write-Host ""
Write-Host "===== Mutation summary =====" -ForegroundColor Cyan
$results | Format-Table -AutoSize -Wrap | Out-String -Width 160 | Write-Host

$killed = @($results | Where-Object { $_.Status -eq 'KILLED' }).Count
# N/A means the mutant is outside what this target claims to catch, which is a
# real answer; anything else that is not a kill is a failure.
$attempted = @($results | Where-Object { $_.Status -ne 'N/A' }).Count
$bad = @($results | Where-Object { $_.Status -notin @('KILLED', 'N/A') })

Pop-Location

# One line on the success stream (not the host) so a caller can capture the
# score; everything above is Write-Host for a human and is invisible to $( ).
Write-Output "$killed/$attempted mutants killed"

if ($bad.Count -gt 0) {
    Write-Host "MUTATION FAILED — $($bad.Count) mutant(s) survived or errored" -ForegroundColor Red
    exit 1
}
Write-Host "MUTATION PASSED — every mutant was applied, caught, and reverted" -ForegroundColor Green
exit 0
