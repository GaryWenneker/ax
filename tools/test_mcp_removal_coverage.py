"""Negative controls for the changed-line coverage gate."""
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location("mcp_removal_coverage", Path(__file__).with_name("check_mcp_removal_coverage.py"))
coverage = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(coverage)


class CoverageGateTests(unittest.TestCase):
    def test_uncovered_counted_region_is_reported(self):
        root = Path("/fixture")
        diff = "+++ b/crates/ax-mcp/src/style.rs\n@@ -0,0 +1,3 @@\n+one\n+two\n+three\n"
        data = {"data":[{"files":[{"filename":"/fixture/crates/ax-mcp/src/style.rs",
                  "segments":[[1,1,1,True,True,False],[2,1,0,True,True,False],[3,1,0,False,False,False]]}]}]}
        result = coverage.check(data, diff, root)
        self.assertEqual(result["covered"], 1)
        self.assertEqual(result["total"], 2)
        self.assertEqual(result["missing"], ["crates/ax-mcp/src/style.rs:2"])
        # Remove the defense's counted zero region: the known-bad fixture no
        # longer reports the missing line, proving this control is nonvacuous.
        data["data"][0]["files"][0]["segments"][1][3] = False
        self.assertEqual(coverage.check(data, diff, root)["missing"], [])

    def test_missing_source_and_empty_scope_fail_closed(self):
        with self.assertRaises(ValueError): coverage.check({"data":[]}, "+++ b/crates/ax-mcp/src/a.rs\n@@ -0,0 +1 @@\n+code", Path("/fixture"))
        with self.assertRaises(ValueError): coverage.check({"data":[]}, "", Path("/fixture"))

    def test_module_declarations_have_no_executable_coverage(self):
        diff = "+++ b/crates/ax-mcp/src/lib.rs\n@@ -0,0 +1 @@\n+mod helper;\n+++ b/crates/ax-mcp/src/helper.rs\n@@ -0,0 +1 @@\n+fn helper() {}\n"
        data = {"data":[{"files":[{"filename":"/fixture/crates/ax-mcp/src/helper.rs", "segments":[[1,1,1,True,True,False],[2,1,0,False,False,False]]}]}]}
        result = coverage.check(data, diff, Path("/fixture"))
        self.assertEqual(result["total"], 1)
        self.assertEqual(result["covered"], 1)
        self.assertEqual(result["declaration_only_files"], ["crates/ax-mcp/src/lib.rs"])
        with self.assertRaises(ValueError):
            coverage.check(data, diff.replace("mod helper;", "fn unchecked() {}"), Path("/fixture"))

    def test_cli_rejects_uncovered_source_and_unreadable_reports(self):
        script = Path(__file__).with_name("check_mcp_removal_coverage.py").resolve()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for command in [
                ["git", "init", "--quiet"],
                ["git", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "--quiet", "--allow-empty", "-m", "fixture"],
            ]:
                subprocess.run(command, cwd=root, check=True, capture_output=True)
            source = root / "crates/ax-mcp/src/style.rs"
            source.parent.mkdir(parents=True)
            source.write_text("fn example() {}\n", encoding="utf-8")
            subprocess.run(["git", "add", "."], cwd=root, check=True, capture_output=True)
            report = root / "coverage.json"
            data = {"data":[{"files":[{"filename":str(source), "segments":[[1,1,0,True,True,False],[2,1,0,False,False,False]]}]}]}
            report.write_text(json.dumps(data), encoding="utf-8")
            command = [sys.executable, str(script), "--report", str(report), "--base", "HEAD"]
            bad = subprocess.run(command, cwd=root, capture_output=True, text=True)
            self.assertNotEqual(bad.returncode, 0)
            self.assertIn("changed executable lines are uncovered", bad.stderr)
            data["data"][0]["files"][0]["segments"][0][2] = 1
            report.write_text(json.dumps(data), encoding="utf-8")
            good = subprocess.run(command, cwd=root, capture_output=True, text=True)
            self.assertEqual(good.returncode, 0, good.stderr)
            report.unlink()
            unreadable = subprocess.run(command, cwd=root, capture_output=True, text=True)
            self.assertNotEqual(unreadable.returncode, 0)
            self.assertIn("coverage check failed", unreadable.stderr)


if __name__ == "__main__": unittest.main()
