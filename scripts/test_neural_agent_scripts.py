#!/usr/bin/env python3
"""Tests for the live Neural agent runners, run in a copy of the tree with fakes.

scripts/neural-agent-live.py must fail when no arm succeeds, stop every daemon it
started even when an arm raises, and leave the checked-in evidence alone unless
asked. scripts/neural-agent-suite.py must not replace the results file when the
ax binary or cursor-agent is missing.
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
LIVE_EVIDENCE = "docs/specs/ax-neural/agent-eval-live-results.json"
SUITE_EVIDENCE = "docs/specs/ax-neural/agent-suite-live-results.json"
SENTINEL = '{"kind": "measured", "rows": ["earlier batch"]}\n'

# Fake ax: logs each call; `neural feedback` fails when FAIL_FEEDBACK is set.
FAKE_AX = """#!/bin/sh
echo "$*" >> "$AX_FAKE_LOG"
if [ "$1" = neural ] && [ -n "$FAIL_FEEDBACK" ]; then exit 1; fi
exit 0
"""

# Fake cursor-agent: answers every turn with something that is not JSON.
LOGGED_OUT_AGENT = "#!/bin/sh\necho 'Error: not logged in' >&2\nexit 1\n"


class _Tree(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name) / "repo"
        for rel in ("scripts/neural-agent-live.py", "scripts/neural-agent-suite.py",
                    "docs/specs/ax-neural/agent-suite-30.json"):
            dest = self.root / rel
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes((ROOT / rel).read_bytes())
        for rel in (LIVE_EVIDENCE, SUITE_EVIDENCE):
            (self.root / rel).write_text(SENTINEL)
        self.bin = Path(self.tmp.name) / "bin"
        self.bin.mkdir()
        self.log = Path(self.tmp.name) / "ax.log"
        self.ax = self._exe("ax-fake", FAKE_AX)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def _exe(self, name: str, body: str) -> Path:
        path = self.bin / name
        path.write_text(body)
        path.chmod(0o755)
        return path

    def _run(self, script: str, *args: str, ax: Path | None, agent: bool,
             extra: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
        if agent:
            self._exe("cursor-agent", LOGGED_OUT_AGENT)
        # Only the fake bin dir and the system tools: no real ax or agent.
        env = {"PATH": f"{self.bin}:/usr/bin:/bin", "AX_FAKE_LOG": str(self.log),
               "HOME": str(Path(self.tmp.name) / "home")}
        if ax is not None:
            env["AX_BIN"] = str(ax)
        env.update(extra or {})
        return subprocess.run([sys.executable, str(self.root / "scripts" / script), *args],
                              cwd=self.tmp.name, env=env, capture_output=True, text=True,
                              timeout=120)

    def _stops(self) -> list[str]:
        if not self.log.exists():
            return []
        return [line for line in self.log.read_text().splitlines() if line.startswith("daemon ")]

    def _inits(self) -> int:
        if not self.log.exists():
            return 0
        return sum(1 for line in self.log.read_text().splitlines() if line == "init")


class NeuralAgentLive(_Tree):
    def test_every_arm_failing_exits_nonzero(self) -> None:
        out = Path(self.tmp.name) / "out.json"
        result = self._run("neural-agent-live.py", "--out", str(out), ax=self.ax, agent=True)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(json.loads(out.read_text())["liveLlmTaskSuccess"]["successRate"], 0.0)

    def test_daemons_are_stopped_when_an_arm_raises(self) -> None:
        out = Path(self.tmp.name) / "out.json"
        result = self._run("neural-agent-live.py", "--out", str(out), ax=self.ax, agent=True,
                           extra={"FAIL_FEEDBACK": "1"})
        self.assertNotEqual(result.returncode, 0)
        # Arm E fails while seeding feedback; its project was initialised and must be stopped too.
        self.assertEqual(self._inits(), 3, self.log.read_text())
        self.assertEqual(len(self._stops()), 3, self.log.read_text())

    def test_default_output_leaves_the_checked_in_evidence_alone(self) -> None:
        self._run("neural-agent-live.py", ax=self.ax, agent=True)
        self.assertEqual((self.root / LIVE_EVIDENCE).read_text(), SENTINEL)

    def test_missing_binary_leaves_the_checked_in_evidence_alone(self) -> None:
        result = self._run("neural-agent-live.py", ax=Path(self.tmp.name) / "missing", agent=True)
        self.assertEqual(result.returncode, 2)
        self.assertEqual((self.root / LIVE_EVIDENCE).read_text(), SENTINEL)


class NeuralAgentSuite(_Tree):
    def test_missing_agent_does_not_replace_merged_results(self) -> None:
        result = self._run("neural-agent-suite.py", "--offset", "10", "--limit", "5", "--merge",
                           ax=self.ax, agent=False)
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("not_measured", result.stdout)
        self.assertEqual((self.root / SUITE_EVIDENCE).read_text(), SENTINEL)

    def test_missing_binary_does_not_replace_results(self) -> None:
        out = Path(self.tmp.name) / "suite.json"
        out.write_text(SENTINEL)
        result = self._run("neural-agent-suite.py", "--out", str(out),
                           ax=Path(self.tmp.name) / "missing", agent=True)
        self.assertEqual(result.returncode, 2)
        self.assertEqual(out.read_text(), SENTINEL)


if __name__ == "__main__":
    unittest.main()
