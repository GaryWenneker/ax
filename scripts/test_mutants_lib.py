#!/usr/bin/env python3
"""Tests for scripts/mutants_lib.py, the engine behind the Python mutant runners.

The engine applies each mutant once, runs its test command, restores the file
byte for byte however the run ends, and counts a mutant as killed only when a
test failed. A build error, a survivor or a mutant that does not apply fails
the run.
"""

from __future__ import annotations

import ast
import os
import sys
import tempfile
import unittest
from pathlib import Path
from subprocess import CompletedProcess

SCRIPTS = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS))

import mutants_lib  # noqa: E402

RUNNERS = [
    "policy-dedup-mutants.py",
    "read-guard-mutants.py",
    "review-loop-mutants.py",
    "stack-review-mutants.py",
]

TEST_FAIL = "test a::b ... FAILED\ntest a::b FAILED\ntest result: FAILED. 0 passed; 1 failed"
TEST_OK = "test result: ok. 1 passed; 0 failed"


def done(code: int, out: str) -> CompletedProcess[str]:
    return CompletedProcess(["x"], code, out, "")


class Classify(unittest.TestCase):
    def test_failing_test_is_a_kill(self) -> None:
        self.assertEqual(mutants_lib.classify(101, TEST_FAIL), "killed")

    def test_compile_error_is_not_a_kill(self) -> None:
        text = "error[E0308]: mismatched types\n" + TEST_FAIL
        self.assertEqual(mutants_lib.classify(101, text), "build-error")
        self.assertEqual(mutants_lib.classify(101, "error: could not compile `x`"), "build-error")

    def test_passing_suite_is_a_survivor(self) -> None:
        self.assertEqual(mutants_lib.classify(0, TEST_OK), "survived")

    def test_other_exit_is_unexpected(self) -> None:
        self.assertEqual(mutants_lib.classify(127, "cargo: not found"), "unexpected")


class RunMutants(unittest.TestCase):
    def setUp(self) -> None:
        self.dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.dir.cleanup)
        self.cwd = os.getcwd()
        os.chdir(self.dir.name)
        self.addCleanup(os.chdir, self.cwd)
        Path("src.rs").write_bytes(b"fn f() { a > b }\r\n")
        self.original = Path("src.rs").read_bytes()

    def test_all_killed_passes_and_restores(self) -> None:
        seen = []

        def runner(cmd: list[str]) -> CompletedProcess[str]:
            seen.append(Path("src.rs").read_bytes())
            return done(0, TEST_OK) if len(seen) == 1 else done(101, TEST_FAIL)

        rc = mutants_lib.run_mutants([("flip", "src.rs", "a > b", "a < b", ["t"])], [["t"]], runner=runner)
        self.assertEqual(rc, 0)
        self.assertEqual(seen[1], b"fn f() { a < b }\r\n")
        self.assertEqual(Path("src.rs").read_bytes(), self.original)

    def test_build_error_fails_the_run(self) -> None:
        results = iter([done(0, TEST_OK), done(101, "error[E0425]: x\n" + TEST_FAIL)])
        rc = mutants_lib.run_mutants([("m", "src.rs", "a > b", "a <", ["t"])], [["t"]],
                                     runner=lambda cmd: next(results))
        self.assertEqual(rc, 1)
        self.assertEqual(Path("src.rs").read_bytes(), self.original)

    def test_survivor_and_unapplied_fail_the_run(self) -> None:
        rc = mutants_lib.run_mutants([("m", "src.rs", "a > b", "a < b", ["t"])], [["t"]],
                                     runner=lambda cmd: done(0, TEST_OK))
        self.assertEqual(rc, 1)
        rc = mutants_lib.run_mutants([("m", "src.rs", "nowhere", "x", ["t"])], [["t"]],
                                     runner=lambda cmd: done(0, TEST_OK))
        self.assertEqual(rc, 1)

    def test_red_baseline_stops_before_mutating(self) -> None:
        calls = []

        def runner(cmd: list[str]) -> CompletedProcess[str]:
            calls.append(cmd)
            return done(101, TEST_FAIL)

        rc = mutants_lib.run_mutants([("m", "src.rs", "a > b", "a < b", ["t"])], [["t"]], runner=runner)
        self.assertEqual(rc, 1)
        self.assertEqual(len(calls), 1)

    def test_interrupt_during_test_restores_source(self) -> None:
        results = iter([done(0, TEST_OK)])

        def runner(cmd: list[str]) -> CompletedProcess[str]:
            try:
                return next(results)
            except StopIteration:
                raise KeyboardInterrupt from None

        with self.assertRaises(KeyboardInterrupt):
            mutants_lib.run_mutants([("m", "src.rs", "a > b", "a < b", ["t"])], [["t"]], runner=runner)
        self.assertEqual(Path("src.rs").read_bytes(), self.original)


class RunnersShareTheEngine(unittest.TestCase):
    """Each runner keeps only its MUTANTS table and test commands."""

    def test_runners_use_mutants_lib(self) -> None:
        for name in RUNNERS:
            with self.subTest(runner=name):
                tree = ast.parse((SCRIPTS / name).read_text(encoding="utf-8"))
                defs = {n.name for n in tree.body if isinstance(n, ast.FunctionDef)}
                self.assertFalse(defs & {"sha", "run", "main"}, f"{name} defines its own engine: {defs}")
                imports = {
                    n.module if isinstance(n, ast.ImportFrom) else a.name
                    for n in tree.body if isinstance(n, (ast.Import, ast.ImportFrom))
                    for a in getattr(n, "names", [])
                }
                self.assertIn("mutants_lib", imports)


if __name__ == "__main__":
    unittest.main()
