#!/usr/bin/env python3
"""The changed-line coverage gate reads only real added lines from a diff."""

from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "context-safety" / "changed_coverage.py"
spec = importlib.util.spec_from_file_location("changed_coverage", SCRIPT)
changed_coverage = importlib.util.module_from_spec(spec)
spec.loader.exec_module(changed_coverage)

EDIT_THEN_DELETE = """\
diff --git a/crates/x/src/a.rs b/crates/x/src/a.rs
--- a/crates/x/src/a.rs
+++ b/crates/x/src/a.rs
@@ -10,0 +11,2 @@ fn a() {
+    let one = 1;
+    let two = 2;
diff --git a/crates/x/src/old.rs b/crates/x/src/old.rs
deleted file mode 100644
--- a/crates/x/src/old.rs
+++ /dev/null
@@ -1,2 +0,0 @@
-fn old() {}
-fn older() {}
"""

ADD_FILE = """\
diff --git a/crates/x/src/new.rs b/crates/x/src/new.rs
new file mode 100644
--- /dev/null
+++ b/crates/x/src/new.rs
@@ -0,0 +1,1 @@
+fn new() {}
"""


class ParseChangedLines(unittest.TestCase):
    def test_deleted_file_header_adds_no_line_to_the_previous_file(self) -> None:
        changed, text = changed_coverage.parse_changed_lines(EDIT_THEN_DELETE)
        self.assertEqual(changed, {"crates/x/src/a.rs": {11, 12}})
        self.assertEqual(text["crates/x/src/a.rs"], ["    let one = 1;", "    let two = 2;"])

    def test_added_file_counts_its_lines(self) -> None:
        changed, _ = changed_coverage.parse_changed_lines(ADD_FILE)
        self.assertEqual(changed, {"crates/x/src/new.rs": {1}})

    def test_run_script_uses_python3(self) -> None:
        run = (ROOT / "scripts" / "context-safety" / "run.sh").read_text(encoding="utf-8")
        bare = [line for line in run.splitlines() if line.split()[:1] == ["python"]]
        self.assertEqual(bare, [], "stock macOS has no `python`; call python3")


if __name__ == "__main__":
    unittest.main()
