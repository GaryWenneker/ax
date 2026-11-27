"""Checks for the gauntlet and mutation helpers under tools/.

Each test drives the real script with fakes for the slow parts (cargo, npx, ax web),
so it runs in seconds without a build.
"""
import importlib.util
import os
import re
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
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

    def test_no_gauntlet_announces_a_control_it_does_not_run(self):
        # A GAUNTLET_NEGATIVE switch that only prints a heading passes on a clean tree,
        # so it claims a negative control that can never fail.
        for path in sorted(TOOLS.glob("gauntlet-*")):
            with self.subTest(path.name):
                self.assertNotIn("GAUNTLET_NEGATIVE", path.read_text())

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


MUTATED_GAUNTLETS = {
    "gauntlet-agents-git-share.sh": [
        "crates/ax-policy/src/agents_share.rs",
        "crates/ax-policy/src/hierarchy.rs",
    ],
    "gauntlet-policy-git-share-indicator.sh": ["crates/ax-web/web-ui/src/components/ui/policyListUtils.ts"],
    "gauntlet-policy-revisions.sh": ["crates/ax-policy/src/revisions.rs"],
    "gauntlet-policy-table-oneline.sh": ["crates/ax-web/web-ui/src/components/ui/policyListUtils.ts"],
    "gauntlet-policy-zip-package.sh": ["crates/ax-policy/src/zip_package.rs"],
}
# Its mutants target a theme preset that is no longer in themes.ts, so it is only checked statically.
STATIC_MUTATED_GAUNTLETS = ["gauntlet-macos-theme.sh"]
MUTATION_HEADING = 'echo "== manual mutation'
FAKE_SUITE = r"""#!/bin/sh
tool=$(basename "$0")
case "$FAKE_MODE" in
  pass) exit 0 ;;
  testfail)
    if [ "$tool" = cargo ]; then
      echo "test it_works ... FAILED"
      echo "test result: FAILED. 0 passed; 1 failed; 0 ignored"
      exit 101
    fi
    echo "# fail 1"; exit 1 ;;
  builderror)
    if [ "$tool" = cargo ]; then
      echo "error[E0425]: cannot find value \`x\` in this scope" >&2
      echo "error: could not compile \`ax-policy\` (lib test) due to 1 previous error" >&2
      exit 101
    fi
    echo "SyntaxError: Unexpected token ')'"; exit 1 ;;
  nomarker) echo "something went wrong" >&2; exit 2 ;;
esac
exit 99
"""


