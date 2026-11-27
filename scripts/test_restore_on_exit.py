#!/usr/bin/env python3
"""Tests for scripts/lib/restore-on-exit.sh, the guard that puts mutated source back.

The mutation gauntlets rewrite tracked source in place. The guard must restore
every guarded file however the script ends: success, a failing command under
`set -e`, Ctrl+C (SIGINT to the process group) or a kill (SIGTERM, SIGHUP).
"""

from __future__ import annotations

import os
import re
import signal
import subprocess
import tempfile
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HELPER = ROOT / "scripts" / "lib" / "restore-on-exit.sh"

# Every script that rewrites tracked source in place to run a mutant.
MUTATING_SCRIPTS = [
    "tools/gauntlet-agents-git-share.sh",
    "tools/gauntlet-policy-revisions.sh",
    "tools/gauntlet-policy-zip-package.sh",
    "tools/gauntlet-policy-git-share-indicator.sh",
    "tools/gauntlet-policy-table-oneline.sh",
    "tools/gauntlet-macos-theme.sh",
    "scripts/dav-mount-mutants.sh",
    "scripts/ide-panel-mutants.sh",
]

ORIGINAL = "pristine source\n"


def _script(body: str) -> str:
    return f"""#!/usr/bin/env bash
set -euo pipefail
source "{HELPER}"
{body}
"""


class RestoreOnExit(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)
        self.target = self.dir / "src.rs"
        self.target.write_text(ORIGINAL)
        self.ready = self.dir / "ready"

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def _run(self, body: str) -> subprocess.CompletedProcess[str]:
        script = self.dir / "run.sh"
        script.write_text(_script(body))
        return subprocess.run(["bash", str(script)], capture_output=True, text=True, timeout=30)

    def _interrupt(self, sig: int) -> int:
        script = self.dir / "run.sh"
        script.write_text(_script(f"""
ax_guard_file "{self.target}"
echo mutant > "{self.target}"
touch "{self.ready}"
sleep 30
"""))
        proc = subprocess.Popen(["bash", str(script)], start_new_session=True,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        deadline = time.time() + 10
        while not self.ready.exists():
            self.assertLess(time.time(), deadline, "script never reached the mutant")
            time.sleep(0.05)
        self.assertEqual(self.target.read_text(), "mutant\n")
        # Ctrl+C and most kills reach the whole foreground process group.
        os.killpg(proc.pid, sig)
        proc.communicate(timeout=10)
        return proc.returncode

    def test_restores_on_normal_exit(self) -> None:
        result = self._run(f'ax_guard_file "{self.target}"\necho mutant > "{self.target}"\n')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.target.read_text(), ORIGINAL)

    def test_restores_when_a_command_fails(self) -> None:
        result = self._run(f'ax_guard_file "{self.target}"\necho mutant > "{self.target}"\nfalse\n')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.target.read_text(), ORIGINAL)

    def test_restores_on_sigint(self) -> None:
        self.assertNotEqual(self._interrupt(signal.SIGINT), 0)
        self.assertEqual(self.target.read_text(), ORIGINAL)

    def test_restores_on_sigterm(self) -> None:
        self.assertNotEqual(self._interrupt(signal.SIGTERM), 0)
        self.assertEqual(self.target.read_text(), ORIGINAL)

    def test_restores_on_sighup(self) -> None:
        self.assertNotEqual(self._interrupt(signal.SIGHUP), 0)
        self.assertEqual(self.target.read_text(), ORIGINAL)

    def test_guarding_twice_keeps_the_first_copy(self) -> None:
        result = self._run(f"""
ax_guard_file "{self.target}"
echo mutant1 > "{self.target}"
ax_guard_file "{self.target}"
echo mutant2 > "{self.target}"
""")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.target.read_text(), ORIGINAL)

    def test_restores_several_files_and_keeps_exit_code(self) -> None:
        other = self.dir / "other.rs"
        other.write_text("second\n")
        result = self._run(f"""
ax_guard_file "{self.target}"
ax_guard_file "{other}"
echo x > "{self.target}"
echo y > "{other}"
exit 7
""")
        self.assertEqual(result.returncode, 7)
        self.assertEqual(self.target.read_text(), ORIGINAL)
        self.assertEqual(other.read_text(), "second\n")


class MutatingScriptsUseTheGuard(unittest.TestCase):
    def test_each_mutating_script_guards_the_files_it_rewrites(self) -> None:
        for rel in MUTATING_SCRIPTS:
            text = (ROOT / rel).read_text(encoding="utf-8")
            self.assertRegex(text, r'(?m)^\s*(source|\.) "[^"]*lib/restore-on-exit\.sh"',
                             f"{rel}: source scripts/lib/restore-on-exit.sh")
            self.assertRegex(text, r"\bax_guard_file\b", f"{rel}: guard each file before mutating it")
            # The guard must be registered before the first in-place rewrite.
            first_guard = re.search(r"\bax_guard_file\b", text).start()
            first_write = min(m.start() for m in re.finditer(r"write_text|perl -0?pi\b|perl -i\b|open\(p, \"w\"\)|>>\s*\"\$", text))
            self.assertLess(first_guard, first_write, f"{rel}: guard before the first rewrite")


if __name__ == "__main__":
    unittest.main()
