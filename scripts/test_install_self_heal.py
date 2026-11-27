#!/usr/bin/env python3
"""Contract tests for installer automatic self-heal (docs/specs/install-auto-self-heal.md)."""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INSTALL_PS1 = ROOT / "install.ps1"
INSTALL_SH = ROOT / "install.sh"
SITE_PS1 = ROOT / "site" / "public" / "install.ps1"
SITE_SH = ROOT / "site" / "public" / "install.sh"


def _read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


class InstallSelfHealContract(unittest.TestCase):
    def test_public_copies_match_root(self) -> None:
        self.assertEqual(_read(INSTALL_PS1), _read(SITE_PS1))
        self.assertEqual(_read(INSTALL_SH), _read(SITE_SH))

    def test_ps1_defines_pending_upgrade_clear(self) -> None:
        text = _read(INSTALL_PS1)
        self.assertIn("function Clear-AxPendingUpgradeState", text)
        self.assertIn("upgrade-status.txt", text)
        self.assertIn("upgrade-staging-*", text)
        self.assertIn("current.old", text)
        self.assertIn("Clear-AxPendingUpgradeState -InstallRoot $installDir", text)
        # The working install is only touched after the version resolves and the download succeeds.
        resolve_at = text.index("$version = Resolve-AxVersion")
        calls = list(re.finditer(r"^(Switch-AxInstallTree -NewTree|Clear-AxPendingUpgradeState)\b", text,
                                 flags=re.MULTILINE))
        self.assertTrue(calls, "Switch-AxInstallTree must be invoked")
        for call in calls:
            self.assertLess(resolve_at, call.start(), call.group(0))

    def test_ps1_skips_rejected_github_tokens_without_changing_the_environment(self) -> None:
        # The token is skipped for this install only; User and Machine variables stay as they are.
        text = _read(INSTALL_PS1)
        self.assertIn("function Skip-AxGitHubToken", text)
        self.assertNotIn("ClearUserScope", text)
        # Rate limits (HTTP 403) are recognized before the auth-failure branch.
        self.assertLess(text.index("-match 'rate limit'"), text.index("Bad credentials'"))

    def test_sh_swaps_install_only_after_resolve(self) -> None:
        text = _read(INSTALL_SH)
        self.assertIn("clear_pending_upgrade_state()", text)
        self.assertIn("upgrade-status.txt", text)
        self.assertIn("clear_pending_upgrade_state", text)
        self.assertIn("skip_github_token", text)
        self.assertNotIn("disable_stale_github_tokens", text)
        # The working install is only touched after the version resolves and the download succeeds.
        invoke_matches = list(
            re.finditer(r"^swap_install_tree ", text, flags=re.MULTILINE)
        )
        self.assertTrue(invoke_matches, "swap_install_tree must be invoked")
        resolve_at = text.index('version="$(resolve_version)"')
        self.assertLess(resolve_at, invoke_matches[-1].start())


def _function(text: str, pattern: str) -> str:
    match = re.search(pattern + r".*?^\}\n", text, flags=re.MULTILINE | re.DOTALL)
    if not match:
        raise AssertionError(f"function not found: {pattern}")
    return match.group(0)


def _fixture(root: Path) -> None:
    (root / "upgrade-status.txt").write_text("park current install: Access is denied.\n", encoding="utf-8")
    staging = root / "upgrade-staging-9"
    (staging / "bin").mkdir(parents=True)
    (staging / "bin" / "ax.exe").write_text("x", encoding="utf-8")
    parked = root / "current.old"
    parked.mkdir()
    (parked / "ax.exe").write_text("y", encoding="utf-8")
    # Working install and user data must survive.
    (root / "current").mkdir()
    (root / "current" / "ax.exe").write_text("live", encoding="utf-8")
    (root / "global.db").write_text("data", encoding="utf-8")


class PendingUpgradeCleanup(unittest.TestCase):
    """Runs the installers' own cleanup functions against a fixture install root."""

    def assert_cleaned(self, root: Path) -> None:
        self.assertFalse((root / "upgrade-status.txt").exists())
        self.assertFalse((root / "upgrade-staging-9").exists())
        self.assertFalse((root / "current.old").exists())
        self.assertEqual((root / "current" / "ax.exe").read_text(encoding="utf-8"), "live")
        self.assertEqual((root / "global.db").read_text(encoding="utf-8"), "data")

    def test_install_sh_clears_pending_upgrade_state(self) -> None:
        func = _function(_read(INSTALL_SH), r"^clear_pending_upgrade_state\(\) \{")
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            _fixture(root)
            out = subprocess.run(["sh", "-c", func + "clear_pending_upgrade_state\n"],
                                 env={**os.environ, "INSTALL_DIR": str(root)},
                                 capture_output=True, text=True, timeout=60)
            self.assertEqual(out.returncode, 0, out.stderr)
            self.assert_cleaned(root)

    @unittest.skipUnless(shutil.which("pwsh"), "pwsh is not installed")
    def test_install_ps1_clears_pending_upgrade_state(self) -> None:
        func = _function(_read(INSTALL_PS1), r"^function Clear-AxPendingUpgradeState \{")
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            _fixture(root)
            script = func + "Clear-AxPendingUpgradeState -InstallRoot $env:AX_TEST_ROOT\n"
            out = subprocess.run([shutil.which("pwsh"), "-NoProfile", "-NonInteractive", "-Command", script],
                                 env={**os.environ, "AX_TEST_ROOT": str(root)},
                                 capture_output=True, text=True, timeout=60)
            self.assertEqual(out.returncode, 0, out.stderr)
            self.assert_cleaned(root)

