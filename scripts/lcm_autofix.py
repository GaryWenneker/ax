#!/usr/bin/env python3
"""Approve only risk-reducing, lockfile-only npm audit remediation candidates."""

import argparse
import json
import subprocess
import sys
from pathlib import Path

SEVERITIES = ("critical", "high", "moderate", "low", "info")


class RejectedCandidate(Exception):
    """The attempted update is not eligible for automatic PR creation."""


def severity_counts(report: dict) -> dict[str, int]:
    if not isinstance(report, dict) or report.get("error"):
        raise RejectedCandidate("npm audit reported a registry or input error")
    metadata = report.get("metadata")
    counts = metadata.get("vulnerabilities") if isinstance(metadata, dict) else None
    if not isinstance(counts, dict):
        raise RejectedCandidate("npm audit report lacks vulnerability metadata")
    parsed = {}
    for severity in SEVERITIES:
        count = counts.get(severity)
        if isinstance(count, bool) or not isinstance(count, int) or count < 0:
            raise RejectedCandidate(f"invalid npm audit count: {severity}")
        parsed[severity] = count
    if counts.get("total") != sum(parsed.values()):
        raise RejectedCandidate("npm audit total is missing or inconsistent")
    return parsed


def is_improvement(before: dict[str, int], after: dict[str, int]) -> bool:
    return all(after[s] <= before[s] for s in SEVERITIES) and any(
        after[s] < before[s] for s in SEVERITIES
    )


def tracked_changes() -> set[str]:
    result = subprocess.run(
        ["git", "diff", "--name-only", "--exit-code"],
        capture_output=True, text=True, check=False,
    )
    if result.returncode not in (0, 1):
        raise RejectedCandidate(f"git diff failed: {result.stderr.strip()}")
    return set(result.stdout.splitlines())


def check_candidate(project: str, before: dict, after: dict, changes: set[str]) -> bool:
    lockfile = f"{project}/package-lock.json"
    if not changes:
        print(f"SKIP {project}: no lockfile update available")
        return False
    if changes != {lockfile}:
        raise RejectedCandidate(
            f"automated fix changed unexpected tracked files: {sorted(changes)}"
        )
    old, new = severity_counts(before), severity_counts(after)
    if not is_improvement(old, new):
        print(f"SKIP {project}: audit findings did not safely decrease: {old} -> {new}")
        return False
    print(f"ELIGIBLE {project}: {old} -> {new}")
    return True


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project", required=True, choices=(
        "site", "telemetry-worker", "crates/ax-web/web-ui",
    ))
    parser.add_argument("--before", required=True, type=Path)
    parser.add_argument("--after", required=True, type=Path)
    parser.add_argument("--github-output", required=True, type=Path)
    args = parser.parse_args()
    try:
        before = json.loads(args.before.read_text(encoding="utf-8"))
        after = json.loads(args.after.read_text(encoding="utf-8"))
        selected = check_candidate(args.project, before, after, tracked_changes())
        with args.github_output.open("a", encoding="utf-8") as stream:
            stream.write(f"eligible={'true' if selected else 'false'}\n")
        return 0
    except (RejectedCandidate, OSError, ValueError, KeyError) as error:
        print(f"::error title=LCM candidate rejected::{error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
