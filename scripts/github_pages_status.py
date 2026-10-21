#!/usr/bin/env python3
"""Exit 0 only when the newest GitHub Pages deployment status is success.

The newest status is the first object returned by the deployments status API.
A missing list, an empty list, or any state other than success is a failure.
"""

from __future__ import annotations

import json
import subprocess
import sys

REPO = "GaryWenneker/ax"


def newest_state(statuses: list) -> str | None:
    if not statuses:
        return None
    state = statuses[0].get("state")
    if not isinstance(state, str) or not state:
        return None
    return state


def fetch_newest_statuses() -> list:
    raw = subprocess.check_output(
        [
            "gh",
            "api",
            f"repos/{REPO}/deployments?environment=github-pages&per_page=1",
        ],
        text=True,
    )
    deployments = json.loads(raw)
    if not deployments:
        return []
    deployment_id = deployments[0]["id"]
    raw = subprocess.check_output(
        ["gh", "api", f"repos/{REPO}/deployments/{deployment_id}/statuses"],
        text=True,
    )
    return json.loads(raw)


def main(argv: list[str]) -> int:
    if "--statuses-json" in argv:
        index = argv.index("--statuses-json")
        statuses = json.loads(argv[index + 1])
    else:
        statuses = fetch_newest_statuses()
    state = newest_state(statuses)
    if state == "success":
        print("GitHub Pages deployment is success")
        return 0
    if state is None:
        print("GitHub Pages deployment status is missing", file=sys.stderr)
        return 1
    print(f"GitHub Pages deployment is {state}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv[1:]))
    except Exception as exc:
        print(f"GitHub Pages status check failed: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc
