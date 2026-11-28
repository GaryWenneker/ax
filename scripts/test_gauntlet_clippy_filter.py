#!/usr/bin/env python3
"""Tests for scripts/gauntlet-clippy-filter.ps1 (gauntlet layer 6).

The filter must count clippy findings on macOS and Linux, where clippy reports
forward-slash paths, as well as on Windows. Skipped when pwsh is not installed.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

FILTER = Path(__file__).with_name("gauntlet-clippy-filter.ps1")
PWSH = shutil.which("pwsh")


def diag(file_name: str, lint: str, line: int = 10) -> dict:
    return {
        "reason": "compiler-message",
        "message": {
            "code": {"code": lint},
            "rendered": f"warning: {lint} at {file_name}:{line}",
            "spans": [{"is_primary": True, "file_name": file_name, "line_start": line}],
        },
    }


def verdict(diags: list[dict], baseline: dict[str, int]) -> dict:
    script = f"""
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. '{FILTER}'
$d = @(Get-Content -Raw -LiteralPath $env:CLIPPY_FILTER_DIAGS | ConvertFrom-Json)
$b = @{{}}
(Get-Content -Raw -LiteralPath $env:CLIPPY_FILTER_BASELINE | ConvertFrom-Json).PSObject.Properties |
    ForEach-Object {{ $b[$_.Name] = [int]$_.Value }}
$v = Get-ClippyVerdict -Diagnostics $d -Crates @('ax-db', 'ax-mcp') -Baseline $b
[pscustomobject]@{{ Error = $v.Error; New = @($v.New); Total = $v.Total }} |
    ConvertTo-Json -Compress -Depth 4
"""
    with tempfile.TemporaryDirectory() as tmp:
        d_path, b_path = Path(tmp, "diags.json"), Path(tmp, "baseline.json")
        d_path.write_text(json.dumps(diags), encoding="utf-8")
        b_path.write_text(json.dumps(baseline), encoding="utf-8")
        env = dict(os.environ, CLIPPY_FILTER_DIAGS=str(d_path), CLIPPY_FILTER_BASELINE=str(b_path))
        run = subprocess.run(
            [PWSH, "-NoProfile", "-NonInteractive", "-Command", script],
            capture_output=True, text=True, env=env, check=False,
        )
    if run.returncode != 0:
        raise AssertionError(f"pwsh failed ({run.returncode}): {run.stderr}{run.stdout}")
    return json.loads(run.stdout)


@unittest.skipUnless(PWSH, "pwsh is not installed")
class ClippyFilterTests(unittest.TestCase):
    def test_forward_slash_finding_is_new(self):
        v = verdict([diag("crates/ax-mcp/src/x.rs", "clippy::needless_return")], {})
        self.assertIsNone(v["Error"])
        self.assertEqual(v["Total"], 1)
        self.assertEqual(v["New"], ["crates/ax-mcp/src/x.rs [clippy::needless_return]: 1 found, 0 baselined"])

    def test_backslash_finding_is_new(self):
        v = verdict([diag("crates\\ax-mcp\\src\\x.rs", "clippy::needless_return")], {})
        self.assertEqual(v["Total"], 1)
        self.assertEqual(len(v["New"]), 1)

    def test_backslash_baseline_key_matches_forward_slash_finding(self):
        v = verdict(
            [diag("crates/ax-db/src/queries.rs", "clippy::len_zero")],
            {"crates\\ax-db\\src\\queries.rs [clippy::len_zero]": 1},
        )
        self.assertEqual(v["Total"], 1)
        self.assertEqual(v["New"], [])

    def test_count_above_baseline_fails(self):
        v = verdict(
            [diag("crates/ax-db/src/queries.rs", "clippy::len_zero", n) for n in (1, 2)],
            {"crates/ax-db/src/queries.rs [clippy::len_zero]": 1},
        )
        self.assertEqual(len(v["New"]), 1)

    def test_out_of_scope_crates_are_ignored(self):
        v = verdict([diag("crates/ax-lsp/src/x.rs", "clippy::len_zero")], {})
        self.assertIsNone(v["Error"])
        self.assertEqual((v["Total"], v["New"]), (0, []))

    def test_duplicate_lib_and_test_reports_count_once(self):
        d = diag("crates/ax-db/src/a.rs", "clippy::len_zero")
        v = verdict([d, d], {})
        self.assertEqual(v["Total"], 1)

    def test_findings_in_scope_that_the_filter_drops_are_an_error(self):
        v = verdict([diag("/work/ax/crates/ax-db/src/a.rs", "clippy::len_zero")], {})
        self.assertIn("none matched the scope filter", v["Error"])


if __name__ == "__main__":
    unittest.main()
