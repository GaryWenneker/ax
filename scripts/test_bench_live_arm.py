#!/usr/bin/env python3
"""The gauntlet's live Claude arm runs in the real checkout, so it may only read.

It skips permission prompts so it can run unattended; without a tool deny list
the agent could edit files or run shell commands in the developer's tree.
"""

from __future__ import annotations

import subprocess
import sys
import types
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts" / "bench-agent-efficiency"))
sys.modules.setdefault("yaml", types.ModuleType("yaml"))  # only main() reads YAML

import gauntlet  # noqa: E402

WRITE_TOOLS = {"Bash", "Edit", "Write", "NotebookEdit"}


class LiveArmTools(unittest.TestCase):
    def test_claude_once_denies_every_tool_that_can_change_the_tree(self) -> None:
        seen: list[list[str]] = []

        def fake_run(cmd, **_kwargs):
            seen.append(cmd)
            return subprocess.CompletedProcess(cmd, 0, stdout="{}", stderr="")

        with mock.patch.object(gauntlet.subprocess, "run", fake_run):
            gauntlet.claude_once("prompt", Path("cfg.json"), ROOT)

        (cmd,) = seen
        self.assertIn("--disallowedTools", cmd)
        denied = set(cmd[cmd.index("--disallowedTools") + 1].split(","))
        self.assertTrue(WRITE_TOOLS <= denied, f"not denied: {sorted(WRITE_TOOLS - denied)}")
        self.assertEqual(cmd[-1], "prompt", "the prompt must stay the last argument")


if __name__ == "__main__":
    unittest.main()
