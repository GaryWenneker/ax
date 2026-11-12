#!/usr/bin/env python3
"""Unit tests for score_ucp.py — includes intentional NC1 failure proof."""

from __future__ import annotations

import json
import unittest
from pathlib import Path

from score_ucp import extract_refs, score_run, score_task

ROOT = Path(__file__).resolve().parents[1]
EXCHANGES = ROOT / "exchanges.json"
TASKS = ROOT / "tasks.json"


class ScoreUcpTests(unittest.TestCase):
    def test_empty_gold_raises(self):
        with self.assertRaises(ValueError):
            score_task([], gold_files=[], gold_symbols=[], allow_empty_gold=False)

    def test_pr251_sample_ucp_positive(self):
        data = json.loads(EXCHANGES.read_text())
        gold_files = [
            "install.ps1",
            "site/public/install.ps1",
            "crates/ax-cli/src/commands/upgrade.rs",
            "site/tests/install-script.test.js",
            "site/tests/install-script.test.ts",
        ]
        gold_symbols = [
            "upgrade_cmd_batch",
            "activate_staged_install",
            "Test-AxReleaseAsset",
            "Disable-AxStaleGitHubTokens",
            "Clear-AxPendingUpgradeState",
        ]
        s = score_task(data["exchanges"], gold_files, gold_symbols, k=5)
        self.assertGreater(s["ucp_at_k"], 0.0)
        self.assertIn("upgrade_cmd_batch", s["hit_symbols"] + s["miss_symbols"])
        # Must hit upgrade.rs via explore/impact/node
        self.assertTrue(
            any("upgrade.rs" in f for f in s["hit_files"]),
            s,
        )

    def test_unrelated_refs_low_ucp(self):
        exchanges = [
            {
                "seq": 3,
                "direction": "agent_to_ax",
                "response": {
                    "entries": [
                        {
                            "node": {
                                "name": "unrelated_helper",
                                "filePath": "crates/ax-web/src/dav/mount.rs",
                            },
                            "score": 1.0,
                        }
                    ]
                },
            }
        ]
        s = score_task(
            exchanges,
            gold_files=["crates/ax-cli/src/commands/upgrade.rs"],
            gold_symbols=["upgrade_cmd_batch"],
            k=5,
        )
        self.assertEqual(s["ucp_at_k"], 0.0)
        self.assertEqual(s["recall_at_k"], 0.0)

    def test_score_run_loads_tasks(self):
        data = json.loads(EXCHANGES.read_text())
        result = score_run(TASKS, {"pr-251": data["exchanges"]}, k=5)
        self.assertEqual(result["n_scored"], 1)
        self.assertIsNotNone(result["median_ucp_at_k"])
        row = result["tasks"][0]
        self.assertEqual(row["id"], "pr-251")
        # S2: sample preflight omitted pr-review-comments — expect False
        self.assertIn("pr-review-comments", row["skills"])


if __name__ == "__main__":
    unittest.main()
