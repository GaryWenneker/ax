#!/usr/bin/env python3
"""Tests install-slim.sh against a local release directory (no network).

A stub `uname` reports Linux x86_64, so the tests also run on macOS.
"""

from __future__ import annotations

import hashlib
import os
import shutil
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INSTALLER = ROOT / "install-slim.sh"
TAG = "v0.0.0-test"
ASSET = "ax-slim-linux-x64.tar.gz"

WORKING_AX = "#!/bin/sh\n[ \"$1\" = version ] && echo 'ax 0.0.1 (old)' && exit 0\nexit 1\n"
GOOD_AX = "#!/bin/sh\n[ \"$1\" = version ] && echo 'ax 0.0.0-test' && exit 0\nexit 1\n"
BROKEN_AX = "#!/bin/sh\necho 'Exec format error' >&2\nexit 126\n"


class InstallSlim(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.work = Path(self.tmp.name)
        self.bin = self.work / "bin"
        self.bin.mkdir()
        self.stubs = self.work / "stubs"
        self.stubs.mkdir()
        uname = self.stubs / "uname"
        uname.write_text('#!/bin/sh\ncase "$1" in -m) echo x86_64 ;; *) echo Linux ;; esac\n')
        uname.chmod(0o755)
        self.releases = self.work / "releases"
        (self.releases / TAG).mkdir(parents=True)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def _publish(self, ax_script: str) -> None:
        stage = self.work / "stage" / "ax-slim-linux-x64"
        shutil.rmtree(stage.parent, ignore_errors=True)
        stage.mkdir(parents=True)
        (stage / "ax").write_text(ax_script)
        (stage / "ax").chmod(0o755)
        archive = self.releases / TAG / ASSET
        with tarfile.open(archive, "w:gz") as tar:
            tar.add(stage, arcname="ax-slim-linux-x64")
        digest = hashlib.sha256(archive.read_bytes()).hexdigest()
        (self.releases / TAG / "SHA256SUMS").write_text(f"{digest}  {ASSET}\n")

    def _install(self) -> subprocess.CompletedProcess[str]:
        env = dict(os.environ)
        env.update(
            PATH=f"{self.stubs}:{env['PATH']}",
            AX_SLIM_DOWNLOAD_BASE=f"file://{self.releases}",
            AX_GITHUB_API=f"file://{self.work}/no-api",
            GITHUB_TOKEN="",
            GH_TOKEN="",
        )
        return subprocess.run(
            ["sh", str(INSTALLER), "--version", TAG, "--bin-dir", str(self.bin)],
            capture_output=True, text=True, env=env, timeout=60,
        )

    def test_installs_a_binary_that_runs(self) -> None:
        self._publish(GOOD_AX)
        result = self._install()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.bin / "ax").read_text(), GOOD_AX)
        self.assertFalse((self.bin / "ax.new").exists())

    def test_a_binary_that_does_not_run_leaves_the_working_install(self) -> None:
        (self.bin / "ax").write_text(WORKING_AX)
        (self.bin / "ax").chmod(0o755)
        self._publish(BROKEN_AX)
        result = self._install()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("does not run on this host", result.stderr)
        self.assertEqual((self.bin / "ax").read_text(), WORKING_AX, "the working ax was replaced")
        self.assertFalse((self.bin / "ax.new").exists(), "the staged binary was left behind")


if __name__ == "__main__":
    unittest.main()
