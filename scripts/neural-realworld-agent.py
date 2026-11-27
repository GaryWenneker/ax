#!/usr/bin/env python3
"""Verify-gated agent runner for REALWORLD-TASKS.json (held-out).

For tasks with injects: break -> confirm RED -> agent -> verify -> restore.
Success = all verify[] steps exit 0. Agent does not self-score.

The runner edits the checkout it lives in: it writes each arm's `neural` block
into ax.json, breaks source on purpose, and the agent edits tracked files. After
every attempt ax.json gets its previous contents back and every tracked file the
attempt changed is checked out from HEAD again, so a run never leaves broken
code behind. Because of that reset the runner refuses to start while tracked
files have uncommitted changes (the results file excepted).
"""

from __future__ import annotations

import argparse
import json
import os
import random
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TASKS = ROOT / "docs/specs/ax-neural/REALWORLD-TASKS.json"
OUT_REL = "docs/specs/ax-neural/REALWORLD-RESULTS.json"
OUT = ROOT / OUT_REL
INJECT = ROOT / "scripts/neural-realworld-inject.py"

ARMS = {
    "A_production_retrieval": {"prediction": {"enabled": False}, "learning": {"feedback_collection": False}},
    "B_neural_ranking": {"prediction": {"enabled": False}, "learning": {"feedback_collection": False}},
    "D_prediction": {"prediction": {"enabled": True}, "learning": {"feedback_collection": False}},
    "E_learning_train_only": {"prediction": {"enabled": True}, "learning": {"feedback_collection": True}},
}


def run(cmd: list[str], cwd: Path, env: dict | None = None, timeout: int | None = None) -> subprocess.CompletedProcess[str]:
    merged = os.environ.copy()
    if env:
        merged.update(env)
    merged["NO_COLOR"] = "1"
    return subprocess.run(cmd, cwd=cwd, env=merged, capture_output=True, text=True, timeout=timeout)


def write_ax_json(neural: dict) -> None:
    path = ROOT / "ax.json"
    cfg = json.loads(path.read_text(encoding="utf-8")) if path.exists() else {}
    cfg["neural"] = neural
    path.write_text(json.dumps(cfg, indent=2) + "\n", encoding="utf-8")


def inject(action: str, task_id: str) -> dict:
    """Apply or revert an inject; raise unless it reports applied or noop."""
    done = run([sys.executable, str(INJECT), action, "--task", task_id, "--tree", str(ROOT)], cwd=ROOT)
    lines = done.stdout.strip().splitlines()
    try:
        result = json.loads(lines[-1]) if lines else None
    except json.JSONDecodeError:
        result = None
    if done.returncode != 0 or not isinstance(result, dict) or not (result.get("applied") or result.get("noop")):
        raise SystemExit(f"inject {action} {task_id} failed: {(done.stderr or done.stdout).strip()[-500:]}")
    return result


def git(*args: str) -> str:
    done = run(["git", *args], cwd=ROOT)
    if done.returncode != 0:
        raise SystemExit(f"git {' '.join(args)} failed: {done.stderr.strip()}")
    return done.stdout


def changed_tracked_files() -> list[str]:
    """Tracked files that differ from HEAD (staged or not), except the results file."""
    names = git("diff", "--name-only", "HEAD", "--").splitlines()
    return sorted(n for n in names if n and n != OUT_REL)


class Attempt:
    """Put ax.json and every tracked file back after one attempt, however it ends."""

    def __enter__(self) -> "Attempt":
        path = ROOT / "ax.json"
        self.ax_json = path.read_bytes() if path.exists() else None
        return self

    def __exit__(self, *exc: object) -> None:
        path = ROOT / "ax.json"
        if self.ax_json is None:
            path.unlink(missing_ok=True)
        else:
            path.write_bytes(self.ax_json)
        changed = changed_tracked_files()
        if changed:
            git("checkout", "HEAD", "--", *changed)
        left = changed_tracked_files()
        if left:
            raise SystemExit(f"could not restore tracked files after the attempt: {', '.join(left)}")


