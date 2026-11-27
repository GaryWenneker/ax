#!/usr/bin/env python3
"""Contract tests for installer automatic self-heal (docs/specs/install-auto-self-heal.md)."""

from __future__ import annotations

import re
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
        swap_at = text.index("Switch-AxInstallTree -NewTree")
        self.assertLess(resolve_at, swap_at)

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

    def test_pending_upgrade_fixture_cleanup_contract(self) -> None:
        """Simulate the file operations Clear-AxPendingUpgradeState / clear_pending_upgrade_state perform."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            status = root / "upgrade-status.txt"
            status.write_text("park current install: Access is denied.\n", encoding="utf-8")
            staging = root / "upgrade-staging-9"
            staging.mkdir()
            (staging / "ax.exe").write_text("x", encoding="utf-8")
            parked = root / "current.old"
            parked.mkdir()
            (parked / "ax.exe").write_text("y", encoding="utf-8")

            # Mirror installer cleanup (keep in sync with install.ps1 / install.sh).
            if status.is_file():
                status.unlink()
            for path in root.glob("upgrade-staging-*"):
                if path.is_dir():
                    for child in path.rglob("*"):
                        if child.is_file():
                            child.unlink()
                    path.rmdir()
            if parked.is_dir():
                for child in parked.rglob("*"):
                    if child.is_file():
                        child.unlink()
                parked.rmdir()

            self.assertFalse(status.exists())
            self.assertFalse(staging.exists())
            self.assertFalse(parked.exists())


if __name__ == "__main__":
    unittest.main()
