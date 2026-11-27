"""Checks for the gauntlet and mutation helpers under tools/.

Each test drives the real script with fakes for the slow parts (cargo, npx, ax web),
so it runs in seconds without a build.
"""
import importlib.util
import os
import re
import shutil
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
        self.assertTrue("tools/check-test-baseline.sh" in text, "L4 does not call the checker")


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


NEGATIVE_CONTROL = ROOT / "scripts" / "lib" / "negative-control.sh"
CONTROLLED_GAUNTLETS = [
    "gauntlet-content-width.sh",
    "gauntlet-macos-theme.sh",
    "gauntlet-policy-git-share-indicator.sh",
    "gauntlet-policy-revisions.sh",
    "gauntlet-policy-storage.sh",
    "gauntlet-policy-table-oneline.sh",
    "gauntlet-policy-zip-package.sh",
]


def control_calls(text):
    """Every `negative_control ...` command in a script, with its continuation lines."""
    calls, current = [], None
    for line in text.splitlines():
        if current is None and line.lstrip().startswith("negative_control "):
            current = []
        if current is not None:
            current.append(line)
            if not line.rstrip().endswith("\\"):
                calls.append("\n".join(current))
                current = None
    return calls


class NegativeControlTest(unittest.TestCase):
    """scripts/lib/negative-control.sh and the gauntlets that use it."""

    def control(self, content, *args):
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "index.css"
            target.write_text(content)
            script = f'source "{NEGATIVE_CONTROL}"\nnegative_control "$@"\necho after'
            run = subprocess.run(["bash", "-c", script, "bash", "label", str(target), *args],
                                 capture_output=True, text=True, timeout=30, env=dict(os.environ, TMPDIR=tmp))
            leftovers = [p.name for p in Path(tmp).iterdir() if p.name != "index.css"]
            return run, leftovers

    def test_a_real_check_passes_its_control(self):
        run, leftovers = self.control("--layout-max: 1100px;\n", "1100px", "1000px",
                                      "grep", "-q", "--", "--layout-max: 1100px")
        self.assertEqual(run.returncode, 0, run.stderr)
        self.assertIn("after", run.stdout)
        self.assertEqual(leftovers, [])

    def test_a_check_that_always_passes_fails_its_control(self):
        run, leftovers = self.control("--layout-max: 1100px;\n", "1100px", "1000px", "grep", "-q", "layout")
        self.assertEqual(run.returncode, 1)
        self.assertIn("still passes", run.stderr)
        self.assertNotIn("after", run.stdout)
        self.assertEqual(leftovers, [])

    def test_a_check_that_never_passes_fails_its_control(self):
        run, _ = self.control("--layout-max: 1100px;\n", "1100px", "1000px", "grep", "-q", "no-such-text")
        self.assertEqual(run.returncode, 1)
        self.assertIn("does not pass", run.stderr)

    def test_a_missing_anchor_fails_the_control(self):
        run, _ = self.control("--layout-max: 1100px;\n", "1320px", "1000px", "grep", "-q", "1100px")
        self.assertEqual(run.returncode, 1)
        self.assertIn("is not in", run.stderr)

    def test_no_gauntlet_keeps_a_vacuous_control(self):
        vacuous = re.compile(r"does-not-exist|NOT_A_REAL_MARKER|never-valid|echo 'Mach-O")
        for name in CONTROLLED_GAUNTLETS + ["gauntlet-macos-dev-ax-path.sh"]:
            with self.subTest(name):
                self.assertIsNone(vacuous.search((TOOLS / name).read_text()))

    def test_every_gauntlet_control_holds_on_the_current_tree(self):
        for name in CONTROLLED_GAUNTLETS:
            text = (TOOLS / name).read_text()
            calls = control_calls(text)
            with self.subTest(name):
                self.assertTrue("scripts/lib/negative-control.sh" in text, "the helper is not sourced")
                self.assertTrue(calls, "no negative_control call")
                if "rg -q" in "\n".join(calls) and shutil.which("rg") is None:
                    self.skipTest("rg is not installed")
                css = re.search(r'^CSS="\$ROOT/(.*)"$', text, re.M)
                prelude = f'set -euo pipefail\nROOT="{ROOT}"\ncd "$ROOT"\nsource "{NEGATIVE_CONTROL}"\n'
                if css:
                    prelude += f'CSS="$ROOT/{css.group(1)}"\n'
                run = subprocess.run(["bash", "-c", prelude + "\n".join(calls)], capture_output=True,
                                     text=True, timeout=60)
                self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
                self.assertEqual(run.stdout.count("fails on a broken copy"), len(calls), run.stdout)

    def test_the_mach_o_check_is_run_against_a_real_binary(self):
        text = (TOOLS / "gauntlet-macos-dev-ax-path.sh").read_text()
        self.assertTrue(re.search(r'is_macho "\$bin"', text), "the control does not check $bin")
        self.assertTrue(re.search(r'is_macho "\$shim"', text), "the shim is not checked with is_macho")


if __name__ == "__main__":
    unittest.main()
