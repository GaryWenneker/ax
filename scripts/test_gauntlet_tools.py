"""Checks for the gauntlet and mutation helpers under tools/.

Each test drives the real script with fakes for the slow parts (cargo, npx, ax web),
so it runs in seconds without a build.
"""
import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
TOOLS = ROOT / "tools"


def load_mutants():
    spec = importlib.util.spec_from_file_location("ax_admin_mutants", TOOLS / "ax_admin_mutants.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class MutantTimeoutTest(unittest.TestCase):
    def test_a_hanging_mutant_is_reported_and_the_run_continues(self):
        mutants = load_mutants()
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            target = tmp / "src.rs"
            target.write_text("const WAIT: u32 = 2;\n")
            hang = [sys.executable, "-c", "import time; time.sleep(30)"]
            finish = [sys.executable, "-c", "print('test result: FAILED. 0 passed; 1 failed')"]
            selected = [
                ("H01", "hangs", "src.rs", "= 2;", "= 30;", hang, "filter"),
                ("H02", "next", "src.rs", "= 2;", "= 3;", finish, "filter"),
            ]
            with mock.patch.object(mutants, "ROOT", tmp), \
                    mock.patch.object(mutants, "MUTANT_TIMEOUT", 1), \
                    mock.patch.object(mutants, "MUTANTS", selected), \
                    mock.patch("builtins.print") as printed:
                status = mutants.main()
            lines = [" ".join(str(a) for a in call.args) for call in printed.call_args_list]
            self.assertEqual(target.read_text(), "const WAIT: u32 = 2;\n")
            self.assertTrue(any(line.startswith("H01 hangs: killed by timeout") for line in lines), lines)
            self.assertTrue(any(line.startswith("H02 next:") for line in lines), lines)
            self.assertIn("mutants killed", lines[-1])
            self.assertEqual(status, 1)


class TestBaselineTest(unittest.TestCase):
    """tools/check-test-baseline.sh, used by gauntlet-ax-admin.sh L4."""

    BASELINE = "alpha_contract\nserver::beta_once\n"

    def check(self, log):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / "log").write_text(log)
            (tmp / "baseline").write_text(self.BASELINE)
            return subprocess.run(["bash", str(TOOLS / "check-test-baseline.sh"), str(tmp / "log"),
                                   str(tmp / "baseline")], capture_output=True, text=True, timeout=30)

    def test_failures_that_match_the_baseline_pass(self):
        run = self.check("test alpha_contract ... FAILED\ntest server::beta_once ... FAILED\n"
                         "test gamma ... ok\n")
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)

    def test_a_new_failure_fails(self):
        run = self.check("test alpha_contract ... FAILED\ntest server::beta_once ... FAILED\n"
                         "test gamma ... FAILED\n")
        self.assertNotEqual(run.returncode, 0)
        self.assertIn("gamma", run.stdout)

    def test_a_baseline_test_that_now_passes_fails(self):
        run = self.check("test alpha_contract ... ok\ntest server::beta_once ... FAILED\n")
        self.assertNotEqual(run.returncode, 0, run.stdout)
        self.assertIn("alpha_contract", run.stdout)

    def test_a_baseline_test_that_did_not_run_fails(self):
        run = self.check("test server::beta_once ... FAILED\n")
        self.assertNotEqual(run.returncode, 0, run.stdout)
        self.assertIn("alpha_contract", run.stdout)

    def test_the_gauntlet_uses_the_checker(self):
        text = (TOOLS / "gauntlet-ax-admin.sh").read_text()
        self.assertIn("tools/check-test-baseline.sh", text)


def gauntlet_block(script, start, end):
    """The lines of a gauntlet script between two step markers."""
    text = (TOOLS / script).read_text()
    return text[text.index(start):text.index(end)]


class EncodingLayerTest(unittest.TestCase):
    """gauntlet-ax-admin.sh L12 on file names with spaces."""

    def run_layer(self, name, content):
        block = gauntlet_block("gauntlet-ax-admin.sh", 'step "L12', 'step "L13')
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            git = lambda *args: subprocess.run(["git", "-c", "user.name=t", "-c", "user.email=t@t",
                                                "-c", "commit.gpgsign=false", *args], cwd=repo,
                                               check=True, capture_output=True)
            git("init", "-q", "-b", "main")
            (repo / "README.md").write_text("base\n")
            git("add", ".")
            git("commit", "-qm", "base")
            git("checkout", "-qb", "feature")
            path = repo / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content)
            git("add", ".")
            git("commit", "-qm", "change")
            (repo / "untracked file.txt").write_bytes(content)
            script = "set -euo pipefail\nstep() { :; }\nROOT=\"$PWD\"\n" + block
            return subprocess.run(["bash", "-c", script], cwd=repo, capture_output=True, text=True,
                                  timeout=60)

    def test_a_bom_in_a_changed_file_with_a_space_in_its_name_fails(self):
        run = self.run_layer("docs/My Notes.md", b"\xef\xbb\xbfhello\n")
        self.assertNotEqual(run.returncode, 0, run.stdout)
        self.assertIn("BOM in docs/My Notes.md", run.stdout)

    def test_clean_files_with_spaces_pass(self):
        run = self.run_layer("docs/My Notes.md", b"hello\n")
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        self.assertIn("encoding ok", run.stdout)

    def test_a_glob_character_in_a_name_is_not_expanded(self):
        run = self.run_layer("docs/*.md", b"\xef\xbb\xbfhello\n")
        self.assertNotEqual(run.returncode, 0, run.stdout)
        self.assertIn("BOM in docs/*.md", run.stdout)


if __name__ == "__main__":
    unittest.main()
