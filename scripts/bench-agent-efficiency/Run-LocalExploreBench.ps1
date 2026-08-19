#Requires -Version 5.1
<#
.SYNOPSIS
  Local efficiency harness: time `ax explore` on fixed queries (graph-backed).

.DESCRIPTION
  Does not drive Claude Code headless (needs API keys). Measures ax-side latency
  and response size for the WITH-graph arm — useful for regression and as input
  to a later WITH/WITHOUT agent comparison.

.EXAMPLE
  .\scripts\bench-agent-efficiency\Run-LocalExploreBench.ps1
  .\scripts\bench-agent-efficiency\Run-LocalExploreBench.ps1 -Runs 5 -Out results.md
#>
param(
    [string]$ProjectPath = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path,
    [int]$Runs = 3,
    [string]$Out = ""
)

$ErrorActionPreference = "Stop"
$queries = @(
    "how does ax_explore work",
    "who calls get_pending_files",
    "how does MCP tool listing filter tools"
)

Write-Host "Project: $ProjectPath"
Write-Host "Runs per query: $Runs"
Write-Host ""

$rows = @()
foreach ($q in $queries) {
    $times = @()
    $chars = @()
    for ($i = 1; $i -le $Runs; $i++) {
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        $text = & ax explore $q 2>$null | Out-String
        $sw.Stop()
        $times += $sw.Elapsed.TotalMilliseconds
        $chars += $text.Length
        Write-Host ("  [{0}/{1}] {2:N0} ms  {3:N0} chars  {4}" -f $i, $Runs, $sw.Elapsed.TotalMilliseconds, $text.Length, $q)
    }
    $sorted = $times | Sort-Object
    $median = $sorted[[int]([math]::Floor(($sorted.Count - 1) / 2))]
    $avgChars = ($chars | Measure-Object -Average).Average
    $rows += [pscustomobject]@{
        Query = $q
        MedianMs = [math]::Round($median, 1)
        AvgChars = [math]::Round($avgChars, 0)
        Runs = $Runs
    }
}

Write-Host ""
Write-Host "| Query | Median ms | Avg response chars | Runs |"
Write-Host "|-------|-----------|--------------------|------|"
foreach ($r in $rows) {
    Write-Host ("| {0} | {1} | {2} | {3} |" -f $r.Query, $r.MedianMs, $r.AvgChars, $r.Runs)
}

if ($Out) {
    $md = @("# ax explore local bench", "", "Project: ``$ProjectPath``", "", "| Query | Median ms | Avg response chars | Runs |", "|-------|-----------|--------------------|------|")
    foreach ($r in $rows) {
        $md += "| $($r.Query) | $($r.MedianMs) | $($r.AvgChars) | $($r.Runs) |"
    }
    $md += "", "_WITH-graph arm only. Pair with a headless agent WITHOUT ax for full competitive table._"
    Set-Content -Path $Out -Value ($md -join "`n") -Encoding UTF8
    Write-Host ""
    Write-Host "Wrote $Out"
}
