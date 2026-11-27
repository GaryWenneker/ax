#!/usr/bin/env python3
"""Contract tests for the standalone Docker build (Dockerfile.standalone).

The build context must leave out local build output, and every base image
must name a fixed version so two builds of one commit use the same toolchain.
"""

from __future__ import annotations

import fnmatch
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DOCKERFILE = ROOT / "Dockerfile.standalone"
DOCKERIGNORE = ROOT / ".dockerignore"


def ignore_patterns() -> list[str]:
    return [
        line.strip()
        for line in DOCKERIGNORE.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.startswith("#") and not line.startswith("!")
    ]


def ignored(path: str) -> bool:
    """True when a .dockerignore pattern drops `path` (or a parent of it) from the context."""
    parts = path.split("/")
    prefixes = ["/".join(parts[: i + 1]) for i in range(len(parts))]
    for pattern in ignore_patterns():
        for prefix in prefixes:
            if fnmatch.fnmatchcase(prefix, pattern):
                return True
            if pattern.startswith("**/") and fnmatch.fnmatchcase(prefix.split("/")[-1], pattern[3:]):
                return True
    return False


def cargo_target_dir() -> str:
    text = (ROOT / ".cargo" / "config.toml").read_text(encoding="utf-8")
    m = re.search(r'(?m)^\s*target-dir\s*=\s*"([^"]+)"', text)
    return m.group(1) if m else "target"


class BuildContext(unittest.TestCase):
    def test_local_build_output_stays_out_of_the_context(self) -> None:
        for path in [
            cargo_target_dir() + "/release/ax",
            "target-dev/debug/ax",
            "target-ui/release/ax",
            "target-linux/release/ax",
            "crates/ax-web/web-ui/target-dev/debug/x",
            "dist/ax.zip",
            "dist-ci/ax.zip",
        ]:
            self.assertTrue(ignored(path), f".dockerignore must exclude {path}")

    def test_sources_stay_in_the_context(self) -> None:
        for path in ["Cargo.toml", "Cargo.lock", "crates/ax-cli/src/main.rs", "crates/ax-web/web-ui/package.json"]:
            self.assertFalse(ignored(path), f".dockerignore must keep {path}")


class BaseImages(unittest.TestCase):
    # A full x.y.z version, a dated Debian snapshot, or a content digest.
    FIXED = re.compile(r"(?:\d+\.\d+\.\d+|-\d{8}(?:-|$)|@sha256:[0-9a-f]{64})")

    def test_every_base_image_names_a_fixed_version(self) -> None:
        text = DOCKERFILE.read_text(encoding="utf-8")
        images = re.findall(r"(?im)^FROM\s+(?:--platform=\S+\s+)?(\S+)", text)
        self.assertTrue(images)
        for image in images:
            self.assertRegex(image, self.FIXED, f"{DOCKERFILE.name}: pin {image} to a fixed version")


if __name__ == "__main__":
    unittest.main()