class GauntletMutantsTest(unittest.TestCase):
    """The manual-mutation step of each gauntlet counts only a failing test as a kill.

    The step runs on its own, from its `== manual mutation` heading to the end of
    the script, in a throwaway copy of the files it mutates with fake `cargo` and
    `node` commands on PATH.
    """

    def run_mutants(self, name, mode):
        text = (TOOLS / name).read_text()
        self.assertIn(MUTATION_HEADING, text, "no manual mutation step")
        section = text[text.index(MUTATION_HEADING):]
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "repo"
            files = MUTATED_GAUNTLETS[name]
            for rel in ["scripts/lib/restore-on-exit.sh", "scripts/lib/mutate.sh", *files]:
                dest = root / rel
                dest.parent.mkdir(parents=True, exist_ok=True)
                dest.write_bytes((ROOT / rel).read_bytes())
            before = {rel: (root / rel).read_bytes() for rel in files}
            bin_dir = Path(tmp) / "bin"
            bin_dir.mkdir()
            for tool in ("cargo", "node"):
                (bin_dir / tool).write_text(FAKE_SUITE)
                (bin_dir / tool).chmod(0o755)
            prelude = f'set -euo pipefail\nROOT="{root}"\ncd "$ROOT"\nsource "$ROOT/scripts/lib/mutate.sh"\n'
            env = dict(os.environ, PATH=f"{bin_dir}:{os.environ['PATH']}", TMPDIR=tmp, FAKE_MODE=mode)
            run = subprocess.run(["bash", "-c", prelude + section], capture_output=True, text=True,
                                 timeout=120, env=env)
            for rel, data in before.items():
                self.assertEqual((root / rel).read_bytes(), data, f"{rel} was not restored")
            return run

    def for_each(self, check):
        for name in MUTATED_GAUNTLETS:
            with self.subTest(name):
                check(name)

    def test_failing_tests_kill_every_mutant(self):
        def check(name):
            run = self.run_mutants(name, "testfail")
            self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
            self.assertRegex(run.stdout, r"(?m)^killed: ")
            self.assertNotIn("SURVIVED", run.stdout)
        self.for_each(check)

    def test_a_passing_suite_is_a_survivor(self):
        def check(name):
            run = self.run_mutants(name, "pass")
            self.assertNotEqual(run.returncode, 0, run.stdout)
        self.for_each(check)

    def test_a_build_error_is_not_a_kill(self):
        def check(name):
            run = self.run_mutants(name, "builderror")
            self.assertNotEqual(run.returncode, 0, run.stdout + run.stderr)
            self.assertNotRegex(run.stdout, r"(?m)^killed")
            self.assertIn("RUN ERROR", run.stdout)
        self.for_each(check)

    def test_a_failure_without_a_failing_test_is_not_a_kill(self):
        def check(name):
            run = self.run_mutants(name, "nomarker")
            self.assertNotEqual(run.returncode, 0, run.stdout + run.stderr)
            self.assertNotRegex(run.stdout, r"(?m)^killed")
        self.for_each(check)

    def test_every_mutating_gauntlet_uses_the_shared_runner(self):
        for name in [*MUTATED_GAUNTLETS, *STATIC_MUTATED_GAUNTLETS]:
            with self.subTest(name):
                text = (TOOLS / name).read_text()
                self.assertRegex(text, r'(?m)^source "\$ROOT/scripts/lib/mutate\.sh"$')
                # mutate.sh sources restore-on-exit.sh; sourcing it again would drop the guarded files.
                self.assertNotIn("lib/restore-on-exit.sh", text)
                self.assertRegex(text, r"(?m)^ax_mutants_summary\b")
                self.assertNotRegex(text, r"(?m)^\s*kill_\w*\(\)")


FAKE_AX = """#!{python}
import http.server, os, sys, threading
port = int(sys.argv[sys.argv.index("--port") + 1])
with open(os.environ["FAKE_AX_PID"], "w") as out:
    out.write(str(os.getpid()))
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        with open(os.environ["FAKE_AX_LOG"], "a") as log:
            log.write(self.path + "\\n")
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"this checkout")
    def log_message(self, *args):
        pass
server = http.server.HTTPServer(("127.0.0.1", port), Handler)
threading.Timer(60, server.shutdown).start()
server.serve_forever()
"""
FAKE_NPX = """#!/bin/sh
echo "$* $AX_WEB_URL" >> "$FAKE_NPX_LOG"
"""
FAKE_NODE = """#!/bin/sh
cat >/dev/null
"""


def free_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def alive(pid):
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    return True


