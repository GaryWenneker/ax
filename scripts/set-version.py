#!/usr/bin/env python3
"""Sets one version on every workspace crate and on the Command Center web UI package.

  python3 scripts/set-version.py 10.1.0

Then run `cargo update --workspace --offline` so Cargo.lock records the new crate versions.
Fails when a file does not carry exactly one version declaration in the expected place.
"""

import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
PACKAGE_VERSION = re.compile(r'(?m)^(\[package\]\n(?:[^\[\n].*\n)*?)version = "[^"]+"')


def set_crate(path: pathlib.Path, version: str) -> None:
    text = path.read_text(encoding="utf-8")
    updated, count = PACKAGE_VERSION.subn(rf'\g<1>version = "{version}"', text, count=1)
    if count != 1:
        sys.exit(f"FAIL: no [package] version in {path}")
    path.write_text(updated, encoding="utf-8")


def set_package(path: pathlib.Path, version: str, lock: bool) -> None:
    data = json.loads(path.read_text(encoding="utf-8"))
    data["version"] = version
    if lock:
        data["packages"][""]["version"] = version
    path.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def main() -> None:
    if len(sys.argv) != 2 or not re.fullmatch(r"\d+\.\d+\.\d+", sys.argv[1]):
        sys.exit("usage: set-version.py MAJOR.MINOR.PATCH")
    version = sys.argv[1]
    crates = sorted((ROOT / "crates").glob("*/Cargo.toml"))
    if not crates:
        sys.exit("FAIL: no crates found")
    for path in crates:
        set_crate(path, version)
    ui = ROOT / "crates/ax-web/web-ui"
    set_package(ui / "package.json", version, lock=False)
    set_package(ui / "package-lock.json", version, lock=True)
    print(f"set {version} on {len(crates)} crates and the web UI package")


if __name__ == "__main__":
    main()
