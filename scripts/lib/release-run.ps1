# Helpers for scripts/release-tag.ps1.

# Converts a `gh --json` timestamp to UTC. Windows PowerShell 5.1 hands over the
# raw `...Z` string and PowerShell 7 a DateTime, so both are accepted. A string
# without an offset is read as UTC, which is what GitHub returns.
function ConvertTo-AxUtc {
    param([Parameter(Mandatory)][object]$Value)
    if ($Value -is [datetime]) {
        if ($Value.Kind -eq [DateTimeKind]::Unspecified) { return [datetime]::SpecifyKind($Value, [DateTimeKind]::Utc) }
        return $Value.ToUniversalTime()
    }
    [datetime]::Parse([string]$Value, [Globalization.CultureInfo]::InvariantCulture,
        [Globalization.DateTimeStyles]'AdjustToUniversal,AssumeUniversal')
}

# Picks the first release run for HeadSha created at or after RequestedAt,
# comparing both sides in UTC.
function Select-AxReleaseRun {
    param([object[]]$Runs, [string]$HeadSha, [datetime]$RequestedAt)
    $since = ConvertTo-AxUtc $RequestedAt
    $Runs | Where-Object {
        $_.headSha -eq $HeadSha -and (ConvertTo-AxUtc $_.createdAt) -ge $since
    } | Select-Object -First 1
}