class CaretGauntletServerTest(unittest.TestCase):
    """gauntlet-md-editor-caret.sh and .ps1 start this checkout's ax web themselves."""

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.bin = self.tmp / "bin"
        self.bin.mkdir()
        for name, body in (("ax", FAKE_AX.format(python=sys.executable)), ("npx", FAKE_NPX),
                           ("node", FAKE_NODE)):
            path = self.bin / name
            path.write_text(body)
            path.chmod(0o755)
        self.pid_file = self.tmp / "ax.pid"
        self.requests = self.tmp / "requests.log"
        self.npx_log = self.tmp / "npx.log"
        self.addCleanup(self.stop_fake_ax)

    def stop_fake_ax(self):
        if self.pid_file.exists():
            try:
                os.kill(int(self.pid_file.read_text()), signal.SIGKILL)
            except (ProcessLookupError, ValueError):
                pass

    def env(self, **extra):
        env = {"PATH": f"{self.bin}:/usr/bin:/bin:/usr/sbin:/sbin", "HOME": str(self.tmp),
               "TMPDIR": str(self.tmp), "AX_BIN": str(self.bin / "ax"),
               "FAKE_AX_PID": str(self.pid_file), "FAKE_AX_LOG": str(self.requests),
               "FAKE_NPX_LOG": str(self.npx_log)}
        env.update(extra)
        return env

    def run_script(self, command, env):
        # Output goes to files, not pipes: a server left running would hold a pipe open
        # and make the run look as if it waited for the server to stop.
        out, err = self.tmp / "stdout", self.tmp / "stderr"
        with out.open("w") as stdout, err.open("w") as stderr:
            done = subprocess.run(command, env=env, stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr,
                                  timeout=120)
        return subprocess.CompletedProcess(command, done.returncode, out.read_text(), err.read_text())

    def run_sh(self, **extra):
        return self.run_script(["/bin/bash", str(TOOLS / "gauntlet-md-editor-caret.sh")], self.env(**extra))

    def run_ps1(self, **extra):
        pwsh = shutil.which("pwsh")
        if pwsh is None:
            self.skipTest("pwsh is not installed")
        env = self.env(**extra)
        env["PATH"] = f"{env['PATH']}:{Path(pwsh).parent}"
        return self.run_script([pwsh, "-NoProfile", "-File", str(TOOLS / "gauntlet-md-editor-caret.ps1")], env)

    def assert_used_and_stopped_own_server(self, run):
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        self.assertTrue(self.pid_file.exists(), "the checkout's ax web was not started")
        pid = int(self.pid_file.read_text())
        port = None
        for line in self.npx_log.read_text().splitlines():
            if line.startswith("playwright"):
                port = int(line.rsplit(":", 1)[1])
        self.assertIsNotNone(port, self.npx_log.read_text())
        with socket.socket() as sock:
            self.assertNotEqual(sock.connect_ex(("127.0.0.1", port)), 0, "ax web is still running")
        for _ in range(50):
            if not alive(pid):
                break
            time.sleep(0.1)
        self.assertFalse(alive(pid), "ax web is still running")

    def test_sh_starts_and_stops_its_own_server(self):
        self.assert_used_and_stopped_own_server(self.run_sh())

    def test_ps1_starts_and_stops_its_own_server(self):
        self.assert_used_and_stopped_own_server(self.run_ps1())

    def foreign_server(self):
        port = free_port()
        server = subprocess.Popen([sys.executable, "-m", "http.server", str(port), "--bind", "127.0.0.1"],
                                  stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, cwd=self.tmp)
        self.addCleanup(server.wait)
        self.addCleanup(server.kill)
        for _ in range(100):
            with socket.socket() as sock:
                if sock.connect_ex(("127.0.0.1", port)) == 0:
                    return port
            time.sleep(0.05)
        self.fail("the foreign server did not start")

    def test_sh_does_not_test_a_server_it_did_not_start(self):
        run = self.run_sh(AX_WEB_PORT=str(self.foreign_server()))
        self.assertNotEqual(run.returncode, 0, run.stdout)
        self.assertIn("already answers", run.stderr)
        self.assertFalse(self.pid_file.exists(), "ax web was started on a busy port")
        self.assertNotIn("playwright", self.npx_log.read_text() if self.npx_log.exists() else "")

    def test_ps1_does_not_test_a_server_it_did_not_start(self):
        run = self.run_ps1(AX_WEB_PORT=str(self.foreign_server()))
        self.assertNotEqual(run.returncode, 0, run.stdout)
        self.assertIn("already answers", run.stdout + run.stderr)
        self.assertFalse(self.pid_file.exists(), "ax web was started on a busy port")
        self.assertNotIn("playwright", self.npx_log.read_text() if self.npx_log.exists() else "")

    def test_ps1_stops_polling_once_the_server_answers(self):
        run = self.run_ps1(AX_WEB_PORT=str(free_port()))
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        polls = self.requests.read_text().splitlines()
        self.assertLessEqual(len(polls), 2, polls)


if __name__ == "__main__":
    unittest.main()
