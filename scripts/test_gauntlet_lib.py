#!/usr/bin/env python3
"""Tests for scripts/lib/gauntlet.sh, the layers the spec gauntlets share.

Each spec gauntlet declares its changed files, crates, baseline file and its own
layers; the shared layers (workspace suite against the baseline, clippy and
rustfmt on changed lines, release build plus CLI docs, the partial-run guard)
live once in scripts/lib/gauntlet.sh.
"""

from __future__ import annotations

import os
import re
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LIB = ROOT / "scripts" / "lib" / "gauntlet.sh"
GAUNTLETS = [
    "scripts/gauntlet-git-hooks.sh",
    "scripts/gauntlet-install-daemon-hygiene.sh",
    "scripts/gauntlet-per-turn-memory.sh",
    "scripts/gauntlet-turn-outcomes.sh",
]


class GauntletsShareTheLayers(unittest.TestCase):
    def test_each_gauntlet_sources_the_library_and_keeps_no_copy(self) -> None:
        for rel in GAUNTLETS:
            text = (ROOT / rel).read_text(encoding="utf-8")
            with self.subTest(script=rel):
                self.assertRegex(text, r'(?m)^(source|\.) "[^"]*lib/gauntlet\.sh"')
                self.assertNotRegex(text, r"(?m)^changed_lines\(\)", "changed_lines is defined in the library")
                self.assertNotIn("comm -23", text, "the baseline comparison is in the library")
                self.assertNotIn("rustfmt --edition", text, "the rustfmt layer is in the library")
                self.assertNotIn('BASELINE_FAILURES="', text, "keep the baseline in a file")
                baseline = re.search(r'gauntlet_load_baseline "?([^"\s]+)"?', text)
                self.assertIsNotNone(baseline, "load a baseline file")
                self.assertTrue((ROOT / baseline.group(1)).is_file(), baseline.group(1))


class _Repo(unittest.TestCase):
    """A throwaway git repo with BASE = the first commit."""

    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.repo = Path(self.tmp.name) / "repo"
        self.repo.mkdir()
        self.bin = Path(self.tmp.name) / "bin"
        self.bin.mkdir()
        self._git("init", "-q")
        self._git("config", "user.email", "t@example.invalid")
        self._git("config", "user.name", "t")
        (self.repo / "old.rs").write_text("fn a() {}\nfn b() {}\nfn c() {}\n")
        self._git("add", ".")
        self._git("commit", "-qm", "base")
        self.base = self._git("rev-parse", "HEAD").strip()

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def _git(self, *args: str) -> str:
        return subprocess.run(["git", *args], cwd=self.repo, check=True, capture_output=True, text=True).stdout

    def _fake(self, name: str, body: str) -> None:
        path = self.bin / name
        path.write_text("#!/bin/sh\n" + body)
        path.chmod(0o755)

    def _run(self, body: str, changed: list[str]) -> subprocess.CompletedProcess[str]:
        script = Path(self.tmp.name) / "run.sh"
        files = " ".join(changed)
        script.write_text(f"""#!/usr/bin/env bash
set -euo pipefail
ROOT="{self.repo}"
cd "$ROOT"
BASE="{self.base}"
CHANGED_RS=({files})
source "{LIB}"
gauntlet_init test
{body}
""")
        env = {**os.environ, "PATH": f"{self.bin}:{os.environ['PATH']}", "TMPDIR": self.tmp.name}
        return subprocess.run(["bash", str(script)], cwd=self.repo, env=env,
                              capture_output=True, text=True, timeout=60)


class ChangedLines(_Repo):
    def test_lists_each_added_or_changed_line(self) -> None:
        (self.repo / "old.rs").write_text("fn a() {}\nfn B() {}\nfn c() {}\nfn d() {}\n")
        result = self._run("gauntlet_changed_lines old.rs", ["old.rs"])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.split(), ["2", "4"])


