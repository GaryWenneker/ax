"""Behavior of the headless container entry points in slim/.

ax-serve-repo.sh and ax-index-repo.sh run against a local upstream repository that is
reached through a token-bearing https URL (rewritten by git's insteadOf), with a fake
`ax` on PATH. create-job.sh runs with a fake `az` that records every call.
"""
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SLIM = ROOT / "slim"
TOKEN = "ghp_SECRETTOKEN123"
REPO_URL = f"https://x-access-token:{TOKEN}@example.invalid/org/private"
CLEAN_URL = "https://example.invalid/org/private"

FAKE_AX = """#!/bin/sh
case "$1" in
  init) ;;
  mcp) git rev-parse HEAD > "$CAPTURE" ;;
esac
"""


def git(cwd, *args):
    return subprocess.run(["git", *args], cwd=cwd, check=True, capture_output=True, text=True).stdout.strip()


class CheckoutTestBase(unittest.TestCase):
    def setUp(self):
        if shutil.which("git") is None or shutil.which("sha256sum") is None:
            self.skipTest("git and sha256sum are required")
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        self.upstream = self.tmp / "upstream"
        self.upstream.mkdir()
        git(self.upstream, "init", "--quiet", "--initial-branch=main")
        git(self.upstream, "config", "user.email", "t@example.invalid")
        git(self.upstream, "config", "user.name", "t")
        self.commit("one")
        git(self.upstream, "branch", "release")
        git(self.upstream, "checkout", "--quiet", "release")
        self.commit("release")
        git(self.upstream, "checkout", "--quiet", "main")
        bin_dir = self.tmp / "bin"
        bin_dir.mkdir()
        (bin_dir / "ax").write_text(FAKE_AX)
        (bin_dir / "ax").chmod(0o755)
        self.capture = self.tmp / "head.txt"
        self.workdir = self.tmp / "work"
        (self.tmp / "home").mkdir()
        self.env = {
            "PATH": f"{bin_dir}{os.pathsep}{os.environ['PATH']}",
            "HOME": str(self.tmp / "home"),
            "CAPTURE": str(self.capture),
            "AX_REPO_URL": REPO_URL,
            "AX_WORKDIR": str(self.workdir),
            "AX_API_KEY": "k" * 40,
            "AX_PUBLIC_URL": "https://ax.example.test",
            "AX_JOB_COMMAND": 'git rev-parse HEAD > "$CAPTURE"',
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_GLOBAL": "/dev/null",
            "GIT_CONFIG_COUNT": "1",
            "GIT_CONFIG_KEY_0": f"url.{self.upstream.as_uri()}.insteadOf",
            "GIT_CONFIG_VALUE_0": REPO_URL,
        }

    def commit(self, name):
        (self.upstream / f"{name}.txt").write_text(name)
        git(self.upstream, "add", ".")
        git(self.upstream, "commit", "--quiet", "-m", name)
        return git(self.upstream, "rev-parse", "HEAD")

    def run_script(self, script, ref=None):
        env = dict(self.env)
        if ref:
            env["AX_REPO_REF"] = ref
        self.capture.unlink(missing_ok=True)
        run = subprocess.run(["sh", str(SLIM / script)], env=env, capture_output=True, text=True, timeout=60)
        self.assertEqual(run.returncode, 0, run.stderr)
        return self.capture.read_text().strip()

    def check_restart_follows_ref(self, script):
        self.assertEqual(self.run_script(script, "main"), git(self.upstream, "rev-parse", "main"))
        newer = self.commit("two")
        self.assertEqual(self.run_script(script, "main"), newer, "a restart must fetch new commits")
        self.assertEqual(self.run_script(script, "release"), git(self.upstream, "rev-parse", "release"),
                         "a restart must follow a changed AX_REPO_REF")
        self.assertEqual(self.run_script(script), newer, "no AX_REPO_REF means the default branch")

    def check_token_not_stored(self, script):
        self.run_script(script, "main")
        self.run_script(script, "release")
        for path in (self.workdir / ".git").rglob("*"):
            if path.is_file():
                self.assertNotIn(TOKEN.encode(), path.read_bytes(), f"token stored in {path}")
        self.assertEqual(git(self.workdir, "remote", "get-url", "origin"), CLEAN_URL)


