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
    "scripts/mutants-per-turn-memory.sh",
    "scripts/mutants-turn-outcomes.sh",
    "scripts/mutants-git-hooks.sh",
    "scripts/mutants-install-daemon-hygiene.sh",
    "scripts/mutants-client-cleanup.sh",
    "scripts/init-ides-mutants.sh",
    "scripts/graph-obsidian-mutants.sh",
    "scripts/green-vessel-mutants.sh",
    "scripts/link-origin-mutants.sh",
    "scripts/link-picker-mutants.sh",
    "scripts/mutate-spec-registry.sh",
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

    def test_restores_a_relative_path_after_the_script_changes_directory(self) -> None:
        elsewhere = self.dir / "elsewhere"
        elsewhere.mkdir()
        result = self._run(f"""
cd "{self.dir}"
ax_guard_file src.rs
echo mutant > src.rs
cd "{elsewhere}"
""")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.target.read_text(), ORIGINAL)
        self.assertFalse((elsewhere / "src.rs").exists())

    def test_scratch_directory_is_removed_on_exit(self) -> None:
        marker = self.dir / "scratch-path"
        result = self._run(f"""
test -d "$AX_GUARD_TMP"
cp "{self.target}" "$AX_GUARD_TMP/orig"
printf '%s' "$AX_GUARD_TMP" > "{marker}"
""")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(Path(marker.read_text()).exists())

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


def _fake_cargo(bin_dir: Path, exit_code: int) -> None:
    bin_dir.mkdir(parents=True, exist_ok=True)
    cargo = bin_dir / "cargo"
    cargo.write_text(f"#!/bin/sh\nexit {exit_code}\n")
    cargo.chmod(0o755)


class MutateSpecRegistry(unittest.TestCase):
    """scripts/mutate-spec-registry.sh, run in a copy of the tree with a fake cargo."""

    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        for rel in ("scripts/mutate-spec-registry.sh", "scripts/lib/restore-on-exit.sh",
                    "crates/ax-usage/src/spec_registry.rs"):
            dest = self.root / rel
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes((ROOT / rel).read_bytes())
        self.source = self.root / "crates/ax-usage/src/spec_registry.rs"

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def _run(self, cargo_exit: int) -> subprocess.CompletedProcess[str]:
        _fake_cargo(self.root / "bin", cargo_exit)
        env = {**os.environ, "PATH": f"{self.root / 'bin'}:{os.environ['PATH']}",
               "TMPDIR": str(self.root)}
        return subprocess.run(["bash", str(self.root / "scripts/mutate-spec-registry.sh")],
                              capture_output=True, text=True, timeout=60, env=env)

    def test_source_is_restored_byte_for_byte(self) -> None:
        before = self.source.read_bytes()
        result = self._run(cargo_exit=101)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("killed 2/2", result.stdout)
        self.assertEqual(self.source.read_bytes(), before)

    def test_source_is_restored_when_a_mutant_survives(self) -> None:
        before = self.source.read_bytes()
        result = self._run(cargo_exit=0)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.source.read_bytes(), before)

    def test_a_missing_anchor_fails_instead_of_testing_unchanged_source(self) -> None:
        text = self.source.read_text()
        anchor = 'let detected = if plan_count > 0 { "plan" } else { "spec" };'
        self.assertIn(anchor, text)
        self.source.write_text(text.replace(anchor, "let detected = pick_kind(plan_count);"))
        result = self._run(cargo_exit=101)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertNotIn("killed", result.stdout)


if __name__ == "__main__":
    unittest.main()
