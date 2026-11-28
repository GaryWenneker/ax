#!/usr/bin/env python3
"""Tests for the release wait loop in scripts/release-tag.ps1.

`gh run list --json createdAt` returns a UTC `...Z` string. Windows PowerShell
5.1 leaves it a string and `[datetime]` turns it into local time, while the
dispatch time is UTC. DateTime comparison ignores Kind, so on a machine east of
UTC a run created before the dispatch looked newer and was picked up as the new
run. The selection now lives in scripts/lib/release-run.ps1 and compares in UTC.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HELPER = ROOT / "scripts" / "lib" / "release-run.ps1"
PWSH = shutil.which("pwsh")


def select(runs: list[dict], head: str, requested_utc: str, tz: str, as_string: bool = True) -> str:
    shape = "$r.createdAt = ([string]$r.createdAtRaw).TrimStart('@')" if as_string else ""
    script = f"""
$ErrorActionPreference = 'Stop'
. '{HELPER}'
$runs = @('{json.dumps(runs)}' | ConvertFrom-Json)
# Windows PowerShell 5.1 keeps createdAt as a string; force that shape.
# The '@' keeps ConvertFrom-Json in PowerShell 7 from turning it into a DateTime.
foreach ($r in $runs) {{ {shape} }}
$requested = [datetime]::Parse('{requested_utc}', [Globalization.CultureInfo]::InvariantCulture,
    [Globalization.DateTimeStyles]'AdjustToUniversal,AssumeUniversal')
$run = Select-AxReleaseRun -Runs $runs -HeadSha '{head}' -RequestedAt $requested
if ($run) {{ $run.databaseId }} else {{ 'none' }}
"""
    env = dict(os.environ, TZ=tz)
    out = subprocess.run([PWSH, "-NoProfile", "-NonInteractive", "-Command", script],
                         capture_output=True, text=True, env=env, timeout=60)
    if out.returncode != 0:
        raise AssertionError(out.stdout + out.stderr)
    return out.stdout.strip()


def run(id_: int, created: str, sha: str = "abc") -> dict:
    return {"databaseId": id_, "createdAtRaw": "@" + created, "createdAt": created, "headSha": sha,
            "status": "completed", "conclusion": "failure"}


@unittest.skipUnless(PWSH, "pwsh is not installed")
class SelectReleaseRun(unittest.TestCase):
    def test_earlier_run_is_ignored_east_of_utc(self) -> None:
        older = run(1, "2026-11-27T10:00:00Z")
        for tz in ("Europe/Amsterdam", "Asia/Tokyo", "UTC", "America/New_York"):
            with self.subTest(tz=tz):
                self.assertEqual(select([older], "abc", "2026-11-27T10:30:00Z", tz), "none")

    def test_powershell7_datetime_values_compare_in_utc(self) -> None:
        older = run(1, "2026-11-27T10:00:00Z")
        newer = run(2, "2026-11-27T10:31:00Z")
        for tz in ("Europe/Amsterdam", "America/New_York"):
            with self.subTest(tz=tz):
                self.assertEqual(select([older], "abc", "2026-11-27T10:30:00Z", tz, as_string=False), "none")
                self.assertEqual(select([newer], "abc", "2026-11-27T10:30:00Z", tz, as_string=False), "2")

    def test_new_run_for_the_head_is_selected(self) -> None:
        runs = [run(9, "2026-11-27T10:31:00Z", sha="other"), run(7, "2026-11-27T10:31:00Z"),
                run(1, "2026-11-27T10:00:00Z")]
        self.assertEqual(select(runs, "abc", "2026-11-27T10:30:00Z", "Europe/Amsterdam"), "7")


class ReleaseTagUsesHelper(unittest.TestCase):
    def test_wait_loop_does_not_cast_created_at_to_local_time(self) -> None:
        text = (ROOT / "scripts" / "release-tag.ps1").read_text(encoding="utf-8")
        self.assertNotIn("[datetime]$_.createdAt", text)
        self.assertIn("Select-AxReleaseRun", text)
        self.assertIn("lib/release-run.ps1", text.replace("\\", "/"))


if __name__ == "__main__":
    unittest.main()
