<#
.SYNOPSIS
  Clippy finding filter for scripts/gauntlet-graph-only.ps1 (layer 6).

.DESCRIPTION
  Dot-source this file. Get-ClippyVerdict takes the compiler-message records
  from `cargo clippy --message-format=json`, keeps the findings in the given
  crates, and compares them with a baseline of "<file> [<lint>]" = count.

  File paths are compared with forward slashes on every platform: clippy
  reports `crates/ax-db/src/x.rs` on macOS and Linux and `crates\ax-db\src\x.rs`
  on Windows, so both the span paths and the baseline keys are normalized.
#>

function ConvertTo-ForwardSlash {
    param([AllowEmptyString()][string]$Path)
    return ($Path -replace '\\', '/')
}

function Get-ClippyVerdict {
    param(
        [AllowEmptyCollection()][object[]]$Diagnostics = @(),
        [Parameter(Mandatory)][string[]]$Crates,
        [Parameter(Mandatory)][hashtable]$Baseline
    )
    $scope = '^crates/(' + (($Crates | ForEach-Object { [regex]::Escape($_) }) -join '|') + ')/'
    $mentions = '(^|[\\/])crates[\\/](' + (($Crates | ForEach-Object { [regex]::Escape($_) }) -join '|') + ')[\\/]'

    $base = @{}
    foreach ($k in $Baseline.Keys) { $base[(ConvertTo-ForwardSlash $k)] = $Baseline[$k] }

    $inCrates = 0
    $sigs = @($Diagnostics | Where-Object { $_.message.code -and $_.message.code.code } | ForEach-Object {
        $span = $_.message.spans | Where-Object { $_.is_primary } | Select-Object -First 1
        if (-not $span) { return }
        if ($span.file_name -match $mentions) { $inCrates++ }
        $file = ConvertTo-ForwardSlash $span.file_name
        if ($file -match $scope) {
            [pscustomobject]@{
                Key  = "$file [$($_.message.code.code)]"
                Uniq = "${file}:$($span.line_start) [$($_.message.code.code)]"
                Text = $_.message.rendered
            }
        }
    } | Group-Object Uniq | ForEach-Object { $_.Group[0] })

    # Guard against a scope filter that silently drops everything: findings that
    # name one of the crates must all survive the filter.
    if ($inCrates -gt 0 -and $sigs.Count -eq 0) {
        return [pscustomobject]@{
            Error = "$inCrates finding(s) in the scoped crates, but none matched the scope filter"
            New = @(); Total = 0; Texts = @()
        }
    }

    $new = @()
    $texts = @()
    foreach ($g in @($sigs | Group-Object Key)) {
        $allowed = if ($base.ContainsKey($g.Name)) { $base[$g.Name] } else { 0 }
        if ($g.Count -gt $allowed) {
            $new += "$($g.Name): $($g.Count) found, $allowed baselined"
            $texts += @($g.Group | Select-Object -Skip $allowed | ForEach-Object { $_.Text })
        }
    }
    return [pscustomobject]@{ Error = $null; New = $new; Total = $sigs.Count; Texts = $texts }
}
