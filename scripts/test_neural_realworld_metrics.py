#!/usr/bin/env python3
"""Offline tests for the retrieval metrics in neural-realworld-run.py.

A ranked file is relevant only when it is the same path as an expected entry.
A distractor that sits under an expected directory, or shares a suffix with an
expected file, is not a hit.
"""

from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "neural_realworld_run", Path(__file__).with_name("neural-realworld-run.py")
)
run = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(run)


class MetricTests(unittest.TestCase):
    def test_distractor_under_an_expected_directory_is_not_a_hit(self):
        relevant = ["crates/ax-cli/src"]
        ranked = ["crates/ax-cli/src/main.rs", "crates/ax-cli/src"]
        self.assertEqual(run.precision_at(relevant, ranked, 1), 0.0)
        self.assertEqual(run.mrr(relevant, ranked), 0.5)
        self.assertEqual(run.recall_at(relevant, ranked, 1), 0.0)
        self.assertEqual(run.ndcg_at(relevant, ranked, 1), 0.0)

    def test_suffix_near_miss_is_not_a_hit(self):
        relevant = ["docs/README.md", "crates/ax-memory/src/lib.rs"]
        ranked = ["README.md", "src/lib.rs", "lib.rs"]
        self.assertEqual(run.precision_at(relevant, ranked, 3), 0.0)
        self.assertEqual(run.recall_at(relevant, ranked, 3), 0.0)
        self.assertEqual(run.mrr(relevant, ranked), 0.0)
        self.assertEqual(run.ndcg_at(relevant, ranked, 3), 0.0)

    def test_empty_string_matches_nothing(self):
        self.assertEqual(run.precision_at(["a.rs"], [""], 1), 0.0)
        self.assertEqual(run.mrr(["a.rs"], [""]), 0.0)

    def test_several_files_under_one_directory_count_once(self):
        relevant = ["crates/ax-db/src", "crates/ax-db/src/queries.rs"]
        ranked = ["crates/ax-db/src/a.rs", "crates/ax-db/src/b.rs", "crates/ax-db/src/queries.rs"]
        self.assertAlmostEqual(run.precision_at(relevant, ranked, 3), 1 / 3)
        self.assertEqual(run.recall_at(relevant, ranked, 3), 0.5)

    def test_exact_matches_score_fully(self):
        relevant = ["crates/ax-db/src/queries.rs", "crates/ax-cli/src"]
        ranked = ["crates/ax-cli/src", "crates/ax-db/src/queries.rs", "README.md"]
        self.assertEqual(run.precision_at(relevant, ranked, 2), 1.0)
        self.assertEqual(run.recall_at(relevant, ranked, 2), 1.0)
        self.assertEqual(run.mrr(relevant, ranked), 1.0)
        self.assertAlmostEqual(run.ndcg_at(relevant, ranked, 2), 1.0)

    def test_separator_and_dot_prefix_are_normalized(self):
        relevant = ["crates/ax-db/src/queries.rs", "crates/ax-cli/src/"]
        ranked = [".\\crates\\ax-db\\src\\queries.rs", "./crates/ax-cli/src"]
        self.assertEqual(run.recall_at(relevant, ranked, 2), 1.0)

    def test_repeated_ranked_entry_is_one_hit(self):
        relevant = ["a.rs", "b.rs"]
        ranked = ["a.rs", "a.rs"]
        self.assertEqual(run.precision_at(relevant, ranked, 2), 0.5)


if __name__ == "__main__":
    unittest.main()