def _stale_park_fixture(root: Path) -> None:
    # A parked tree from an earlier install that could not be removed.
    (root / "current.old").mkdir()
    (root / "current.old" / "ax.exe").write_text("stale", encoding="utf-8")
    (root / "current").mkdir()
    (root / "current" / "ax.exe").write_text("live", encoding="utf-8")


class SwapWithStaleParkedTree(unittest.TestCase):
    """A failed activation restores exactly the tree that was live, even over a stale current.old."""

    def assert_live_restored(self, root: Path) -> None:
        self.assertEqual((root / "current" / "ax.exe").read_text(encoding="utf-8"), "live")
        self.assertFalse((root / "current" / "current").exists())

    def test_install_sh_restores_the_live_tree(self) -> None:
        func = _function(_read(INSTALL_SH), r"^swap_install_tree\(\) \{")
        stubs = "stop_ax_processes() { :; }\nclear_pending_upgrade_state() { :; }\n"
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            _stale_park_fixture(root)
            out = subprocess.run(["sh", "-c", stubs + func + 'swap_install_tree "$INSTALL_DIR/missing-new-tree"\n'],
                                 env={**os.environ, "INSTALL_DIR": str(root)},
                                 capture_output=True, text=True, timeout=60)
            self.assertNotEqual(out.returncode, 0)
            self.assert_live_restored(root)

    def test_install_sh_restores_the_live_tree_when_the_stale_tree_cannot_be_removed(self) -> None:
        func = _function(_read(INSTALL_SH), r"^swap_install_tree\(\) \{")
        stubs = "stop_ax_processes() { :; }\nclear_pending_upgrade_state() { :; }\n"
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            _stale_park_fixture(root)
            # A read-only directory keeps its files: rm -rf fails, as on a locked tree.
            (root / "current.old").chmod(0o555)
            try:
                out = subprocess.run(["sh", "-c", stubs + func + 'swap_install_tree "$INSTALL_DIR/missing-new-tree"\n'],
                                     env={**os.environ, "INSTALL_DIR": str(root)},
                                     capture_output=True, text=True, timeout=60)
            finally:
                (root / "current.old").chmod(0o755)
            self.assertNotEqual(out.returncode, 0)
            self.assert_live_restored(root)
            self.assertEqual((root / "current.old" / "ax.exe").read_text(encoding="utf-8"), "stale")

    @unittest.skipUnless(shutil.which("pwsh"), "pwsh is not installed")
    def test_install_ps1_restores_the_live_tree_when_the_stale_tree_cannot_be_removed(self) -> None:
        func = _function(_read(INSTALL_PS1), r"^function Switch-AxInstallTree \{")
        stubs = "function Stop-AxProcesses {}\nfunction Clear-AxPendingUpgradeState { param($InstallRoot) }\n"
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            _stale_park_fixture(root)
            (root / "current.old").chmod(0o555)
            try:
                script = (stubs + func + "$installDir = $env:AX_TEST_ROOT\n"
                          "try { Switch-AxInstallTree -NewTree (Join-Path $installDir 'missing-new-tree')"
                          " -Live (Join-Path $installDir 'current'); exit 0 } catch { Write-Host $_; exit 3 }\n")
                out = subprocess.run([shutil.which("pwsh"), "-NoProfile", "-NonInteractive", "-Command", script],
                                     env={**os.environ, "AX_TEST_ROOT": str(root)},
                                     capture_output=True, text=True, timeout=60)
            finally:
                (root / "current.old").chmod(0o755)
            self.assertEqual(out.returncode, 3, out.stdout + out.stderr)
            self.assert_live_restored(root)
            self.assertEqual((root / "current.old" / "ax.exe").read_text(encoding="utf-8"), "stale")

    def test_install_sh_activates_over_a_stale_parked_tree(self) -> None:
        func = _function(_read(INSTALL_SH), r"^swap_install_tree\(\) \{")
        stubs = "stop_ax_processes() { :; }\nclear_pending_upgrade_state() { :; }\n"
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            _stale_park_fixture(root)
            (root / "new").mkdir()
            (root / "new" / "ax.exe").write_text("new", encoding="utf-8")
            out = subprocess.run(["sh", "-c", stubs + func + 'swap_install_tree "$INSTALL_DIR/new"\n'],
                                 env={**os.environ, "INSTALL_DIR": str(root)},
                                 capture_output=True, text=True, timeout=60)
            self.assertEqual(out.returncode, 0, out.stderr)
            self.assertEqual((root / "current" / "ax.exe").read_text(encoding="utf-8"), "new")
            self.assertEqual(sorted(p.name for p in root.iterdir()), ["current"])

    @unittest.skipUnless(shutil.which("pwsh"), "pwsh is not installed")
    def test_install_ps1_restores_the_live_tree(self) -> None:
        func = _function(_read(INSTALL_PS1), r"^function Switch-AxInstallTree \{")
        # The stub leaves current.old in place, as a locked file would.
        stubs = "function Stop-AxProcesses {}\nfunction Clear-AxPendingUpgradeState { param($InstallRoot) }\n"
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            _stale_park_fixture(root)
            script = (stubs + func + "$installDir = $env:AX_TEST_ROOT\n"
                      "try { Switch-AxInstallTree -NewTree (Join-Path $installDir 'missing-new-tree')"
                      " -Live (Join-Path $installDir 'current'); exit 0 } catch { Write-Host $_; exit 3 }\n")
            out = subprocess.run([shutil.which("pwsh"), "-NoProfile", "-NonInteractive", "-Command", script],
                                 env={**os.environ, "AX_TEST_ROOT": str(root)},
                                 capture_output=True, text=True, timeout=60)
            self.assertEqual(out.returncode, 3, out.stdout + out.stderr)
            self.assert_live_restored(root)


if __name__ == "__main__":
    unittest.main()
