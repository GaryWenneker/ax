#!/usr/bin/env python3
"""§5 live identical-LLM suite: ≥30 coding tasks × Ax baseline vs Neural prediction.

Same cursor-agent binary, model, and per-task prompt; only ax.json neural flags differ.
Supports --limit/--offset for batched runs; merges into --out.
A missing ax binary or cursor-agent prints not_measured and exits 2 without
touching --out.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SUITE_PATH = ROOT / "docs/specs/ax-neural/agent-suite-30.json"
DEFAULT_OUT = ROOT / "docs/specs/ax-neural/agent-suite-live-results.json"

ARMS = {
    "B_ax_baseline": {
        "prediction": {"enabled": False},
        "learning": {"feedback_collection": False},
    },
    "C_ax_neural_prediction": {
        "prediction": {"enabled": True},
        "learning": {"feedback_collection": False},
    },
}


def run(cmd: list[str], cwd: Path, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    merged = os.environ.copy()
    if env:
        merged.update(env)
    merged["NO_COLOR"] = "1"
    return subprocess.run(cmd, cwd=cwd, env=merged, capture_output=True, text=True)


def write_ax_json(project: Path, neural: dict) -> None:
    path = project / "ax.json"
    cfg = json.loads(path.read_text(encoding="utf-8")) if path.exists() else {}
    cfg["neural"] = neural
    path.write_text(json.dumps(cfg, indent=2) + "\n", encoding="utf-8")


def write_mcp(project: Path, binary: Path, home: Path) -> None:
    cursor_dir = project / ".cursor"
    cursor_dir.mkdir(parents=True, exist_ok=True)
    (cursor_dir / "mcp.json").write_text(
        json.dumps(
            {
                "mcpServers": {
                    "ax": {
                        "command": str(binary),
                        "args": ["serve", "--mcp", "--path", str(project)],
                        "env": {"HOME": str(home), "AX_HOME_DIR": str(home), "NO_COLOR": "1"},
                    }
                }
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )


def materialize(task: dict, project: Path) -> None:
    (project / "src").mkdir(parents=True, exist_ok=True)
    extras = task.get("extraFiles") or {}
    if extras:
        for rel, body in extras.items():
            path = project / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(body, encoding="utf-8")
    else:
        path = project / task["file"]
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(task["source"], encoding="utf-8")
        if task["file"] != "src/lib.rs" and not (project / "src/lib.rs").exists():
            mod = Path(task["file"]).stem
            (project / "src/lib.rs").write_text(f"mod {mod};\n", encoding="utf-8")


def make_project(binary: Path, base: Path, name: str, neural: dict, home: Path, task: dict) -> Path:
    project = base / name
    project.mkdir(parents=True)
    materialize(task, project)
    env = {"HOME": str(home), "AX_HOME_DIR": str(home), "NO_COLOR": "1"}
    done = run([str(binary), "init"], cwd=project, env=env)
    if done.returncode != 0:
        raise SystemExit(f"ax init failed: {done.stdout}\n{done.stderr}")
    write_ax_json(project, neural)
    write_mcp(project, binary, home)
    return project


def agent_turn(prompt: str, anchors: list[str], project: Path, model: str, agent_bin: str) -> dict:
    cmd = [
        agent_bin,
        "-p",
        "--trust",
        "--approve-mcps",
        "-f",
        "--output-format",
        "json",
        "--model",
        model,
        "--workspace",
        str(project),
        prompt,
    ]
    started = time.perf_counter()
    done = subprocess.run(cmd, cwd=project, capture_output=True, text=True)
    elapsed_ms = (time.perf_counter() - started) * 1000.0
    try:
        payload = json.loads(done.stdout)
    except json.JSONDecodeError:
        return {
            "ok": False,
            "error": f"non-json exit={done.returncode}",
            "elapsedMillis": elapsed_ms,
            "taskSuccess": False,
            "text": (done.stdout or done.stderr or "")[-800:],
        }
    usage = payload.get("usage") or {}
    text = payload.get("result") or ""
    anchors_hit = [a for a in anchors if a in text]
    is_error = bool(payload.get("is_error"))
    return {
        "ok": done.returncode == 0 and not is_error and payload.get("subtype") == "success",
        "session": payload.get("session_id"),
        "inputTokens": int(usage.get("inputTokens") or usage.get("input_tokens") or 0),
        "outputTokens": int(usage.get("outputTokens") or usage.get("output_tokens") or 0),
        "elapsedMillis": elapsed_ms,
        "text": text,
        "anchorsHit": anchors_hit,
        "taskSuccess": len(anchors_hit) == len(anchors),
        "isError": is_error,
        "exitCode": done.returncode,
    }


def summarize(rows: list[dict], suite: dict, model: str, agent: str) -> dict:
    by_arm: dict[str, list[dict]] = {}
    for r in rows:
        by_arm.setdefault(r["arm"], []).append(r)
    per_arm = {}
    for arm, items in by_arm.items():
        ok = sum(1 for i in items if i.get("taskSuccess"))
        per_arm[arm] = {
            "tasks": len(items),
            "successes": ok,
            "successRate": (ok / len(items)) if items else 0.0,
            "inputTokens": sum(int(i.get("inputTokens") or 0) for i in items),
            "outputTokens": sum(int(i.get("outputTokens") or 0) for i in items),
        }
    cats = {}
    for r in rows:
        key = (r.get("category"), r["arm"])
        cats.setdefault(key, {"n": 0, "ok": 0})
        cats[key]["n"] += 1
        cats[key]["ok"] += int(bool(r.get("taskSuccess")))
    return {
        "kind": "measured" if rows else "not_measured",
        "scope": "identical cursor-agent tasks across Ax baseline vs Neural prediction",
        "attribution": "Same agent, model, task prompt/repo snapshot; only neural.prediction.enabled differs",
        "suite": str(SUITE_PATH),
        "taskCountDefined": len(suite["tasks"]),
        "taskCountExecuted": len({r["taskId"] for r in rows}),
        "model": model,
        "agent": agent,
        "arms": per_arm,
        "byCategory": {
            f"{c}:{a}": {"n": v["n"], "successRate": v["ok"] / v["n"] if v["n"] else 0.0}
            for (c, a), v in sorted(cats.items())
        },
        "rows": rows,
        "productionDefaultsUnchanged": True,
        "semanticWeight": 0.0,
        "minimumTasksRequired": 30,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", default=str(DEFAULT_OUT))
    parser.add_argument("--suite", default=str(SUITE_PATH))
    parser.add_argument("--model", default=os.environ.get("AX_NEURAL_LIVE_MODEL", "composer-2-fast"))
    parser.add_argument("--limit", type=int, default=0, help="max tasks this run (0=all)")
    parser.add_argument("--offset", type=int, default=0)
    parser.add_argument("--merge", action="store_true", help="merge with existing --out rows")
    opts = parser.parse_args()

    suite = json.loads(Path(opts.suite).read_text(encoding="utf-8"))
    tasks = suite["tasks"][opts.offset :]
    if opts.limit:
        tasks = tasks[: opts.limit]

    binary = Path(os.environ.get("AX_BIN") or shutil.which("ax") or "")
    agent_bin = shutil.which("cursor-agent") or shutil.which("agent")
    if not binary.is_file() or agent_bin is None:
        report = {
            "kind": "not_measured",
            "reason": "ax binary or cursor-agent missing",
            "taskCountDefined": len(suite["tasks"]),
        }
        # Print only: the results file may hold rows from earlier batches.
        print(json.dumps(report, indent=2))
        return 2

    existing: list[dict] = []
    out_path = Path(opts.out)
    if opts.merge and out_path.is_file():
        prev = json.loads(out_path.read_text(encoding="utf-8"))
        existing = list(prev.get("rows") or [])

    done_keys = {(r.get("taskId"), r.get("arm")) for r in existing}
    rows = list(existing)
    base = Path(tempfile.mkdtemp(prefix="ax-neural-suite-"))
    try:
        for task in tasks:
            for arm_name, neural in ARMS.items():
                key = (task["id"], arm_name)
                if key in done_keys:
                    continue
                home = base / f"home-{task['id']}-{arm_name}"
                home.mkdir(parents=True, exist_ok=True)
                project = make_project(
                    binary, base, f"{task['id']}-{arm_name}", neural, home, task
                )
                result = agent_turn(task["prompt"], task["anchors"], project, opts.model, agent_bin)
                row = {
                    "taskId": task["id"],
                    "category": task["category"],
                    "arm": arm_name,
                    "neural": neural,
                    **result,
                }
                rows.append(row)
                done_keys.add(key)
                print(
                    json.dumps(
                        {
                            "taskId": task["id"],
                            "arm": arm_name,
                            "taskSuccess": result.get("taskSuccess"),
                            "ok": result.get("ok"),
                        }
                    ),
                    flush=True,
                )
                run(
                    [str(binary), "daemon", str(project), "stop"],
                    cwd=project,
                    env={"HOME": str(home)},
                )
                # checkpoint after each pair step
                report = summarize(rows, suite, opts.model, agent_bin)
                out_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    finally:
        shutil.rmtree(base, ignore_errors=True)

    report = summarize(rows, suite, opts.model, agent_bin)
    out_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(
        json.dumps(
            {
                "kind": report["kind"],
                "taskCountExecuted": report["taskCountExecuted"],
                "arms": report["arms"],
            },
            indent=2,
        )
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