class RustfmtDrift(_Repo):
    def setUp(self) -> None:
        super().setUp()
        # A fake rustfmt: one "Diff in" hunk for any input containing BAD.
        self._fake("rustfmt", 'f=""; for a in "$@"; do f="$a"; done\n'
                   'if [ -f "$f" ]; then c="$(cat "$f")"; n="$PWD/$f"; else c="$(cat)"; n=stdin; fi\n'
                   'case "$c" in *BAD*) echo "Diff in $n:1:";; esac\nexit 0\n')

    def test_a_new_file_counts_from_zero_without_git_errors(self) -> None:
        (self.repo / "new.rs").write_text("fn ok() {}\n")
        result = self._run("gauntlet_rustfmt_drift", ["new.rs"])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("new.rs: 0 diff hunks (was 0)", result.stdout)
        self.assertNotIn("fatal", result.stderr)

    def test_new_drift_fails(self) -> None:
        (self.repo / "new.rs").write_text("BAD\n")
        result = self._run("gauntlet_rustfmt_drift", ["new.rs"])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("rustfmt drift grew in new.rs", result.stderr)


class ClippyChangedLines(_Repo):
    def setUp(self) -> None:
        super().setUp()
        self._fake("cargo", 'echo "old.rs:4:1: warning: unused"\necho "old.rs:1:1: warning: old code"\n')
        (self.repo / "old.rs").write_text("fn a() {}\nfn b() {}\nfn c() {}\nfn d() {}\n")

    def test_reports_only_findings_on_changed_lines(self) -> None:
        result = self._run("gauntlet_clippy_changed_lines -p x", ["old.rs"])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("old.rs:4:", result.stdout)
        self.assertNotIn("old.rs:1:", result.stdout)
        self.assertIn("1 clippy finding(s) on changed lines", result.stderr)

    def test_a_file_without_findings_passes(self) -> None:
        (self.repo / "other.rs").write_text("fn e() {}\n")
        result = self._run("gauntlet_clippy_changed_lines -p x", ["other.rs"])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("0 on changed lines", result.stdout)

    def test_a_stale_changed_file_list_fails(self) -> None:
        result = self._run("gauntlet_clippy_changed_lines -p x", ["gone.rs"])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("changed file list is stale: gone.rs", result.stderr)


class WorkspaceAgainstBaseline(_Repo):
    def _baseline(self, *names: str) -> Path:
        path = Path(self.tmp.name) / "baseline.txt"
        path.write_text("# known failures\n" + "".join(f"{n}\n" for n in names))
        return path

    def _cargo_with(self, *failed: str) -> None:
        lines = "".join(f'echo "test {n} ... FAILED"\n' for n in failed)
        self._fake("cargo", lines + 'echo "test result: FAILED. 5 passed; 1 failed; 0 ignored"\nexit 101\n')

    def test_known_failures_pass(self) -> None:
        self._cargo_with("a::known")
        result = self._run(f'gauntlet_load_baseline "{self._baseline("a::known")}"\n'
                           "gauntlet_workspace_against_baseline", [])
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_a_new_failure_fails(self) -> None:
        self._cargo_with("a::known", "b::new")
        result = self._run(f'gauntlet_load_baseline "{self._baseline("a::known")}"\n'
                           "gauntlet_workspace_against_baseline", [])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("new test failures: b::new", result.stderr)

    def test_a_missing_baseline_file_fails(self) -> None:
        result = self._run('gauntlet_load_baseline "nope.txt"', [])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing nope.txt", result.stderr)


class PartialRunGuard(_Repo):
    def test_a_layer_subset_is_not_evidence(self) -> None:
        env_body = 'LAYERS="3 4"\ngauntlet_finish'
        result = self._run(env_body, [])
        self.assertEqual(result.returncode, 0)
        self.assertIn("PARTIAL RUN", result.stdout)
        self.assertNotIn("GAUNTLET PASSED", result.stdout)

    def test_a_full_run_passes(self) -> None:
        result = self._run("gauntlet_finish", [])
        self.assertIn("GAUNTLET PASSED", result.stdout)


if __name__ == "__main__":
    unittest.main()
