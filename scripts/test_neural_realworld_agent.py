#!/usr/bin/env python3
"""Tests for scripts/neural-realworld-agent.py, run in a throwaway git repo with a fake agent.

The runner breaks source on purpose, lets an agent edit the checkout and writes
each arm's `neural` block into ax.json. After every attempt the checkout must be
back as it was: ax.json and every tracked file, even when the agent rewrote the
injected region. A tree with uncommitted changes is refused, because the reset
would discard them. A timed-out agent keeps its stderr in the result row.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RESULTS = "docs/specs/ax-neural/REALWORLD-RESULTS.json"
AX_JSON = '{\n  "project": "fixture"\n}\n'
SOURCE = "fn mode() -> &'static str { \"fixed-value\" }\n"
OTHER = "untouched\n"

MANIFEST = {"injects": {"t1": {"file": "src/a.rs", "fixed": '"fixed-value"', "broken": '"broken-value"'}}}
TASKS = {
    "tasks": [{
        "id": "t1",
        "split": "heldOut",
        "category": "bug",
        "difficulty": "easy",
        "description": "Restore the fixed value.",
        "acceptanceCriteria": ["mode() returns fixed-value"],
        "expectedRelevantFiles": ["src/a.rs"],
        "inject": {"manifestId": "t1"},
        "verify": [{"cmd": "grep -q fixed-value src/a.rs"}],
    }]
}

# Fake cursor-agent. "rewrite" replaces the injected line (both anchors gone) and
# edits a second tracked file; "hang" writes to stderr and never answers.
FAKE_AGENT = """#!/bin/sh
case "$FAKE_AGENT_MODE" in
  rewrite)
    echo 'fn mode() -> &'"'"'static str { "agent-value" }' > src/a.rs
    echo changed > src/other.rs
    echo '{"result": "done", "usage": {}}'
    ;;
  hang)
    echo "stuck on git status" >&2
    exec sleep 30
    ;;
esac
"""


class RealworldAgent(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name) / "repo"
        for rel in ("scripts/neural-realworld-agent.py", "scripts/neural-realworld-inject.py"):
            dest = self.root / rel
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes((ROOT / rel).read_bytes())
        spec = self.root / "docs/specs/ax-neural"
        (spec / "injects").mkdir(parents=True)
        (spec / "injects/manifest.json").write_text(json.dumps(MANIFEST))
        (spec / "REALWORLD-TASKS.json").write_text(json.dumps(TASKS))
        (self.root / RESULTS).write_text("{}\n")
        (self.root / "src").mkdir()
        (self.root / "src/a.rs").write_text(SOURCE)
        (self.root / "src/other.rs").write_text(OTHER)
        (self.root / ".gitignore").write_text("ax.json\n")
        self._git("init", "-q")
        self._git("add", "-A")
        self._git("commit", "-qm", "base")
        (self.root / "ax.json").write_text(AX_JSON)
        self.bin = Path(self.tmp.name) / "bin"
        self.bin.mkdir()
        agent = self.bin / "cursor-agent"
        agent.write_text(FAKE_AGENT)
        agent.chmod(0o755)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def _git(self, *args: str) -> str:
        return subprocess.run(["git", "-C", str(self.root), "-c", "user.name=t", "-c", "user.email=t@t", *args],
                              check=True, capture_output=True, text=True).stdout

    def _run(self, mode: str, extra: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
        env = {**os.environ, "PATH": f"{self.bin}:/usr/bin:/bin", "FAKE_AGENT_MODE": mode,
               "HOME": str(Path(self.tmp.name) / "home"), **(extra or {})}
        return subprocess.run([sys.executable, str(self.root / "scripts/neural-realworld-agent.py"),
                               "--task", "t1", "--arms", "E_learning_train_only"],
                              cwd=self.root, env=env, capture_output=True, text=True, timeout=120)

    def _rows(self) -> list[dict]:
        return json.loads((self.root / RESULTS).read_text())["agent"]["rows"]

    def test_checkout_is_restored_after_the_agent_rewrites_the_injected_region(self) -> None:
        result = self._run("rewrite")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual((self.root / "ax.json").read_text(), AX_JSON)
        self.assertEqual((self.root / "src/a.rs").read_text(), SOURCE)
        self.assertEqual((self.root / "src/other.rs").read_text(), OTHER)
        changed = [line for line in self._git("status", "--porcelain").splitlines()
                   if not line.endswith(RESULTS)]
        self.assertEqual(changed, [])
        [row] = self._rows()
        self.assertTrue(row["baselineRed"])
        self.assertFalse(row["taskSuccess"])

    def test_a_tree_with_uncommitted_changes_is_refused(self) -> None:
        (self.root / "src/other.rs").write_text("my work in progress\n")
        result = self._run("rewrite")
        self.assertEqual(result.returncode, 78, result.stdout)  # configuration error: not retried
        self.assertIn("src/other.rs", result.stdout + result.stderr)
        self.assertEqual((self.root / "src/other.rs").read_text(), "my work in progress\n")
        self.assertEqual((self.root / "src/a.rs").read_text(), SOURCE)
        self.assertEqual((self.root / "ax.json").read_text(), AX_JSON)

    def test_a_missing_inject_anchor_stops_the_run(self) -> None:
        (self.root / "src/a.rs").write_text("fn mode() {}\n")
        self._git("commit", "-qam", "drift")
        result = self._run("rewrite")
        self.assertEqual(result.returncode, 78, result.stdout)
        self.assertIn("inject fix t1 failed", result.stderr)
        self.assertEqual((self.root / "ax.json").read_text(), AX_JSON)

    def test_a_timed_out_agent_keeps_its_stderr(self) -> None:
        result = self._run("hang", {"AX_RW_AGENT_TIMEOUT_SEC": "1"})
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        [row] = self._rows()
        self.assertEqual(row["agent"]["error"], "agent_timeout")
        self.assertIn("stuck on git status", row["agent"]["stderrTail"])
        self.assertEqual((self.root / "src/a.rs").read_text(), SOURCE)


if __name__ == "__main__":
    unittest.main()
