#!/usr/bin/env python3
"""Tests for the shell mutant runners that share scripts/lib/mutate.sh.

Each runner rewrites tracked source, runs a test suite and puts the source back.
Two properties matter:

* The source comes back byte for byte however the run ends, Ctrl+C included,
  and no `*.orig` file is left in the tree.
* Only a failing test counts as a killed mutant. A compile error, a syntax error
  or a missing toolchain is a run error and fails the run, because it proves
  nothing about the tests.

Each runner runs in a throwaway copy of the files it mutates, with fake `cargo`
and `node` commands on PATH.
"""

from __future__ import annotations

import os
import signal
import subprocess
import tempfile
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

WEB_UI = "crates/ax-web/web-ui/src"
RUNNERS = {
    "scripts/picker-ide-mutants.sh": [
        "crates/ax-web/src/folder_picker.rs",
        "crates/ax-web/src/agent.rs",
        f"{WEB_UI}/ideInstall.ts",
        f"{WEB_UI}/vaultMount.ts",
    ],
    "scripts/policy-graph-mutants.sh": [
        f"{WEB_UI}/lib/policyGraph.ts",
        f"{WEB_UI}/lib/graphSettings.ts",
        f"{WEB_UI}/lib/graphLabels.ts",
        f"{WEB_UI}/lib/autoGroup.ts",
        "crates/ax-web/src/links_api.rs",
        "crates/ax-web/src/memory.rs",
        "crates/ax-cli/src/commands/turn_hook.rs",
    ],
    "scripts/vault-folders-mutants.sh": [
        "crates/ax-web/src/dav/folders.rs",
        "crates/ax-web/src/vault_folders.rs",
        "crates/ax-web/src/dav/fs.rs",
        "crates/ax-web/src/dav/mod.rs",
        "crates/ax-memory/src/folder_sync.rs",
        f"{WEB_UI}/vaultMount.ts",
        f"{WEB_UI}/memoryCategory.ts",
    ],
    "scripts/v601-mutants.sh": [
        "crates/ax-quality/src/bootstrap.rs",
        "crates/ax-usage/src/savings.rs",
    ],
}
# Needs a live `ax web` and a browser, so it is only checked statically.
STATIC_ONLY = ["scripts/wysiwyg-mutants.sh"]

LIBS = ["scripts/lib/restore-on-exit.sh", "scripts/lib/mutate.sh"]

FAKE = r"""#!/bin/sh
tool=$(basename "$0")
# An unmutated tree passes, as a real suite's baseline does.
git -C "$FAKE_REPO" diff --quiet && exit 0
case "$FAKE_MODE" in
  pass) exit 0 ;;
  testfail)
    if [ "$tool" = cargo ]; then
      echo "test it_works ... FAILED"
      echo "test result: FAILED. 0 passed; 1 failed; 0 ignored"
      exit 101
    fi
    echo "# fail 1"; exit 1 ;;
  builderror)
    if [ "$tool" = cargo ]; then
      echo "error[E0425]: cannot find value \`x\` in this scope" >&2
      echo "error: could not compile \`ax-web\` (lib test) due to 1 previous error" >&2
      exit 101
    fi
    echo "SyntaxError: Unexpected token ')'"; echo "# fail 1"; exit 1 ;;
  nomarker) echo "something went wrong" >&2; exit 2 ;;
  hang) touch "$FAKE_READY"; sleep 30; exit 0 ;;
esac
exit 99
"""


