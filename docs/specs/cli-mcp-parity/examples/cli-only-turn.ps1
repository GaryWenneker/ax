# AX CLI-only research turn (Windows PowerShell) — no MCP, plugins, hooks, or listeners.
# UTF-8 without BOM.

$ErrorActionPreference = "Stop"
$utf8 = New-Object System.Text.UTF8Encoding $false

function Invoke-AxStdin {
  param(
    [Parameter(Mandatory = $true)][string[]]$Args,
    [Parameter(Mandatory = $true)][string]$Body
  )
  $tmp = [System.IO.Path]::GetTempFileName()
  try {
    [System.IO.File]::WriteAllText($tmp, $Body, $utf8)
    Get-Content -Raw -Encoding utf8 $tmp | & ax @Args
    if ($LASTEXITCODE -ne 0) { throw "ax $($Args -join ' ') failed with $LASTEXITCODE" }
  }
  finally {
    Remove-Item -Force $tmp -ErrorAction SilentlyContinue
  }
}

$sessionJson = & ax session start --json | ConvertFrom-Json
$session = $sessionJson.session
Write-Host "session=$session"

$begin = Invoke-AxStdin -Args @("turn","begin","--session",$session,"--prompt-stdin","--json") -Body "Investigate auth failure after refresh" | ConvertFrom-Json
$turn = $begin.turnId
Write-Host "turn=$turn"

Invoke-AxStdin -Args @("turn","event","--session",$session,"--turn",$turn,"--input-stdin") -Body '{"kind":"finding","body":"SameSite cookie cleared on soft reload"}' | Out-Null

$finish = Invoke-AxStdin -Args @("turn","finish","--session",$session,"--turn",$turn,"--outcome-stdin","--json") -Body '{"outcome":"Root cause: SameSite mismatch. No code change this turn."}' | ConvertFrom-Json
Write-Host ("memory={0} idempotent={1}" -f $finish.memory_id, $finish.idempotent)

& ax doctor --mode cli --json