class ServeRepoTest(CheckoutTestBase):
    def test_restart_fetches_and_follows_ref(self):
        self.check_restart_follows_ref("ax-serve-repo.sh")

    def test_token_is_not_stored_in_the_checkout(self):
        self.check_token_not_stored("ax-serve-repo.sh")


class IndexRepoTest(CheckoutTestBase):
    def test_restart_fetches_and_follows_ref(self):
        self.check_restart_follows_ref("ax-index-repo.sh")

    def test_token_is_not_stored_in_the_checkout(self):
        self.check_token_not_stored("ax-index-repo.sh")


class CreateJobHelpTest(unittest.TestCase):
    def test_help_prints_only_the_usage_comment(self):
        run = subprocess.run(["sh", str(SLIM / "azure" / "create-job.sh"), "--help"],
                             capture_output=True, text=True, timeout=30)
        self.assertEqual(run.returncode, 0, run.stderr)
        self.assertIn("--resource-group RG", run.stdout)
        self.assertIn("--start", run.stdout)
        self.assertNotIn("set -eu", run.stdout)
        self.assertNotIn('NAME="ax-index"', run.stdout)
        self.assertNotIn("#!/bin/sh", run.stdout)


FAKE_AZ = """#!/bin/sh
# Records argv, and the content of every @file argument the way the Azure CLI loads it.
printf '%s\\n' "$*" >> "$AZ_LOG"
for arg in "$@"; do
  case "$arg" in @*) { cat "${arg#@}"; echo; } >> "$AZ_FILES" ;; esac
done
case "$*" in
  "containerapp job show"*) [ "${JOB_EXISTS:-0}" = 1 ] ;;
  *) exit 0 ;;
esac
"""


class CreateJobSecretTest(unittest.TestCase):
    def run_job(self, args, job_exists, env_url=None):
        tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, tmp, ignore_errors=True)
        (tmp / "bin").mkdir()
        (tmp / "bin" / "az").write_text(FAKE_AZ)
        (tmp / "bin" / "az").chmod(0o755)
        (tmp / "t").mkdir()
        env = {
            "PATH": f"{tmp / 'bin'}{os.pathsep}{os.environ['PATH']}",
            "AZ_LOG": str(tmp / "argv.log"),
            "AZ_FILES": str(tmp / "files.log"),
            "JOB_EXISTS": "1" if job_exists else "0",
            "TMPDIR": str(tmp / "t"),
        }
        if env_url:
            env["AX_REPO_URL"] = env_url
        run = subprocess.run(
            ["sh", str(SLIM / "azure" / "create-job.sh"), "--resource-group", "rg", "--environment", "env",
             "--version", "v1.2.3", *args],
            env=env, capture_output=True, text=True, timeout=30,
        )
        self.assertEqual(run.returncode, 0, run.stderr)
        argv = (tmp / "argv.log").read_text()
        files = (tmp / "files.log").read_text() if (tmp / "files.log").exists() else ""
        self.assertEqual(list((tmp / "t").iterdir()), [], "the secret file must be removed")
        return argv, files

    def check(self, args, env_url=None):
        for job_exists in (False, True):
            argv, files = self.run_job(args, job_exists, env_url)
            self.assertNotIn(TOKEN, argv, "the repository token must not be on the az command line")
            self.assertIn(f"ax-repo-url={REPO_URL}", files.splitlines())
            self.assertIn("AX_REPO_URL=secretref:ax-repo-url", argv)

    def test_repo_url_from_environment_never_reaches_argv(self):
        self.check([], env_url=REPO_URL)

    def test_repo_flag_keeps_the_token_off_the_az_command_line(self):
        self.check(["--repo", REPO_URL])


if __name__ == "__main__":
    unittest.main()
