#!/usr/bin/env python3
"""Repository hygiene: no tracked file is one that .gitignore says to ignore.

A file added before its .gitignore rule stays tracked, goes stale (compiled
bytecode, build output) and shows up as a diff on every run that rebuilds it.
"""

from __future__ import annotations

import shutil
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


@unittest.skipUnless(shutil.which("git") and (ROOT / ".git").exists(), "needs a git checkout")
class TrackedFiles(unittest.TestCase):
    def test_no_tracked_file_is_ignored(self) -> None:
        result = subprocess.run(
            ["git", "ls-files", "--cached", "--ignored", "--exclude-standard"],
            cwd=ROOT, capture_output=True, text=True, check=True,
        )
        tracked_but_ignored = result.stdout.split()
        self.assertEqual(tracked_but_ignored, [], "git rm --cached these files: " + " ".join(tracked_but_ignored))


if __name__ == "__main__":
    unittest.main()