class Tree:
    """A git-tracked copy of one runner, the shared libraries and its target files."""

    def __init__(self, runner: str) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name) / "repo"
        self.runner = runner
        self.files = RUNNERS[runner]
        for rel in [runner, *LIBS, *self.files]:
            src = ROOT / rel
            if not src.exists():
                continue
            dest = self.root / rel
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(src.read_bytes())
            dest.chmod(src.stat().st_mode)
        bin_dir = Path(self.tmp.name) / "bin"
        bin_dir.mkdir()
        for tool in ("cargo", "node"):
            path = bin_dir / tool
            path.write_text(FAKE)
            path.chmod(0o755)
        self.ready = Path(self.tmp.name) / "ready"
        self.env = {
            **os.environ,
            "PATH": f"{bin_dir}:{os.environ['PATH']}",
            "TMPDIR": self.tmp.name,
            "FAKE_READY": str(self.ready),
            "FAKE_REPO": str(self.root),
        }
        git = ["git", "-C", str(self.root), "-c", "user.name=t", "-c", "user.email=t@t"]
        subprocess.run([*git, "init", "-q"], check=True)
        subprocess.run([*git, "add", "-A"], check=True)
        subprocess.run([*git, "commit", "-qm", "base"], check=True)
        self.before = {rel: (self.root / rel).read_bytes() for rel in self.files}

    def cleanup(self) -> None:
        self.tmp.cleanup()

    def run(self, mode: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(["bash", str(self.root / self.runner)], capture_output=True,
                              text=True, timeout=120, env={**self.env, "FAKE_MODE": mode})

    def assert_pristine(self, case: unittest.TestCase) -> None:
        for rel, data in self.before.items():
            case.assertEqual((self.root / rel).read_bytes(), data, f"{rel} was not restored")
        leftovers = sorted(str(p.relative_to(self.root)) for p in self.root.rglob("*.orig"))
        case.assertEqual(leftovers, [], "backup files left in the tree")


class RunnerBehavior(unittest.TestCase):
    def for_each_runner(self, check) -> None:
        for runner in RUNNERS:
            with self.subTest(runner=runner):
                tree = Tree(runner)
                try:
                    check(tree)
                finally:
                    tree.cleanup()

    def test_failing_tests_kill_every_mutant(self) -> None:
        def check(tree: Tree) -> None:
            result = tree.run("testfail")
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("SURVIVED", result.stdout)
            self.assertNotIn("RUN ERROR", result.stdout)
            tree.assert_pristine(self)
        self.for_each_runner(check)

    def test_a_passing_suite_is_a_survivor(self) -> None:
        def check(tree: Tree) -> None:
            result = tree.run("pass")
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertIn("SURVIVED", result.stdout)
            tree.assert_pristine(self)
        self.for_each_runner(check)

    def test_a_build_error_is_not_a_kill(self) -> None:
        def check(tree: Tree) -> None:
            result = tree.run("builderror")
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertNotRegex(result.stdout, r"(?m)^killed:")
            self.assertIn("RUN ERROR", result.stdout)
            tree.assert_pristine(self)
        self.for_each_runner(check)

    def test_a_failure_without_a_failing_test_is_not_a_kill(self) -> None:
        def check(tree: Tree) -> None:
            result = tree.run("nomarker")
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertNotRegex(result.stdout, r"(?m)^killed:")
            tree.assert_pristine(self)
        self.for_each_runner(check)

    def test_ctrl_c_during_a_suite_restores_the_source(self) -> None:
        def check(tree: Tree) -> None:
            proc = subprocess.Popen(["bash", str(tree.root / tree.runner)], start_new_session=True,
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                    env={**tree.env, "FAKE_MODE": "hang"})
            deadline = time.time() + 30
            while not tree.ready.exists():
                if proc.poll() is not None or time.time() > deadline:
                    out, err = proc.communicate()
                    self.fail(f"runner never reached a suite: {out!r} {err!r}")
                time.sleep(0.05)
            changed = [rel for rel, data in tree.before.items()
                       if (tree.root / rel).read_bytes() != data]
            self.assertTrue(changed, "a mutant should be applied while the suite runs")
            os.killpg(proc.pid, signal.SIGINT)
            proc.communicate(timeout=20)
            self.assertNotEqual(proc.returncode, 0)
            tree.assert_pristine(self)
        self.for_each_runner(check)


class RunnersUseTheSharedLibrary(unittest.TestCase):
    def test_each_runner_sources_mutate_sh_and_keeps_no_local_backup(self) -> None:
        for rel in [*RUNNERS, *STATIC_ONLY]:
            with self.subTest(runner=rel):
                text = (ROOT / rel).read_text(encoding="utf-8")
                self.assertRegex(text, r'(?m)^\s*(source|\.) "[^"]*lib/mutate\.sh"')
                self.assertNotRegex(text, r"\.orig\b")
                self.assertNotRegex(text, r"(?m)^\s*mutate\(\)")
                # restore-on-exit.sh owns the traps; a runner's own trap would replace them.
                self.assertNotRegex(text, r"(?m)^\s*trap\b")


if __name__ == "__main__":
    unittest.main()
