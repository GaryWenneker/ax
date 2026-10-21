#!/usr/bin/env python3
"""Enforce release-age policy for locked crates, npm packages, and PHP.

Crates and npm packages must be at least seven days old. The PHP runtime is
the exception: every version that runs must be the latest stable release on
https://www.php.net/, including a release published today.
Uses only Python's standard library and public registry metadata.
"""

import argparse
import json
import os
import re
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
PHP_RELEASES_URL = "https://www.php.net/releases/?json"
_STABLE_VERSION = re.compile(r"^\d+\.\d+\.\d+$")
_PRERELEASE_TAG = re.compile(r"^(?:rc|alpha|beta)$", re.IGNORECASE)
_PHP_EXACT = re.compile(r"^v?(?P<core>\d+\.\d+\.\d+)(?:$|[-.](?!\d).*)$")
_PHP_MINOR = re.compile(r"^v?(?P<core>\d+\.\d+)(?:$|[-.](?!\d).*)$")
_RESOLVED_LATEST_PIN = "steps.php_runtime.outputs.version"
_SETUP_PIN = re.compile(
    r"(?im)^[ \t]*php-version:[ \t]*['\"]?([^'\"\s#]+)['\"]?[ \t]*$"
)
_DOCKER_PIN = re.compile(r"(?im)^[ \t]*FROM[ \t]+php:(\S+)")


def latest_stable_php(payload: dict) -> str:
    """Return the newest stable PHP x.y.z from the php.net releases document."""
    if not isinstance(payload, dict):
        raise PolicyError("PHP releases feed has no current stable version")
    best: tuple[tuple[int, int, int], str] | None = None
    for release in payload.values():
        if not isinstance(release, dict) or release.get("museum") is True:
            continue
        version = release.get("version")
        if not isinstance(version, str) or _STABLE_VERSION.fullmatch(version) is None:
            continue
        tags = release.get("tags") or []
        if not isinstance(tags, list):
            raise PolicyError("PHP releases feed has no current stable version")
        if any(isinstance(tag, str) and _PRERELEASE_TAG.fullmatch(tag) for tag in tags):
            continue
        parsed = tuple(int(part) for part in version.split("."))
        if best is None or parsed > best[0]:
            best = (parsed, version)
    if best is None:
        raise PolicyError("PHP releases feed has no current stable version")
    return best[1]


def php_runtime_failure(version: str, latest: str) -> str | None:
    """Return a failure when `version` is not the latest stable PHP runtime.

    The seven-day minimum age does not apply. A minor channel is current only
    when it is the major.minor of `latest`, because that channel installs that
    line's current patch.
    """
    if not isinstance(latest, str) or _STABLE_VERSION.fullmatch(latest) is None:
        raise PolicyError("PHP latest stable version is not an x.y.z release")
    token = version.strip()
    exact = _PHP_EXACT.fullmatch(token)
    minor = None if exact else _PHP_MINOR.fullmatch(token)
    latest_minor = ".".join(latest.split(".")[:2])
    if (exact and exact.group("core") == latest) or (
        minor and minor.group("core") == latest_minor
    ):
        return None
    shown = token or "(empty)"
    return (
        f"PHP runtime {shown} is not the latest stable {latest}. "
        "The 7-day minimum does not apply to PHP; the latest version is required."
    )


def declared_php_runtimes(text: str) -> list[str]:
    """Return concrete PHP pins from workflow YAML and Dockerfiles."""
    body = "\n".join(
        line for line in text.splitlines() if _RESOLVED_LATEST_PIN not in line
    )
    return [match.group(1) for pattern in (_SETUP_PIN, _DOCKER_PIN) for match in pattern.finditer(body)]


def php_runtime_pins(root: Path) -> list[str]:
    """Return PHP runtime pins declared by workflows and Dockerfiles."""
    files: list[Path] = []
    workflow_dir = root / ".github" / "workflows"
    if workflow_dir.is_dir():
        files.extend(sorted(workflow_dir.glob("*.yml")))
        files.extend(sorted(workflow_dir.glob("*.yaml")))
    files.extend(sorted(path for path in root.glob("Dockerfile") if path.is_file()))
    files.extend(sorted(path for path in root.glob("Dockerfile.*") if path.is_file()))
    files.extend(sorted(path for path in root.glob("*.Dockerfile") if path.is_file()))
    pins: list[str] = []
    for path in files:
        pins.extend(declared_php_runtimes(path.read_text(encoding="utf-8")))
    return pins
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
        if ecosystem == "php":
            try:
                latest = cache.get("php-latest")
                if not isinstance(latest, str):
                    latest = latest_stable_php(fetch_json(PHP_RELEASES_URL))
                    cache["php-latest"] = latest
                failure = php_runtime_failure(version, latest)
            except PolicyError as error:
                failures.append(str(error))
                continue
            if failure:
                failures.append(failure)
            else:
                print(
                    f"PASS php:{name}@{version} "
                    f"(latest stable {latest}; 7-day minimum does not apply)"
                )
            continue
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


def check_php_runtimes(
    versions: list[str], latest: str | None, scan: Path | None
) -> int:
    resolved = latest if latest is not None else latest_stable_php(fetch_json(PHP_RELEASES_URL))
    pins = list(versions)
    if scan is not None:
        pins.extend(php_runtime_pins(scan))
    failures = []
    for version in pins:
        try:
            failure = php_runtime_failure(version, resolved)
        except PolicyError as error:
            failures.append(str(error))
            continue
        if failure:
            failures.append(failure)
        else:
            print(
                f"PASS php runtime {version} "
                f"(latest stable {resolved}; 7-day minimum does not apply)"
            )
    for failure in failures:
        print(
            f"::error title=PHP runtime must be the latest stable::{failure}",
            file=sys.stderr,
        )
    print(f"Checked {len(pins)} PHP runtimes against {resolved}; blocked {len(failures)}.")
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--base",
        default=f"origin/{os.environ.get('GITHUB_BASE_REF', 'main')}",
        help="Target branch ref to compare against",
    )
    parser.add_argument(
        "--php-runtime",
        action="append",
        default=[],
        help="PHP version that will run. Must be the latest stable release.",
    )
    parser.add_argument(
        "--php-latest",
        help="Override the latest stable PHP version. Used by tests and fixtures.",
    )
    parser.add_argument(
        "--php-scan",
        type=Path,
        help="Directory whose workflow and Dockerfile PHP pins must be current",
    )
    parser.add_argument(
        "--print-latest-php",
        action="store_true",
        help="Print the latest stable PHP version from php.net",
    )
    args = parser.parse_args()
    try:
        if args.print_latest_php:
            print(args.php_latest or latest_stable_php(fetch_json(PHP_RELEASES_URL)))
            return 0
        if args.php_runtime or args.php_scan is not None:
            return check_php_runtimes(args.php_runtime, args.php_latest, args.php_scan)
        return run(args.base)
    except (PolicyError, OSError, json.JSONDecodeError, ValueError) as error:
        print(f"::error title=Dependency release-age check failed::{error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
