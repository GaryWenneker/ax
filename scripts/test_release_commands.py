#!/usr/bin/env python3
"""The release helpers only suggest and forward what release-tag.ps1 accepts.

release-tag.ps1 refuses -Force, -Bump and -AllowDirty: versions change only
through a PR to main. ship-local.ps1 must not print a command that is always
refused, publish-release.ps1 must not forward -Force, and ship-local -Bump
must use scripts/set-version.py (the one version writer) instead of its own
copy, and say that the change has to reach main through a PR.
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / "scripts"
PWSH = shutil.which("pwsh")
REFUSED = re.compile(r"release-tag\.ps1[^\n\"]*-(Force|Bump|AllowDirty)\b")


class PrintedCommands(unittest.TestCase):
    def test_no_script_suggests_a_refused_release_tag_flag(self) -> None:
        for path in sorted(SCRIPTS.glob("*.ps1")):
            if path.name == "release-tag.ps1":
                continue
            with self.subTest(script=path.name):
                self.assertIsNone(REFUSED.search(path.read_text(encoding="utf-8")))

    def test_ship_local_bump_uses_set_version(self) -> None:
        text = (SCRIPTS / "ship-local.ps1").read_text(encoding="utf-8")
        self.assertIn("set-version.py", text)
        self.assertNotIn("function Set-WorkspaceVersion", text)
        self.assertNotIn("latest.txt", text)
        self.assertIn("through a PR", text)


@unittest.skipUnless(PWSH, "pwsh is not installed")
class PublishReleaseForwarding(unittest.TestCase):
    def setUp(self) -> None:
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir)
        shutil.copy(SCRIPTS / "publish-release.ps1", self.dir / "publish-release.ps1")
        # Stub that records the arguments it was given.
        (self.dir / "release-tag.ps1").write_text(
            "param([string]$Tag, [switch]$Force, [switch]$Wait)\n"
            "Write-Output \"tag=$Tag force=$Force wait=$Wait\"\n",
            encoding="utf-8",
        )

    def publish(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [PWSH, "-NoProfile", "-NonInteractive", "-File", str(self.dir / "publish-release.ps1"), *args],
            capture_output=True, text=True, timeout=60, env=dict(os.environ),
        )

    def test_forwards_tag_and_wait(self) -> None:
        out = self.publish("-Version", "v1.2.3", "-Wait")
        self.assertEqual(out.returncode, 0, out.stderr)
        self.assertIn("tag=v1.2.3 force=False wait=True", out.stdout)

    def test_force_is_not_forwarded(self) -> None:
        out = self.publish("-Version", "v1.2.3", "-Force")
        self.assertNotIn("force=True", out.stdout)
        self.assertNotEqual(out.returncode, 0)


if __name__ == "__main__":
    unittest.main()
