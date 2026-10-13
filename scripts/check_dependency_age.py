#!/usr/bin/env python3
"""Enforce a seven-day publication age for newly locked crates and npm packages.

Compares the proposed lockfiles with the target branch. Existing versions are
grandfathered; adding or upgrading to an immature release fails closed.
Uses only Python's standard library and public package registry metadata.
"""

import argparse
import json
import os
import subprocess
import sys
import time
import tomllib
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timedelta, timezone
from pathlib import Path

MINIMUM_AGE = timedelta(days=7)
NPM_LOCKFILES = (
    "site/package-lock.json",
    "telemetry-worker/package-lock.json",
    "crates/ax-web/web-ui/package-lock.json",
)
CARGO_LOCKFILE = "Cargo.lock"
USER_AGENT = "ax-dependency-lifecycle/1.0 (https://github.com/GaryWenneker/ax)"


class PolicyError(Exception):
    """A registry lookup or release-age policy failure."""


def read_base_file(base: str, path: str) -> bytes | None:
    result = subprocess.run(
        ["git", "show", f"{base}:{path}"], capture_output=True, check=False
    )
    if result.returncode == 0:
        return result.stdout
    # Missing files on the base branch are valid for newly created lockfiles.
    exists = subprocess.run(
        ["git", "cat-file", "-e", f"{base}:{path}"], capture_output=True, check=False
    )
    if exists.returncode == 0:
        raise PolicyError(f"Cannot read {base}:{path}")
    return None


def npm_packages(content: bytes | None) -> set[tuple[str, str]]:
    if not content:
        return set()
    doc = json.loads(content)
    if not isinstance(doc.get("packages"), dict):
        raise PolicyError("Expected a modern npm package-lock.json with a packages map")
    versions = set()
    for path, record in doc["packages"].items():
        if not path or not isinstance(record, dict) or record.get("link"):
            continue
        version = record.get("version")
        if not isinstance(version, str) or version.startswith(("file:", "workspace:")):
            continue
        # Nested modules and scoped package names preserve their npm identity.
        name = record.get("name") or path.rsplit("node_modules/", 1)[-1]
        resolved = record.get("resolved", "")
        if resolved and not resolved.startswith("https://registry.npmjs.org/"):
            # Non-registry dependencies have no trustworthy npm publication age.
            # Fail closed when introduced rather than silently exempting them.
            versions.add((f"!unsupported-source:{name}", version))
            continue
        versions.add((name, version))
    return versions


def cargo_packages(content: bytes | None) -> set[tuple[str, str]]:
    if not content:
        return set()
    doc = tomllib.loads(content.decode("utf-8"))
    versions = set()
    for package in doc.get("package", []):
        source = package.get("source", "")
        if not source:
            continue  # Workspace/path dependencies are not published releases.
        if source != "registry+https://github.com/rust-lang/crates.io-index":
            versions.add((f"!unsupported-source:{package['name']}", package["version"]))
        else:
            versions.add((package["name"], package["version"]))
    return versions


def fetch_json(url: str) -> dict:
    request = urllib.request.Request(
        url, headers={"Accept": "application/json", "User-Agent": USER_AGENT}
    )
    for attempt in range(3):
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                return json.load(response)
        except urllib.error.HTTPError as error:
            if error.code in (429, 502, 503, 504) and attempt < 2:
                time.sleep(2 ** attempt)
                continue
            raise PolicyError(f"Registry lookup failed ({error.code}): {url}") from error
        except (urllib.error.URLError, TimeoutError, ValueError) as error:
            if attempt < 2:
                time.sleep(2 ** attempt)
                continue
            raise PolicyError(f"Registry lookup failed: {url} ({error})") from error
    raise PolicyError(f"Registry lookup failed: {url}")


def published_at(ecosystem: str, name: str, version: str, cache: dict) -> datetime:
    if name.startswith("!unsupported-source:"):
        raise PolicyError(
            f"{name.removeprefix('!unsupported-source:')}@{version} is not from "
            "the official registry; publication age cannot be verified"
        )
    if ecosystem == "cargo":
        endpoint = (
            "https://crates.io/api/v1/crates/"
            + urllib.parse.quote(name, safe="")
            + "/"
            + urllib.parse.quote(version, safe="")
        )
        raw = fetch_json(endpoint).get("version", {}).get("created_at")
    else:
        if name not in cache:
            endpoint = "https://registry.npmjs.org/" + urllib.parse.quote(name, safe="@")
            cache[name] = fetch_json(endpoint).get("time", {})
        raw = cache[name].get(version)
    if not isinstance(raw, str):
        raise PolicyError(f"No publication timestamp for {ecosystem}:{name}@{version}")
    try:
        return datetime.fromisoformat(raw.replace("Z", "+00:00")).astimezone(timezone.utc)
    except ValueError as error:
        raise PolicyError(f"Invalid release date for {ecosystem}:{name}@{version}") from error


def added_versions(base: set, current: set) -> list[tuple[str, str]]:
    return sorted(current - base)


def evaluate(
    ecosystem: str,
    additions: list[tuple[str, str]],
    now: datetime,
    cache: dict,
) -> list[str]:
    failures = []
    for name, version in additions:
        try:
            released = published_at(ecosystem, name, version, cache)
        except PolicyError as error:
            failures.append(str(error))
            continue
        age = now - released
        if age < MINIMUM_AGE:
            remaining = MINIMUM_AGE - age
            failures.append(
                f"{ecosystem}:{name}@{version} published {released.isoformat()}; "
                f"wait at least {remaining} (7-day minimum)"
            )
        else:
            print(f"PASS {ecosystem}:{name}@{version} ({age.days} days old)")
    return failures


def run(base: str) -> int:
    now = datetime.now(timezone.utc)
    checked = 0
    failures = []
    npm_cache: dict = {}
    for path in (CARGO_LOCKFILE, *NPM_LOCKFILES):
        target = Path(path)
        if not target.exists():
            raise PolicyError(f"Required dependency lockfile is missing: {path}")
        before = read_base_file(base, path)
        after = target.read_bytes()
        if path in NPM_LOCKFILES:
            manifest_path = target.with_name("package.json")
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            root_lock = json.loads(after).get("packages", {}).get("", {})
            for group in ("dependencies", "devDependencies", "optionalDependencies"):
                if manifest.get(group, {}) != root_lock.get(group, {}):
                    raise PolicyError(
                        f"{manifest_path} {group} differs from {path}; "
                        "regenerate and commit the npm lockfile"
                    )
        parser = cargo_packages if path == CARGO_LOCKFILE else npm_packages
        ecosystem = "cargo" if path == CARGO_LOCKFILE else "npm"
        additions = added_versions(parser(before), parser(after))
        checked += len(additions)
        if additions:
            print(f"Checking {len(additions)} new versions in {path}")
        failures.extend(evaluate(ecosystem, additions, now, npm_cache))
    for failure in failures:
        print(f"::error title=Dependency release-age policy::{failure}", file=sys.stderr)
    print(f"Checked {checked} new/changed locked package versions; blocked {len(failures)}.")
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--base",
        default=f"origin/{os.environ.get('GITHUB_BASE_REF', 'main')}",
        help="Target branch ref to compare against",
    )
    args = parser.parse_args()
    try:
        return run(args.base)
    except (PolicyError, OSError, json.JSONDecodeError, ValueError) as error:
        print(f"::error title=Dependency release-age check failed::{error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
