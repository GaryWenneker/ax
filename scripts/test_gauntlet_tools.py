"""Checks for the gauntlet and mutation helpers under tools/.

Each test drives the real script with fakes for the slow parts (cargo, npx, ax web),
so it runs in seconds without a build.
"""
import importlib.util
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


if __name__ == "__main__":
    unittest.main()
