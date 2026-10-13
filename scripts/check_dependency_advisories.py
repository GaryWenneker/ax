#!/usr/bin/env python3
"""Fail PRs adding package releases affected by known OSV advisories."""

import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

from check_dependency_age import (
    CARGO_LOCKFILE, NPM_LOCKFILES, PolicyError, USER_AGENT,
    added_versions, cargo_packages, npm_packages, read_base_file,
)


def proposed_additions(base: str) -> list[dict]:
    queries = []
    for path in (CARGO_LOCKFILE, *NPM_LOCKFILES):
        parser = cargo_packages if path == CARGO_LOCKFILE else npm_packages
        before = parser(read_base_file(base, path))
        after = parser(Path(path).read_bytes())
        ecosystem = "crates.io" if path == CARGO_LOCKFILE else "npm"
        for name, version in added_versions(before, after):
            if name.startswith("!unsupported-source:"):
                raise PolicyError(f"Cannot audit non-registry package: {name}@{version}")
            queries.append({
                "package": {"name": name, "ecosystem": ecosystem},
                "version": version,
            })
    return queries


def fetch_batch(queries: list[dict]) -> list[dict]:
    request = urllib.request.Request(
        "https://api.osv.dev/v1/querybatch",
        data=json.dumps({"queries": queries}).encode(),
        headers={"Content-Type": "application/json", "User-Agent": USER_AGENT},
        method="POST",
    )
    for attempt in range(3):
        try:
            with urllib.request.urlopen(request, timeout=45) as response:
                payload = json.load(response)
            result = payload.get("results")
            if not isinstance(result, list) or len(result) != len(queries):
                raise PolicyError("Incomplete OSV batch response")
            return result
        except urllib.error.HTTPError as error:
            if error.code in (429, 502, 503, 504) and attempt < 2:
                time.sleep(2 ** attempt)
                continue
            raise PolicyError(f"OSV returned HTTP {error.code}") from error
        except (urllib.error.URLError, TimeoutError, ValueError) as error:
            if attempt < 2:
                time.sleep(2 ** attempt)
                continue
            raise PolicyError(f"OSV is unavailable: {error}") from error
    raise PolicyError("OSV query failed after retries")


def classify_findings(
    queries: list[dict], results: list[dict]
) -> tuple[list[str], list[str]]:
    """Separate expected vulnerable releases from failed/incomplete OSV lookups.

    A rejected automation candidate is not a pipeline malfunction, but missing
    security evidence must still fail the pipeline closed.
    """
    advisories, errors = [], []
    if len(queries) != len(results):
        raise PolicyError("Incomplete OSV results")
    for query, result in zip(queries, results):
        ecosystem = query["package"]["ecosystem"]
        name = query["package"]["name"]
        version = query["version"]
        identity = f"{ecosystem}:{name}@{version}"
        if not isinstance(result, dict):
            errors.append(f"Incomplete OSV result for {identity}")
            continue
        vulns = result.get("vulns", [])
        if not isinstance(vulns, list) or result.get("next_page_token"):
            errors.append(f"Incomplete/paginated OSV result for {identity}")
        elif any(
            not isinstance(v, dict)
            or not isinstance(v.get("id"), str)
            or not v["id"]
            for v in vulns
        ):
            errors.append(f"Malformed OSV advisory for {identity}")
        elif vulns:
            ids = ", ".join(v["id"] for v in vulns)
            advisories.append(f"{identity} has known vulnerabilities: {ids}")
        else:
            print(f"PASS {identity}: no known OSV advisories")
    return advisories, errors


def findings(queries: list[dict], results: list[dict]) -> list[str]:
    """Backward-compatible strict PR review: all OSV problems block the change."""
    advisories, errors = classify_findings(queries, results)
    return errors + advisories


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", default="origin/" + os.environ.get("GITHUB_BASE_REF", "main"))
    parser.add_argument(
        "--candidate", action="store_true",
        help="Return 2 for a known-vulnerable auto-remediation candidate; "
             "operational/security-evidence failures remain exit 1.",
    )
    args = parser.parse_args()
    try:
        additions = proposed_additions(args.base)
        advisories, errors = [], []
        for offset in range(0, len(additions), 100):
            batch = additions[offset:offset + 100]
            batch_advisories, batch_errors = classify_findings(batch, fetch_batch(batch))
            advisories.extend(batch_advisories)
            errors.extend(batch_errors)
        for error in errors:
            print(f"::error title=OSV review incomplete::{error}", file=sys.stderr)
        for advisory in advisories:
            level = "warning" if args.candidate else "error"
            print(f"::{level} title=Dependency vulnerability::{advisory}", file=sys.stderr)
        print(f"OSV checked {len(additions)} newly locked versions.")
        if errors:
            return 1
        if advisories:
            return 2 if args.candidate else 1
        return 0
    except (PolicyError, OSError, ValueError, json.JSONDecodeError) as error:
        print(f"::error title=OSV review failed::{error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
