#!/usr/bin/env python3
"""Tests for scripts/check-cli-docs.mjs: which binary and which doc it checks.

The gauntlets build a release binary and hand it to the check. The check must
use that binary (first argument, else AX_BIN) and find the repository's CLI
reference however the caller's working directory is set.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHECK = ROOT / "scripts" / "check-cli-docs.mjs"

# A fake `ax` whose only command is `status`, which the CLI reference documents.
FAKE_AX = """#!/bin/sh
if [ "$1" = "--help" ]; then
  printf 'Usage: ax\\n\\nCommands:\\n  status  Show status\\n\\nOptions:\\n'
  exit 0
fi
printf 'Usage: ax %s\\n' "$1"
"""

# A fake `ax` whose help fails, standing in for a stale or wrong binary.
BROKEN_AX = "#!/bin/sh\nexit 3\n"


@unittest.skipUnless(shutil.which("node"), "node is required")
class CheckCliDocsBinary(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)
        self.fake = self._bin("ax-fake", FAKE_AX)
        self.broken = self._bin("ax-broken", BROKEN_AX)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def _bin(self, name: str, body: str) -> Path:
        path = self.dir / name
        path.write_text(body)
        path.chmod(0o755)
        return path

    def _run(self, *args: str, ax_bin: Path | None = None) -> subprocess.CompletedProcess[str]:
        env = {k: v for k, v in os.environ.items() if k != "AX_BIN"}
        if ax_bin is not None:
            env["AX_BIN"] = str(ax_bin)
        # Run from an unrelated directory: relative defaults must not decide the outcome.
        return subprocess.run(["node", str(CHECK), *args], cwd=self.dir, env=env,
                              capture_output=True, text=True, timeout=60)

    def test_ax_bin_environment_variable_selects_the_binary(self) -> None:
        result = self._run(ax_bin=self.fake)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("all 1 ax commands", result.stdout)

    def test_first_argument_wins_over_ax_bin(self) -> None:
        result = self._run(str(self.fake), ax_bin=self.broken)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_a_failing_binary_fails_closed(self) -> None:
        result = self._run(ax_bin=self.broken)
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)


class GauntletsNameTheBinary(unittest.TestCase):
    def test_every_gauntlet_passes_the_binary_as_an_argument(self) -> None:
        scripts = sorted((ROOT / "scripts").glob("gauntlet-*.sh")) + [ROOT / "scripts/lib/gauntlet.sh"]
        calls = 0
        for script in scripts:
            for line in script.read_text(encoding="utf-8").splitlines():
                if "check-cli-docs.mjs" not in line:
                    continue
                calls += 1
                with self.subTest(script=script.name):
                    self.assertRegex(line, r'check-cli-docs\.mjs\s+"\$', f"{script.name}: pass the built binary")
        self.assertGreater(calls, 0, "no gauntlet runs the CLI docs check")


if __name__ == "__main__":
    unittest.main()
