#!/usr/bin/env python3
"""Stamp Chocolatey package placeholders from a GitHub release SHA256SUMS.

Works on macOS (no choco required). Used by scripts/chocolatey-prepare.ps1 and
unit tests.

Usage:
  python3 scripts/chocolatey_prepare.py --tag v8.4.3 [--sums-file PATH]
"""

from __future__ import annotations

import argparse
import re
import sys
import urllib.request
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
PKG_ROOT = REPO_ROOT / "packaging" / "chocolatey" / "ax-agent"
DEFAULT_REPO = "GaryWenneker/ax"

PLACEHOLDER_FILES = [
    PKG_ROOT / "ax-agent.nuspec",
    PKG_ROOT / "tools" / "chocolateyInstall.ps1",
    PKG_ROOT / "tools" / "VERIFICATION.txt",
]


def parse_sha256sums(text: str) -> dict[str, str]:
    """Parse `SHA256SUMS` lines into {filename: lowercase hex}."""
    out: dict[str, str] = {}
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        # GNU coreutils: "<sha256><two spaces><name>"
        m = re.match(r"^([0-9a-fA-F]{64})\s+(\S+)\s*$", line)
        if not m:
            raise ValueError(f"unrecognized SHA256SUMS line: {line!r}")
        out[Path(m.group(2)).name] = m.group(1).lower()
    return out


def normalize_tag(tag: str) -> tuple[str, str]:
    tag = tag.strip()
    if not tag:
        raise ValueError("tag is empty")
    version = tag[1:] if tag.startswith("v") else tag
    if not re.match(r"^\d+\.\d+\.\d+", version):
        raise ValueError(f"expected semver tag, got {tag!r}")
    return f"v{version}", version


def fetch_sums(tag: str, repo: str) -> str:
    url = f"https://github.com/{repo}/releases/download/{tag}/SHA256SUMS"
    with urllib.request.urlopen(url, timeout=60) as resp:  # noqa: S310 — fixed GitHub URL
        return resp.read().decode("utf-8")


def required_checksums(sums: dict[str, str]) -> dict[str, str]:
    need = ("ax-win32-x64.zip", "ax-win32-arm64.zip")
    missing = [n for n in need if n not in sums]
    if missing:
        raise KeyError(f"SHA256SUMS missing {missing}; have {sorted(sums)}")
    return {n: sums[n] for n in need}


def stamp_files(
    version: str,
    sha_x64: str,
    sha_arm64: str,
    files: list[Path] | None = None,
) -> list[Path]:
    replacements = {
        "__AX_VERSION__": version,
        "__AX_SHA256_WIN32_X64__": sha_x64,
        "__AX_SHA256_WIN32_ARM64__": sha_arm64,
    }
    touched: list[Path] = []
    for path in files or PLACEHOLDER_FILES:
        text = path.read_text(encoding="utf-8")
        original = text
        for key, value in replacements.items():
            text = text.replace(key, value)
        if "__AX_" in text:
            leftover = sorted(set(re.findall(r"__AX_[A-Z0-9_]+__", text)))
            raise RuntimeError(f"{path} still has placeholders: {leftover}")
        if text != original:
            path.write_text(text, encoding="utf-8", newline="\n")
            touched.append(path)
    return touched


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True, help="Release tag, e.g. v8.4.3")
    parser.add_argument("--repo", default=DEFAULT_REPO)
    parser.add_argument(
        "--sums-file",
        type=Path,
        help="Local SHA256SUMS instead of downloading",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Parse and print replacements without writing",
    )
    args = parser.parse_args(argv)

    tag, version = normalize_tag(args.tag)
    raw = (
        args.sums_file.read_text(encoding="utf-8")
        if args.sums_file
        else fetch_sums(tag, args.repo)
    )
    checksums = required_checksums(parse_sha256sums(raw))
    print(f"tag={tag} version={version}")
    for name, digest in checksums.items():
        print(f"  {name} {digest}")

    if args.dry_run:
        return 0

    touched = stamp_files(
        version,
        checksums["ax-win32-x64.zip"],
        checksums["ax-win32-arm64.zip"],
    )
    for path in touched:
        print(f"stamped {path.relative_to(REPO_ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
