#!/usr/bin/env python3
"""ensure-cloudflare-tunnel.ps1 runs as Administrator, so it trusts no unverified download.

The functions are loaded from the script's AST into pwsh, then run with test
doubles for the network, signature and admin checks defined after them.
"""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "ensure-cloudflare-tunnel.ps1"

LOAD = r"""
$ErrorActionPreference = 'Stop'
$ast = [System.Management.Automation.Language.Parser]::ParseFile($env:TUNNEL_SCRIPT, [ref]$null, [ref]$null)
foreach ($fn in $ast.EndBlock.Statements | Where-Object { $_ -is [System.Management.Automation.Language.FunctionDefinitionAst] }) {
    . ([scriptblock]::Create($fn.Extent.Text))
}
function Refresh-Path {}
function Test-IsAdmin { $true }
function Write-Host { param([Parameter(ValueFromRemainingArguments)]$Rest) }
"""


def pwsh(body: str, workdir: Path) -> subprocess.CompletedProcess[str]:
    script = workdir / "run.ps1"
    script.write_text(LOAD + body, encoding="utf-8")
    return subprocess.run(
        ["pwsh", "-NoProfile", "-NonInteractive", "-File", str(script)],
        capture_output=True, text=True, timeout=120,
        env={"PATH": "/usr/bin:/bin:/opt/homebrew/bin:/usr/local/bin", "HOME": str(workdir), "TUNNEL_SCRIPT": str(SCRIPT)},
    )


@unittest.skipUnless(shutil.which("pwsh"), "pwsh is not installed")
class TunnelScript(unittest.TestCase):
    def setUp(self) -> None:
        self.dir = Path(tempfile.mkdtemp())

    def tearDown(self) -> None:
        shutil.rmtree(self.dir, ignore_errors=True)

    def test_hostname_has_no_personal_default(self) -> None:
        done = pwsh(
            r"""
$param = $ast.ParamBlock.Parameters | Where-Object { $_.Name.VariablePath.UserPath -eq 'Hostname' }
$mandatory = [bool]($param.Attributes | Where-Object { $_.TypeName.Name -eq 'Parameter' -and $_.NamedArguments.ArgumentName -contains 'Mandatory' })
@{ default = [string]$param.DefaultValue; mandatory = $mandatory } | ConvertTo-Json -Compress
""",
            self.dir,
        )
        self.assertEqual(done.returncode, 0, done.stderr)
        result = json.loads(done.stdout)
        self.assertEqual(result["default"], "", "the hostname must be passed, not defaulted to a personal domain")
        self.assertTrue(result["mandatory"])

    def run_cloudflared_install(self, status: str, subject: str) -> subprocess.CompletedProcess[str]:
        target = self.dir / "cloudflared"
        return pwsh(
            rf"""
$CloudflaredDir = '{target}'
function Get-Command {{ param([Parameter(Position = 0)]$Name) $null }}
function Invoke-WebRequest {{ param($Uri, $OutFile, $TimeoutSec, [switch]$UseBasicParsing) Set-Content -Path $OutFile -Value 'binary' }}
function Get-AuthenticodeSignature {{
    param([Parameter(Position = 0)]$FilePath)
    [pscustomobject]@{{ Status = '{status}'; SignerCertificate = [pscustomobject]@{{ Subject = '{subject}' }} }}
}}
try {{ $path = Ensure-CloudflaredCli; "installed:$path" }} catch {{ "refused:$($_.Exception.Message)" }}
""",
            self.dir,
        )

    def test_unsigned_cloudflared_download_is_refused_and_removed(self) -> None:
        done = self.run_cloudflared_install("NotSigned", "")
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertIn("refused:", done.stdout)
        leftovers = list((self.dir / "cloudflared").glob("*")) if (self.dir / "cloudflared").exists() else []
        self.assertEqual(leftovers, [], "an unverified binary was left in the install folder")

    def test_download_signed_by_someone_else_is_refused(self) -> None:
        done = self.run_cloudflared_install("Valid", "CN=Mallory, O=Mallory Ltd")
        self.assertIn("refused:", done.stdout)
        self.assertFalse((self.dir / "cloudflared" / "cloudflared.exe").exists())

    def test_cloudflare_signed_download_is_installed(self) -> None:
        done = self.run_cloudflared_install("Valid", "CN=Cloudflare, Inc., O=Cloudflare, Inc., L=San Francisco, S=California, C=US")
        self.assertIn("installed:", done.stdout, done.stdout + done.stderr)
        self.assertTrue((self.dir / "cloudflared" / "cloudflared.exe").is_file())

    def test_ax_install_never_evaluates_downloaded_code(self) -> None:
        marker = self.dir / "evaluated"
        done = pwsh(
            rf"""
$root = '{self.dir}'
function Get-Command {{ param([Parameter(Position = 0)]$Name) $null }}
function Invoke-WebRequest {{ param($Uri, [switch]$UseBasicParsing, $OutFile) [pscustomobject]@{{ Content = "Set-Content -Path '{marker}' -Value ran" }} }}
function Invoke-RestMethod {{ param($Uri, [switch]$UseBasicParsing, $OutFile) "Set-Content -Path '{marker}' -Value ran" }}
try {{ Ensure-AxCli; 'done' }} catch {{ "refused:$($_.Exception.Message)" }}
""",
            self.dir,
        )
        self.assertFalse(marker.exists(), "downloaded installer text was evaluated: " + done.stdout + done.stderr)
        self.assertIn("refused:", done.stdout)


if __name__ == "__main__":
    unittest.main()
