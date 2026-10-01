#!/usr/bin/env python3
"""L7: live Claude Code conversations against ax MCP, conversation cache off vs on.

Each conversation is three `claude -p` turns joined with `--resume`; turn 3 re-asks turn 1.
User-level Claude settings are skipped so global ax hooks neither log into the real usage
database nor rewrite the real active session. The ax server gets its own AX_HOME_DIR holding the conversation id, the way the Cursor hook
provides one, and its own AX_USAGE_DB. Claude keeps the real HOME and login.
Arms alternate per conversation. Each arm indexes its own copy of the two crates the questions are about (HEAD), because
`ax serve --mcp` proxies to one daemon per project and the daemon keeps the starter's env.

Pass: median total input tokens per conversation drop by at least 15%, and the on arm
answers correctly at least as often as the off arm. Exits nonzero otherwise or on any run error.
Usage: AX_BIN=/path/to/ax python3 reuse_live.py [--runs 5] [--out report.json]
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import statistics
import subprocess
import sys
import tempfile
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SOURCES = ["crates/ax-usage", "crates/ax-mcp"]
TURNS = [
    "In this repo, how does ax decide whether a repeated MCP graph call is answered from the "
    "per-conversation cache instead of running again? Name the functions involved.",
    "Which function computes the index fingerprint, and what exactly does it hash?",
    "Remind me again: how does ax decide whether a repeated graph call is answered from the "
    "per-conversation cache? List the functions once more.",
]
SUFFIX = " Use the ax graph tools. Do not edit files. Answer with the facts you found."
ANCHORS = ["index_matches", "index_fingerprint"]
BUDGET_USD = "0.50"


def run(cmd: list[str], cwd: Path, env: dict[str, str]) -> None:
    done = subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True)
    if done.returncode != 0:
        raise SystemExit(f"command failed ({done.returncode}): {' '.join(cmd)}\n{done.stderr[-2000:]}")


def make_project(binary: str, base: Path, name: str, server_env: dict[str, str]) -> Path:
    project = base / name
    project.mkdir()
    archive = subprocess.run(["git", "archive", "HEAD", *SOURCES], cwd=ROOT, capture_output=True, check=True)
    subprocess.run(["tar", "-x", "-C", str(project)], input=archive.stdout, check=True)
    env = {**os.environ, **server_env}
    run([binary, "init", str(project)], project, env)
    run([binary, "index", str(project), "--quiet"], project, env)
    return project


def server_env(home: Path, cache_on: bool) -> dict[str, str]:
    (home / ".ax").mkdir(parents=True, exist_ok=True)
    return {
        "AX_HOME_DIR": str(home),
        "AX_USAGE_DB": str(home / "usage.db"),
        "AX_CONTEXT_CACHE": "on" if cache_on else "off",
        "NO_COLOR": "1",
    }


def claude_turn(prompt: str, config: Path, project: Path, session: str | None) -> dict:
    cmd = [
        "claude", "-p", "--setting-sources", "project", "--strict-mcp-config", "--mcp-config", str(config),
        "--output-format", "json",
        "--dangerously-skip-permissions", "--disallowedTools", "Edit,Write,NotebookEdit",
        "--max-budget-usd", BUDGET_USD,
    ]
    if session:
        cmd += ["--resume", session]
    done = subprocess.run(cmd + [prompt], cwd=project, capture_output=True, text=True)
    try:
        payload = json.loads(done.stdout)
    except json.JSONDecodeError as err:
        raise SystemExit(f"claude returned no JSON (exit {done.returncode}): {done.stderr[-1000:]}") from err
    if done.returncode != 0 or payload.get("is_error"):
        raise SystemExit(f"claude run failed: {json.dumps(payload)[:1000]}")
    usage = payload.get("usage") or {}
    return {
        "session": payload["session_id"],
        "input": int(usage.get("input_tokens") or 0),
        "cache_read": int(usage.get("cache_read_input_tokens") or 0),
        "cache_creation": int(usage.get("cache_creation_input_tokens") or 0),
        "output": int(usage.get("output_tokens") or 0),
        "turns": int(payload.get("num_turns") or 0),
        "cost": float(payload.get("total_cost_usd") or 0.0),
        "text": payload.get("result") or "",
    }


def ax_hits(home: Path, conversation: str) -> int:
    import sqlite3

    db = home / "usage.db"
    if not db.exists():
        return 0
    with sqlite3.connect(db) as conn:
        row = conn.execute(
            "SELECT COALESCE(SUM(hits), 0) FROM mcp_reuse_cache WHERE conversation LIKE ?",
            (conversation + "\x1f%",),
        ).fetchone()
    return int(row[0])


def conversation(arm: dict, index: int) -> dict:
    conv_id = f"l7-{arm['name']}-{index}-{uuid.uuid4().hex[:8]}"
    (arm["home"] / ".ax" / "active-cursor-session").write_text(conv_id + "\n", encoding="utf-8")
    session = None
    turns = []
    for prompt in TURNS:
        turn = claude_turn(prompt + SUFFIX, arm["config"], arm["project"], session)
        session = turn["session"]
        turns.append(turn)
    total_in = sum(t["input"] + t["cache_read"] + t["cache_creation"] for t in turns)
    answer = turns[-1]["text"]
    return {
        "arm": arm["name"],
        "conversation": conv_id,
        "total_input_tokens": total_in,
        "output_tokens": sum(t["output"] for t in turns),
        "agent_turns": sum(t["turns"] for t in turns),
        "cost_usd": round(sum(t["cost"] for t in turns), 4),
        "correct": all(a in answer for a in ANCHORS),
        "ax_cache_hits": ax_hits(arm["home"], conv_id),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--out")
    opts = parser.parse_args()
    binary = os.environ.get("AX_BIN")
    if not binary or not Path(binary).is_file() or shutil.which("claude") is None:
        print("AX_BIN must point at the ax binary, and claude must be on PATH", file=sys.stderr)
        return 2
    base = Path(tempfile.mkdtemp(prefix="ax-reuse-live-"))
    arms = []
    for name, on in (("off", False), ("on", True)):
        home = base / f"home-{name}"
        env = server_env(home, on)
        project = make_project(binary, base, f"proj-{name}", env)
        config = base / f"mcp-{name}.json"
        config.write_text(json.dumps({"mcpServers": {"ax": {
            "command": binary, "args": ["serve", "--mcp", "--path", str(project)], "env": env}}}), encoding="utf-8")
        arms.append({"name": name, "home": home, "project": project, "config": config})
    rows = []
    try:
        for i in range(opts.runs):
            for arm in arms:
                row = conversation(arm, i)
                rows.append(row)
                print(json.dumps(row), flush=True)
    finally:
        for arm in arms:
            subprocess.run([binary, "daemon", str(arm["project"]), "stop"], capture_output=True)
        shutil.rmtree(base, ignore_errors=True)

    def arm_rows(name: str) -> list[dict]:
        return [r for r in rows if r["arm"] == name]

    off, on = arm_rows("off"), arm_rows("on")
    med_off = statistics.median(r["total_input_tokens"] for r in off)
    med_on = statistics.median(r["total_input_tokens"] for r in on)
    drop = 100 * (med_off - med_on) / med_off
    ok_off, ok_on = sum(r["correct"] for r in off), sum(r["correct"] for r in on)
    report = {
        "runs_per_arm": opts.runs,
        "median_total_input_off": med_off,
        "median_total_input_on": med_on,
        "median_drop_pct": round(drop, 1),
        "correct_off": ok_off,
        "correct_on": ok_on,
        "hits_on": [r["ax_cache_hits"] for r in on],
        "hits_off": [r["ax_cache_hits"] for r in off],
        "cost_usd": round(sum(r["cost_usd"] for r in rows), 2),
        "pass": drop >= 15 and ok_on >= ok_off,
        "rows": rows,
    }
    text = json.dumps(report, indent=2)
    print(text)
    if opts.out:
        Path(opts.out).write_text(text + "\n", encoding="utf-8")
    return 0 if report["pass"] else 1


if __name__ == "__main__":
    sys.exit(main())
