#!/usr/bin/env python3
"""Mutation scripts list each mutant once.

A mutant listed twice runs the same mutation twice and adds two to both the
total and the killed count, so the reported score overstates what was tested.
"""

from __future__ import annotations

import re
import unittest
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MUTANT_CALL = re.compile(r'^mutant\s+"([^"]+)"', re.MULTILINE)


class MutantNames(unittest.TestCase):
    def test_every_mutant_name_is_unique_per_script(self) -> None:
        checked = 0
        for script in sorted(ROOT.joinpath("scripts").glob("*mutants*.sh")):
            names = MUTANT_CALL.findall(script.read_text(encoding="utf-8"))
            if not names:
                continue
            checked += 1
            duplicates = sorted(name for name, count in Counter(names).items() if count > 1)
            with self.subTest(script=script.name):
                self.assertEqual(duplicates, [], f"{script.name} lists these mutants more than once")
        self.assertGreater(checked, 0, "no mutation script with mutant calls was found")


if __name__ == "__main__":
    unittest.main()
