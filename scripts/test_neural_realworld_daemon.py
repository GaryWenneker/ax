#!/usr/bin/env python3
"""Tests for scripts/neural-realworld-daemon.py, run in the foreground with a fake agent runner.

The daemon relaunches the real-world agent matrix until it finishes. It must not
spin forever: a configuration error stops it at once, other failures are retried
with backoff up to a cap, and its log and pid files live in a per-user state
directory instead of fixed /tmp paths.
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# Fake agent runner: counts its launches and exits with FAKE_EXIT.
FAKE_RUNNER = """import os, sys
from pathlib import Path
count = Path(os.environ["FAKE_COUNT"])
count.write_text(str(int(count.read_text() or 0) + 1) if count.exists() else "1")
sys.exit(int(os.environ["FAKE_EXIT"]))
"""


class RealworldDaemon(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name) / "repo"
        (self.root / "scripts").mkdir(parents=True)
        (self.root / "scripts/neural-realworld-daemon.py").write_bytes(
            (ROOT / "scripts/neural-realworld-daemon.py").read_bytes())
        (self.root / "scripts/neural-realworld-agent.py").write_text(FAKE_RUNNER)
        self.state = Path(self.tmp.name) / "state"
        self.count = Path(self.tmp.name) / "count"

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def _run(self, exit_code: int) -> subprocess.CompletedProcess[str]:
        env = {**os.environ, "FAKE_EXIT": str(exit_code), "FAKE_COUNT": str(self.count),
               "AX_RW_STATE_DIR": str(self.state), "AX_RW_BACKOFF_SEC": "0",
               "AX_RW_MAX_RESTARTS": "3"}
        env.pop("LOG", None)
        try:
            return subprocess.run([sys.executable, str(self.root / "scripts/neural-realworld-daemon.py"),
                                   "--foreground"], env=env, capture_output=True, text=True, timeout=30)
        except subprocess.TimeoutExpired:
            self.fail(f"daemon still relaunching after 30s ({self._launches()} launches)")

    def _launches(self) -> int:
        return int(self.count.read_text()) if self.count.exists() else 0

    def test_a_finished_matrix_stops_the_daemon(self) -> None:
        result = self._run(0)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self._launches(), 1)

    def test_a_configuration_error_is_not_retried(self) -> None:
        result = self._run(78)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self._launches(), 1)

    def test_a_usage_error_is_not_retried(self) -> None:
        result = self._run(2)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self._launches(), 1)

    def test_repeated_failures_stop_after_the_restart_cap(self) -> None:
        result = self._run(1)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self._launches(), 4)  # the first launch plus three restarts

    def test_logs_and_pid_files_go_to_the_state_directory(self) -> None:
        self._run(0)
        self.assertTrue((self.state / "rw-agent-daemon.log").is_file())
        self.assertTrue((self.state / "rw-agent-resume.log").is_file())


class OneLauncher(unittest.TestCase):
    def test_the_shell_watchdog_is_gone(self) -> None:
        self.assertFalse((ROOT / "scripts/neural-realworld-watchdog.sh").exists(),
                         "two launchers share pid and log files; keep the daemon only")


if __name__ == "__main__":
    unittest.main()
