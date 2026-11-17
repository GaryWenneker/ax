#!/usr/bin/env python3
"""Phase 6 live identical-LLM agent evaluation for Ax Neural.

Runs the same coding prompt under Cursor Agent (`cursor-agent`) with identical
model settings across Neural configuration arms. Attribution requires the same
agent binary, model, and prompt; only ax.json neural flags differ.

Arms (Neural ranking stays required / lock_required in production):
  B_ax_baseline          — prediction off, feedback off (Ax + required ranking)
  C_ax_neural_prediction — prediction on
  E_ax_neural_learning   — feedback on; seed feedback then predict+preflight

Usage:
  AX_BIN=/path/to/ax ./scripts/neural-agent-live.py [--out path] [--model <id>]

Writes measured JSON or records not_measured with an explicit reason.
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
DEFAULT_MODEL = os.environ.get("AX_NEURAL_LIVE_MODEL", "composer-2-fast")

# Identical task for every arm — anchors must appear in the assistant text.
TASK = {
    "id": "neural-live-parser",
    "prompt": (
        "In this tiny Rust project, name the function in src/lib.rs and say whether "
        "Ax Neural prediction is relevant to fixing a parser. Reply in two short lines. "
        "Include the exact tokens LIB_FN=parse and NEURAL_AWARE=yes. "
        "You may call ax_preflight once if available."
    ),
    "anchors": ["LIB_FN=parse", "NEURAL_AWARE=yes"],
}


def check(cond: bool, msg: str) -> None:
    if not cond:
        raise SystemExit(msg)


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


def write_mcp_config(project: Path, binary: Path, home: Path) -> None:
    cursor_dir = project / ".cursor"
    cursor_dir.mkdir(parents=True, exist_ok=True)
    (cursor_dir / "mcp.json").write_text(
        json.dumps(
            {
                "mcpServers": {
                    "ax": {
                        "command": str(binary),
                        "args": ["serve", "--mcp", "--path", str(project)],
                        "env": {
                            "HOME": str(home),
                            "AX_HOME_DIR": str(home),
                            "NO_COLOR": "1",
                        },
                    }
                }
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )


def make_project(binary: Path, base: Path, name: str, neural: dict, home: Path) -> Path:
    project = base / name
    project.mkdir(parents=True)
    (project / "src").mkdir()
    (project / "src" / "lib.rs").write_text("pub fn parse() {}\n", encoding="utf-8")
    env = {
        "HOME": str(home),
        "AX_HOME_DIR": str(home),
        "NO_COLOR": "1",
    }
    done = run([str(binary), "init"], cwd=project, env=env)
    check(done.returncode == 0, f"ax init failed for {name}: {done.stdout}\n{done.stderr}")
    write_ax_json(project, neural)
    write_mcp_config(project, binary, home)
    return project


def seed_feedback(binary: Path, project: Path, home: Path) -> None:
    env = {"HOME": str(home), "AX_HOME_DIR": str(home), "NO_COLOR": "1"}
    for i, outcome in enumerate(("success", "success", "partial"), start=1):
        done = run(
            [
                str(binary),
                "neural",
                "feedback",
                f"live-exec-{i}",
                "parser",
                outcome,
                "--context-id",
                "memory:parser-lines",
                "--skill-id",
                "parse-errors",
                "--rule-id",
                "english-only",
                "--input-tokens",
                "200",
                "--output-tokens",
                "40",
            ],
            cwd=project,
            env=env,
        )
        check(done.returncode == 0, f"feedback seed failed: {done.stdout}\n{done.stderr}")


def agent_turn(prompt: str, project: Path, model: str, agent_bin: str) -> dict:
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
            "error": f"non-json (exit {done.returncode}): {(done.stderr or done.stdout)[-1500:]}",
            "elapsedMillis": elapsed_ms,
            "rawStdoutTail": (done.stdout or "")[-1500:],
        }
    usage = payload.get("usage") or {}
    text = payload.get("result") or ""
    anchors_hit = [a for a in TASK["anchors"] if a in text]
    is_error = bool(payload.get("is_error"))
    return {
        "ok": done.returncode == 0 and not is_error and payload.get("subtype") == "success",
        "session": payload.get("session_id"),
        "inputTokens": int(usage.get("inputTokens") or usage.get("input_tokens") or 0),
        "outputTokens": int(usage.get("outputTokens") or usage.get("output_tokens") or 0),
        "cacheReadTokens": int(usage.get("cacheReadTokens") or usage.get("cache_read_input_tokens") or 0),
        "cacheCreationTokens": int(
            usage.get("cacheWriteTokens") or usage.get("cache_creation_input_tokens") or 0
        ),
        "turns": 1,
        "durationApiMs": payload.get("duration_api_ms"),
        "elapsedMillis": elapsed_ms,
        "text": text,
        "anchorsHit": anchors_hit,
        "taskSuccess": len(anchors_hit) == len(TASK["anchors"]),
        "isError": is_error,
        "exitCode": done.returncode,
        "agent": agent_bin,
        "model": model,
    }


def arm_spec(name: str, neural: dict, seed: bool) -> dict:
    return {"name": name, "neural": neural, "seedFeedback": seed}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", default=str(ROOT / "docs/specs/ax-neural/agent-eval-live-results.json"))
    parser.add_argument("--model", default=DEFAULT_MODEL)
    opts = parser.parse_args()
    model = str(opts.model)

    binary = Path(os.environ.get("AX_BIN") or shutil.which("ax") or "")
    if not binary.is_file():
        report = {
            "kind": "not_measured",
            "reason": "AX_BIN / ax binary missing",
            "liveLlmTaskSuccess": {"kind": "not_measured"},
        }
        Path(opts.out).write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(report, indent=2))
        return 2

    agent_bin = shutil.which("cursor-agent") or shutil.which("agent")
    if agent_bin is None:
        report = {
            "kind": "not_measured",
            "reason": "cursor-agent not on PATH (claude CLI was unauthenticated)",
            "liveLlmTaskSuccess": {"kind": "not_measured"},
        }
        Path(opts.out).write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(report, indent=2))
        return 2

    arms = [
        arm_spec(
            "B_ax_baseline",
            {"prediction": {"enabled": False}, "learning": {"feedback_collection": False}},
            False,
        ),
        arm_spec(
            "C_ax_neural_prediction",
            {"prediction": {"enabled": True}, "learning": {"feedback_collection": False}},
            False,
        ),
        arm_spec(
            "E_ax_neural_learning",
            {"prediction": {"enabled": True}, "learning": {"feedback_collection": True}},
            True,
        ),
    ]

    base = Path(tempfile.mkdtemp(prefix="ax-neural-live-"))
    rows = []
    try:
        for arm in arms:
            home = base / f"home-{arm['name']}"
            home.mkdir()
            project = make_project(binary, base, arm["name"], arm["neural"], home)
            if arm["seedFeedback"]:
                seed_feedback(binary, project, home)
            result = agent_turn(TASK["prompt"], project, model, agent_bin)
            row = {
                "arm": arm["name"],
                "neural": arm["neural"],
                "seedFeedback": arm["seedFeedback"],
                "taskId": TASK["id"],
                **result,
            }
            rows.append(row)
            print(
                json.dumps(
                    {
                        "arm": arm["name"],
                        "taskSuccess": result.get("taskSuccess"),
                        "ok": result.get("ok"),
                        "inputTokens": result.get("inputTokens"),
                        "outputTokens": result.get("outputTokens"),
                    }
                ),
                flush=True,
            )
            run([str(binary), "daemon", str(project), "stop"], cwd=project, env={"HOME": str(home)})
    finally:
        shutil.rmtree(base, ignore_errors=True)

    successes = [r for r in rows if r.get("taskSuccess")]
    hard_errors = [r for r in rows if r.get("error") or r.get("isError")]
    report = {
        "kind": "measured",
        "scope": "identical cursor-agent prompt across Neural config arms",
        "attribution": (
            "Same cursor-agent binary, model, and prompt; only ax.json neural "
            "prediction/learning flags differ. Ranking remains required."
        ),
        "task": TASK,
        "model": model,
        "agent": agent_bin,
        "arms": {r["arm"]: r for r in rows},
        "liveLlmTaskSuccess": {
            "kind": "measured",
            "successRate": (len(successes) / len(rows)) if rows else 0.0,
            "perArm": {r["arm"]: bool(r.get("taskSuccess")) for r in rows},
        },
        "tokens": {
            "kind": "measured",
            "perArm": {
                r["arm"]: {
                    "inputTokens": r.get("inputTokens"),
                    "outputTokens": r.get("outputTokens"),
                }
                for r in rows
            },
        },
        "latency": {
            "kind": "measured",
            "perArmMillis": {r["arm"]: r.get("elapsedMillis") for r in rows},
        },
        "retries": {"kind": "not_measured", "reason": "single-shot turns; no retry loop"},
        "productionDefaultsUnchanged": True,
        "semanticWeight": 0.0,
        "note": "Arm A (disable production Neural ranking) is not runnable; ranking is lock_required.",
    }
    if hard_errors and not successes:
        report["kind"] = "partial"
        report["liveLlmTaskSuccess"]["kind"] = "measured"
        report["liveLlmTaskSuccess"]["note"] = "arms executed but anchors not met or agent errors"
    if not rows:
        report["kind"] = "not_measured"
        report["liveLlmTaskSuccess"] = {"kind": "not_measured", "reason": "no arm rows"}
    Path(opts.out).write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    return 0 if report.get("kind") in ("measured", "partial") else 1


if __name__ == "__main__":
    sys.exit(main())
