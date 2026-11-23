#!/usr/bin/env python3
"""Adds the specs and plans shared in the 2026-11-22 chat to the project, through the ax CLI.

Every write goes through `ax spec-create`, `ax spec-plan`, `ax spec-import`, and `ax spec-classify`,
the same JSON contracts as the web routes. Stops at the first command that fails or answers ok: false.

  python3 scripts/import-chat-specs.py --session <chat id> --turn <turn id> [--dry-run]
"""

import argparse
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
OVERHAUL = "docs/specs/spec-plan-context-overhaul"
WORKSPACE = "docs/specs/specification-workspace"

# (spec id, title, spec file, [(plan id, plan title, plan file)]); an empty plan list imports the spec alone.
SPECS = [
    (
        "spec-plan-context-overhaul",
        "Spec, plan and context overhaul",
        f"{OVERHAUL}/01-MASTER-SPEC.md",
        [
            ("ax-overhaul-plan-foundation", "Plan A — Domain model, migration, agent capture and UI", f"{OVERHAUL}/02-PLAN-FOUNDATION.md"),
            ("ax-overhaul-plan-resolver", "Plan B — Authority, context resolver, manifests and fresh-agent replay", f"{OVERHAUL}/03-PLAN-RESOLVER-AND-REPLAY.md"),
            ("ax-replay-validation-contract", "Plan C — Replay validation contract", f"{OVERHAUL}/04-VALIDATION-SPEC-AND-PLAN.md"),
            ("plan-b-b0-contract", "Plan B · Phase B0 contract", f"{OVERHAUL}/B0-CONTRACT.md"),
            ("plan-b-b6-hardening", "Plan B · Phase B6 hardening", f"{OVERHAUL}/B6-HARDENING.md"),
            ("plan-b-b6-replica-and-fixture-pilot", "Plan B · B6 replica manifest sync and Plan C fixture pilot", f"{OVERHAUL}/B6-REPLICA-AND-FIXTURE-PILOT.md"),
            ("plan-c-pilot-v1", "Plan C · Pilot v1 registry", f"{OVERHAUL}/PLAN-C-PILOT-V1.md"),
        ],
    ),
    (
        "specification-workspace",
        "Specification workspace UI/UX overhaul",
        f"{WORKSPACE}/SPEC.md",
        [
            ("specification-workspace-phase-1", "Phase 1 — Links, gated schema, and the checklist store", f"{WORKSPACE}/PHASE-1-SPEC.md"),
            ("specification-workspace-phase-2a", "Phase 2A — The workspace view", f"{WORKSPACE}/PHASE-2A-WORKSPACE-VIEW.md"),
        ],
    ),
    ("specs-hierarchy-ui", "Specs hierarchy UI (specs first, then plans)", "docs/specs/specs-hierarchy-ui/SPEC.md", []),
    ("global-db-first-open", "Fix: concurrent first opens of a new global database", "docs/specs/global-db-first-open/SPEC.md", []),
]

CLASSIFY = {
    "allPlanMode": True,
    "docType": "spec",
    "actor": "owner decision, applied by the agent",
    "reason": "Owner in chat on 2026-11-22: the 30 plan-mode documents are specifications.",
}


def run(ax, command, body, dry_run, cwd=ROOT):
    payload = json.dumps(body)
    if dry_run:
        print(f"[dry-run] ax {command} {payload[:120]}")
        return
    done = subprocess.run([ax, command, payload], cwd=cwd, capture_output=True, text=True)
    if done.returncode != 0:
        sys.exit(f"FAIL: ax {command} exited {done.returncode}: {done.stderr.strip() or done.stdout.strip()}")
    try:
        answer = json.loads(done.stdout.strip().splitlines()[-1])
    except (ValueError, IndexError):
        sys.exit(f"FAIL: ax {command} printed no JSON: {done.stdout!r}")
    if answer.get("ok") is not True:
        sys.exit(f"FAIL: ax {command} answered {answer}")
    print(f"ok: ax {command} {answer}")


def text(path):
    body = (ROOT / path).read_text(encoding="utf-8").strip()
    if not body:
        sys.exit(f"FAIL: {path} is empty")
    return body


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--ax", default=str(ROOT / "target-dev/release/ax"))
    parser.add_argument("--project", default="local")
    parser.add_argument("--session", required=True)
    parser.add_argument("--turn", required=True)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--cwd", default=str(ROOT), help="project whose .ax database receives the writes")
    args = parser.parse_args()
    origin = {"projectId": args.project, "sessionId": args.session, "turnId": args.turn}
    cwd = pathlib.Path(args.cwd)

    def call(command, body):
        run(args.ax, command, body, args.dry_run, cwd)

    for spec_id, title, spec_file, plans in SPECS:
        if not plans:
            call("spec-import", {**origin, "specId": spec_id, "specTitle": title, "specBody": text(spec_file)})
            continue
        first_id, first_title, first_file = plans[0]
        call("spec-create", {
            **origin,
            "specId": spec_id,
            "specTitle": title,
            "specBody": text(spec_file),
            "planId": first_id,
            "planTitle": first_title,
            "planBody": text(first_file),
        })
        for plan_id, plan_title, plan_file in plans[1:]:
            call("spec-plan", {
                **origin,
                "specId": spec_id,
                "planId": plan_id,
                "planTitle": plan_title,
                "planBody": text(plan_file),
            })
    call("spec-classify", CLASSIFY)


if __name__ == "__main__":
    main()
