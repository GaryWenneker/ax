#!/usr/bin/env python3
"""No helper in scripts/ or tools/ rewrites git history.

A script that runs `git filter-branch` or `git filter-repo` over every ref
rewrites every branch and tag in the clone with new SHAs, so a later push
diverges from origin. Such a tool does not belong next to everyday helpers.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REWRITE = re.compile(r"\bgit\s+filter-(branch|repo)\b")


class NoHistoryRewrite(unittest.TestCase):
    def test_no_script_rewrites_history(self) -> None:
        hits = []
        for folder in ("scripts", "tools"):
            for path in sorted((ROOT / folder).rglob("*")):
                if not path.is_file() or path.name == Path(__file__).name or "node_modules" in path.parts:
                    continue
                try:
                    text = path.read_text(encoding="utf-8")
                except (UnicodeDecodeError, OSError):
                    continue
                if REWRITE.search(text):
                    hits.append(str(path.relative_to(ROOT)))
        self.assertEqual(hits, [])


if __name__ == "__main__":
    unittest.main()