def tail(text: str | bytes | None, size: int) -> str:
    if isinstance(text, (bytes, bytearray)):
        text = text.decode("utf-8", errors="replace")
    return (text or "")[-size:]


def verify_task(task: dict) -> dict:
    steps = []
    ok = True
    for step in task.get("verify") or []:
        cmd = step.get("cmd")
        if not cmd:
            continue
        started = time.perf_counter()
        try:
            done = run(["bash", "-lc", cmd], cwd=ROOT, timeout=int(step.get("timeoutSec") or 600))
            elapsed = (time.perf_counter() - started) * 1000
            expect = int(step.get("expectExit", 0))
            passed = done.returncode == expect
            ok = ok and passed
            steps.append({
                "cmd": cmd,
                "exitCode": done.returncode,
                "passed": passed,
                "elapsedMillis": elapsed,
                "stderrTail": (done.stderr or "")[-400:],
            })
        except subprocess.TimeoutExpired:
            ok = False
            steps.append({"cmd": cmd, "passed": False, "error": "timeout"})
    return {"passed": ok, "steps": steps}


def agent_attempt(task: dict, model: str, agent_bin: str) -> dict:
    prompt = (
        "You are working in the Ax repository at the workspace root.\n"
        "A deliberate defect/inject may be present for this benchmark task. Fix the code so the acceptance criteria hold.\n"
        "Do not claim success without making the required code changes.\n"
        "Do not raise SEMANTIC_WEIGHT or enable production ONNX.\n"
        f"Task id: {task['id']}\nCategory: {task['category']}\nDifficulty: {task['difficulty']}\n\n"
        f"{task['description']}\n\nAcceptance criteria:\n- "
        + "\n- ".join(task["acceptanceCriteria"])
        + "\n\nPrefer editing these files when relevant:\n- "
        + "\n- ".join(task["expectedRelevantFiles"])
    )
    cmd = [
        agent_bin, "-p", "--trust", "--approve-mcps", "-f",
        "--output-format", "json", "--model", model,
        "--workspace", str(ROOT), prompt,
    ]
    started = time.perf_counter()
    # Cap wall time: agents often hang on full-repo git status/diff in this tree.
    agent_timeout = int(os.environ.get("AX_RW_AGENT_TIMEOUT_SEC", "480"))
    try:
        done = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True, timeout=agent_timeout)
    except subprocess.TimeoutExpired as exc:
        elapsed = (time.perf_counter() - started) * 1000
        return {
            "ok": False,
            "error": "agent_timeout",
            "timeoutSec": agent_timeout,
            "elapsedMillis": elapsed,
            "stderrTail": tail(exc.stderr, 1000) or f"timed out after {agent_timeout}s",
        }
    elapsed = (time.perf_counter() - started) * 1000
    try:
        payload = json.loads(done.stdout)
    except json.JSONDecodeError:
        return {
            "ok": False,
            "error": "non-json agent output",
            "elapsedMillis": elapsed,
            "stderrTail": (done.stderr or done.stdout or "")[-1000:],
        }
    usage = payload.get("usage") or {}
    return {
        "ok": done.returncode == 0 and not payload.get("is_error"),
        "elapsedMillis": elapsed,
        "inputTokens": int(usage.get("inputTokens") or usage.get("input_tokens") or 0),
        "outputTokens": int(usage.get("outputTokens") or usage.get("output_tokens") or 0),
        "session": payload.get("session_id"),
        "textTail": (payload.get("result") or "")[-1500:],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--task", action="append")
    parser.add_argument("--inject-heldout", action="store_true", help="all heldOut tasks that have inject metadata")
    parser.add_argument("--arms", default="A_production_retrieval,B_neural_ranking,D_prediction,E_learning_train_only")
    parser.add_argument("--model", default=os.environ.get("AX_NEURAL_LIVE_MODEL", "composer-2-fast"))
    parser.add_argument("--verify-only", action="store_true")
    parser.add_argument("--reps", type=int, default=1)
    parser.add_argument("--seed", type=int, default=3)
    parser.add_argument("--limit", type=int, default=0, help="max task count this run")
    parser.add_argument("--skip-done", action="store_true", help="skip task/arm/rep already scored in REALWORLD-RESULTS.json")
    parser.add_argument("--exclude-task", action="append", default=[], help="task ids to exclude")
    opts = parser.parse_args()

    suite = json.loads(TASKS.read_text(encoding="utf-8"))
    held = [t for t in suite["tasks"] if t["split"] == "heldOut"]
    by_id = {t["id"]: t for t in suite["tasks"]}

    if opts.inject_heldout:
        selected = [t for t in held if t.get("inject")]
    elif opts.task:
        selected = [by_id[i] for i in opts.task]
    else:
        selected = [t for t in held if t.get("inject")][:1]

    if opts.exclude_task:
        exclude = set(opts.exclude_task)
        selected = [t for t in selected if t["id"] not in exclude]

    if opts.limit:
        selected = selected[: opts.limit]

    arms = [a.strip() for a in opts.arms.split(",") if a.strip()]
    for a in arms:
        if a not in ARMS:
            raise SystemExit(f"unknown or blocked arm {a}")

    dirty = changed_tracked_files()
    if dirty:
        raise SystemExit(
            "tracked files have uncommitted changes; every attempt resets tracked files to HEAD, "
            f"so commit or stash these first: {', '.join(dirty)}"
        )

    agent_bin = shutil.which("cursor-agent") or shutil.which("agent")
    rows = []
    rng = random.Random(opts.seed)

    print(json.dumps({"runner": "start", "selected": len(selected), "arms": arms, "skipDone": opts.skip_done}), flush=True)
    existing = json.loads(OUT.read_text(encoding="utf-8")) if OUT.is_file() else {}
    done_keys = set()
    if opts.skip_done:
        for r in (existing.get("agent") or {}).get("rows") or []:
            if r.get("invalid"):
                continue
            if "taskSuccess" in r:
                done_keys.add((r.get("taskId"), r.get("arm"), r.get("rep", 0)))

    print(json.dumps({"runner": "done_keys", "n": len(done_keys)}), flush=True)

    for rep in range(opts.reps):
        order = list(arms)
        rng.shuffle(order)
        for task in selected:
            for arm in order:
                if opts.skip_done and (task["id"], arm, rep) in done_keys:
                    print(json.dumps({"taskId": task["id"], "arm": arm, "rep": rep, "skipped": True}), flush=True)
                    continue
                baseline_red = None
                inject_meta = None
                with Attempt():
                    write_ax_json(ARMS[arm])
                    if task.get("inject"):
                        # ensure fixed, then break
                        inject("fix", task["inject"]["manifestId"])
                        inject_meta = inject("break", task["inject"]["manifestId"])
                        baseline = verify_task(task)
                        baseline_red = not baseline.get("passed")
                        if not baseline_red and not opts.verify_only:
                            # cannot score — inject did not create RED
                            row = {
                                "taskId": task["id"], "arm": arm, "rep": rep,
                                "category": task["category"],
                                "inject": inject_meta,
                                "baselineRed": False,
                                "taskSuccess": False,
                                "invalid": True,
                                "reason": "inject did not make verify RED",
                                "selfScored": False,
                            }
                            rows.append(row)
                            print(json.dumps(row), flush=True)
                            inject("fix", task["inject"]["manifestId"])
                            continue

                    agent = {"kind": "skipped", "reason": "verify-only"}
                    if not opts.verify_only:
                        if not agent_bin:
                            agent = {"kind": "not_measured", "reason": "cursor-agent missing"}
                        else:
                            agent = agent_attempt(task, opts.model, agent_bin)

                    ver = verify_task(task)
                    row = {
                        "taskId": task["id"],
                        "arm": arm,
                        "rep": rep,
                        "category": task["category"],
                        "inject": inject_meta,
                        "baselineRed": baseline_red,
                        "agent": agent,
                        "verify": ver,
                        "taskSuccess": bool(ver.get("passed")),
                        "selfScored": False,
                        "elapsedMillis": (agent or {}).get("elapsedMillis"),
                        "inputTokens": (agent or {}).get("inputTokens"),
                        "outputTokens": (agent or {}).get("outputTokens"),
                    }
                    rows.append(row)
                    # CHECKPOINT: persist after each arm so long runs are not all-or-nothing
                    _agent_block = existing.get("agent") if isinstance(existing.get("agent"), dict) else {}
                    _prev = list(_agent_block.get("rows") or [])
                    _merged = {(r.get("taskId"), r.get("arm"), r.get("rep", 0)): r for r in _prev}
                    for r in rows:
                        _merged[(r["taskId"], r["arm"], r.get("rep", 0))] = r
                    _all = list(_merged.values())
                    _scored = [r for r in _all if not r.get("invalid")]
                    _ok = sum(1 for r in _scored if r.get("taskSuccess"))
                    existing["agent"] = {
                        "kind": "partial" if _all else "not_measured",
                        "method": "inject break -> confirm RED -> cursor-agent -> automated verify -> restore; no self-score",
                        "rows": _all,
                        "tasksExecuted": len({r["taskId"] for r in _all}),
                        "scoredRows": len(_scored),
                        "successes": _ok,
                        "successRate": (_ok / len(_scored)) if _scored else None,
                        "note": "checkpointed mid-run; C blocked",
                    }
                    OUT.write_text(json.dumps(existing, indent=2) + "\n", encoding="utf-8")
                    print(json.dumps({
                        "taskId": task["id"], "arm": arm, "rep": rep,
                        "taskSuccess": row["taskSuccess"], "baselineRed": baseline_red,
                    }), flush=True)

    agent_block = existing.get("agent") if isinstance(existing.get("agent"), dict) else {}
    prev_rows = list(agent_block.get("rows") or [])
    merged = {(r.get("taskId"), r.get("arm"), r.get("rep", 0)): r for r in prev_rows}
    for r in rows:
        merged[(r["taskId"], r["arm"], r.get("rep", 0))] = r
    all_rows = list(merged.values())
    scored = [r for r in all_rows if not r.get("invalid")]
    successes = sum(1 for r in scored if r.get("taskSuccess"))
    existing["agent"] = {
        "kind": "partial" if all_rows else "not_measured",
        "method": "inject break -> confirm RED -> cursor-agent -> automated verify -> restore; no self-score",
        "rows": all_rows,
        "tasksExecuted": len({r["taskId"] for r in all_rows}),
        "scoredRows": len(scored),
        "successes": successes,
        "successRate": (successes / len(scored)) if scored else None,
        "byArm": {},
        "note": "Full 30 heldOut x arms x reps still required; C blocked",
    }
    for arm in ARMS:
        subset = [r for r in scored if r.get("arm") == arm]
        if not subset:
            continue
        ok = sum(1 for r in subset if r.get("taskSuccess"))
        existing["agent"]["byArm"][arm] = {
            "n": len(subset),
            "successes": ok,
            "successRate": ok / len(subset),
            "inputTokens": sum(int(r.get("inputTokens") or 0) for r in subset),
            "outputTokens": sum(int(r.get("outputTokens") or 0) for r in subset),
            "latenciesMs": [r.get("elapsedMillis") for r in subset if r.get("elapsedMillis") is not None],
        }
    OUT.write_text(json.dumps(existing, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"wrote": str(OUT), "newRows": len(rows), "totalRows": len(all_rows)}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
