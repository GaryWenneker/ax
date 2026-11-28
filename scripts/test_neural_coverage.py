#!/usr/bin/env python3
"""Tests for scripts/neural-coverage.py, the line-coverage gate for the Neural files.

A floor of 0% can never fail, so a Neural file that lost all its tests would
still print `ok`. Every file needs a real floor, and the gauntlet needs a
negative control that zeroes a single Neural file.
"""

from __future__ import annotations

import importlib.util
import io
import contextlib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("neural_coverage", ROOT / "scripts/neural-coverage.py")
coverage = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(coverage)


def full_report() -> dict[str, dict[int, int]]:
    return {path: {line: 1 for line in range(1, 101)} for path in coverage.FLOORS}


def floors_fail(files: dict[str, dict[int, int]]) -> bool:
    with contextlib.redirect_stdout(io.StringIO()):
        return coverage.check_floors(files)


class Floors(unittest.TestCase):
    def test_every_floor_can_fail(self) -> None:
        for path, floor in coverage.FLOORS.items():
            with self.subTest(path=path):
                self.assertGreater(floor, 0.5, "a floor this low lets a file lose most of its tests")
                self.assertLessEqual(floor, 1.0)

    def test_full_coverage_passes(self) -> None:
        self.assertFalse(floors_fail(full_report()))

    def test_one_file_without_tests_fails(self) -> None:
        for path in coverage.FLOORS:
            with self.subTest(path=path):
                files = full_report()
                files[path] = {line: 0 for line in files[path]}
                self.assertTrue(floors_fail(files))

    def test_a_file_missing_from_the_report_fails(self) -> None:
        files = full_report()
        del files["crates/ax-neural/src/predict.rs"]
        self.assertTrue(floors_fail(files))


class Gauntlet(unittest.TestCase):
    def test_gauntlet_zeroes_one_neural_file_as_a_negative_control(self) -> None:
        text = (ROOT / "scripts/neural-gauntlet.sh").read_text(encoding="utf-8")
        self.assertIn("one-file-zero.info", text)
        self.assertRegex(text, r'must_fail "[^"]*one Neural file[^"]*" python3 scripts/neural-coverage\.py')


if __name__ == "__main__":
    unittest.main()
