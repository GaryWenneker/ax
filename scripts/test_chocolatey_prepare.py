#!/usr/bin/env python3
"""Unit tests for Chocolatey prepare/stamp helpers (runs on macOS)."""

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from chocolatey_prepare import (
    normalize_tag,
    parse_sha256sums,
    required_checksums,
    stamp_files,
)

SAMPLE = """\
cc93d6991c998df32d9a23dd5ef0641af5283a0e7db069a6eb573ee3935b37aa  ax-win32-x64.zip
01fe8c1e60359c4a9caba355347009dd32ad7a20bd6e499bbe010eef381651d5  ax-win32-arm64.zip
9cb1ebc4c22a85c3a01da2421a9d92f9682e51732416b36e17ffb37d5defa74d  ax-darwin-arm64.tar.gz
"""


class ChocolateyPrepareTests(unittest.TestCase):
    def test_parse_and_require_windows_zips(self) -> None:
        sums = parse_sha256sums(SAMPLE)
        need = required_checksums(sums)
        self.assertEqual(
            need["ax-win32-x64.zip"],
            "cc93d6991c998df32d9a23dd5ef0641af5283a0e7db069a6eb573ee3935b37aa",
        )
        self.assertEqual(
            need["ax-win32-arm64.zip"],
            "01fe8c1e60359c4a9caba355347009dd32ad7a20bd6e499bbe010eef381651d5",
        )

    def test_normalize_tag(self) -> None:
        self.assertEqual(normalize_tag("v8.4.3"), ("v8.4.3", "8.4.3"))
        self.assertEqual(normalize_tag("8.4.3"), ("v8.4.3", "8.4.3"))

    def test_stamp_replaces_all_placeholders(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "sample.nuspec"
            path.write_text(
                "version=__AX_VERSION__ x64=__AX_SHA256_WIN32_X64__ "
                "arm=__AX_SHA256_WIN32_ARM64__\n",
                encoding="utf-8",
            )
            stamp_files(
                "8.4.3",
                "aa" * 32,
                "bb" * 32,
                files=[path],
            )
            text = path.read_text(encoding="utf-8")
            self.assertIn("version=8.4.3", text)
            self.assertIn("x64=" + "aa" * 32, text)
            self.assertIn("arm=" + "bb" * 32, text)
            self.assertNotIn("__AX_", text)

    def test_missing_windows_asset_fails_closed(self) -> None:
        with self.assertRaises(KeyError):
            required_checksums(
                parse_sha256sums(
                    "9cb1ebc4c22a85c3a01da2421a9d92f9682e51732416b36e17ffb37d5defa74d  "
                    "ax-darwin-arm64.tar.gz\n"
                )
            )


if __name__ == "__main__":
    unittest.main()
