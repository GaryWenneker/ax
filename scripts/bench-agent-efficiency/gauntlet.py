#!/usr/bin/env python3
"""Deterministic WITH vs WITHOUT token gauntlet, plus an optional Claude arm.

Usage:
  gauntlet.py            run every task, write out/report.json, out/report.md, out/summary.md
  gauntlet.py --freeze   record the current without-block hashes (only after a reviewed change)
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import sqlite3
import statistics
import subprocess
import sys
import tempfile
from pathlib import Path

import yaml

from mcp_client import McpError, McpSession

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
TASKS = HERE / "tasks.yaml"
HASHES = HERE / "without_hashes.json"
OUT = HERE / "out"
ANSI = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")
AX = os.environ.get("AX_BIN", "ax")
CONTROL_LIMIT_PCT = 5.0
MIN_NET_PCT = 50.0
SENTENCE_KIND = "note"


def tokens(text: str) -> int:
    import tiktoken

    return len(tiktoken.get_encoding("o200k_base").encode(text))


def run(cmd: list[str], cwd: Path) -> str:
    env = os.environ.copy()
    env["NO_COLOR"] = "1"
    env["AX_ASCII"] = "1"
    proc = subprocess.run(cmd, cwd=cwd, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
    text = ANSI.sub("", proc.stdout or "")
    text = re.sub(r"\n+\|\n! Update available[\s\S]*?L -+\n*", "\n", text)
    if proc.returncode != 0:
        text += f"\n[exit {proc.returncode}]\n"
    return text


def rg_read(repo: Path, pattern: str, glob: str) -> str:
    listed = run(["rg", "-l", "--glob", glob, pattern, "."], repo)
    paths = [line.strip() for line in listed.splitlines() if line.strip() and not line.startswith("[exit")]
    chunks = [f"$ rg -l --glob {glob} {pattern}\n{listed}"]
    for rel in sorted(paths):
        path = repo / rel
        if path.is_file():
            chunks.append(f"\n--- {rel} ---\n")
            chunks.append(path.read_text(encoding="utf-8", errors="replace"))
    return "".join(chunks)


def cat_dir(repo: Path, directory: str, glob: str) -> str:
    chunks = []
    for path in sorted((repo / directory).glob(glob)):
        chunks.append(f"\n--- {path.relative_to(repo)} ---\n")
        chunks.append(path.read_text(encoding="utf-8", errors="replace"))
    return "".join(chunks)


def skill_scan(repo: Path, root: str, full: str, head_lines: int) -> str:
    full_path = repo / full
    chunks = [f"\n--- {full} ---\n", full_path.read_text(encoding="utf-8", errors="replace")]
    for path in sorted((repo / root).glob("*/SKILL.md")):
        if path.resolve() == full_path.resolve():
            continue
        lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
        chunks.append(f"\n--- {path.relative_to(repo)} (first {head_lines} lines) ---\n")
        chunks.append("\n".join(lines[:head_lines]) + "\n")
    return "".join(chunks)


def sqlite_memories(db: Path) -> str:
    con = sqlite3.connect(db)
    try:
        rows = con.execute("SELECT title, body FROM memories ORDER BY created_at").fetchall()
    finally:
        con.close()
    return "".join(f"\n--- {title} ---\n{body}\n" for title, body in rows)


class Context:
    """Per-task state: fixture paths plus one MCP session per project."""

    def __init__(self, repo: Path, fixture: Path, db: Path) -> None:
        self.repo = repo
        self.fixture = fixture
        self.db = db
        self.sessions: dict[str, McpSession] = {}

    def session(self, project: str) -> McpSession:
        if project not in self.sessions:
            path = self.fixture if project == "fixture" else self.repo
            self.sessions[project] = McpSession(path, AX)
        return self.sessions[project]

    def close(self) -> None:
        for session in self.sessions.values():
            session.close()
        self.sessions.clear()


def load_prompts(name: str) -> list[dict]:
    data = json.loads((HERE / name).read_text(encoding="utf-8"))
    prompts = data["prompts"]
    if not prompts:
        raise SystemExit(f"{name}: no prompts")
    return prompts


def step_args(step: dict, turn: int) -> dict:
    if "args_by_turn" in step:
        return step["args_by_turn"][(turn - 1) % len(step["args_by_turn"])]
    if "prompts_file" in step:
        prompts = load_prompts(step["prompts_file"])
        return dict(prompts[(turn - 1) % len(prompts)])
    return step.get("args") or {}


def run_step(step: dict, ctx: Context, turn: int = 1) -> str:
    kind = step["kind"]
    if kind == "rg_read":
        return rg_read(ctx.repo, step["pattern"], step["glob"])
    if kind == "cat_dir":
        root = ctx.fixture if step.get("project") == "fixture" else ctx.repo
        return cat_dir(root, step["dir"], step["glob"])
    if kind == "fixture_write":
        target = ctx.fixture / step["path"]
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(step["text"], encoding="utf-8")
        ax_quiet(["policy", "index"], ctx.fixture)
        return ""
    if kind == "skill_scan":
        return skill_scan(ctx.repo, step["root"], step["full"], int(step["head_lines"]))
    if kind == "sqlite_memories":
        return sqlite_memories(ctx.db)
    if kind == "mcp":
        text = ctx.session(step.get("project", "repo")).call_text(step["tool"], step_args(step, turn))
        return f"\n[{step['tool']}]\n{text}\n"
    if kind == "ax":
        cwd = ctx.fixture if step.get("cwd") == "fixture" else ctx.repo
        return f"$ ax {' '.join(step['args'])}\n" + run([AX, *step["args"]], cwd)
    raise SystemExit(f"unknown step kind: {kind}")


def execute(steps: list[dict], turns: int, ctx: Context) -> str:
    """Turn 1 runs every step; later turns rerun only steps marked every_turn."""
    parts = []
    for turn in range(1, turns + 1):
        for step in steps:
            if turn == 1 or step.get("every_turn"):
                parts.append(run_step(step, ctx, turn))
    return "".join(parts)


def anchors_ok(text: str, include: list[str], exclude: list[str]) -> tuple[bool, list[str]]:
    missing = [item for item in include if item not in text]
    present = [item for item in exclude if item in text]
    return (not missing and not present), missing + [f"excluded present: {item}" for item in present]


def without_hash(task: dict) -> str:
    frozen: dict = {"turns": int(task.get("turns", 1)), "without": task["without"]}
    files = sorted({step["prompts_file"] for step in task["with"] if "prompts_file" in step})
    if files:
        frozen["prompts"] = [load_prompts(name) for name in files]
    return hashlib.sha256(json.dumps(frozen, sort_keys=True).encode("utf-8")).hexdigest()


def check_hashes(tasks: list[dict]) -> list[str]:
    if not HASHES.is_file():
        return [f"{HASHES.name} is missing; run with --freeze once after review"]
    stored = json.loads(HASHES.read_text(encoding="utf-8"))
    problems = []
    for task in tasks:
        expected = stored.get(task["id"])
        if expected is None:
            problems.append(f"{task['id']}: no frozen without-hash")
        elif expected != without_hash(task):
            problems.append(f"{task['id']}: without-block changed")
    return problems


def ax_quiet(args: list[str], cwd: Path) -> None:
    proc = subprocess.run([AX, *args], cwd=cwd, stdin=subprocess.DEVNULL, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
    if proc.returncode != 0:
        raise SystemExit(f"ax {' '.join(args[:2])} failed:\n{proc.stdout}")


def prepare_fixture(sentence: str) -> tuple[Path, Path]:
    fixture = Path(tempfile.mkdtemp(prefix="ax-gauntlet-"))
    (fixture / "lib.rs").write_text("fn fixture_marker() {}\n", encoding="utf-8")
    ax_quiet(["init", str(fixture)], fixture)
    ax_quiet(["remember", sentence, "--title", "Gauntlet vault hint", "--kind", SENTENCE_KIND], fixture)
    # Filler notes make the without-arm a store dump. Only the sentence above is the anchor.
    for index in range(24):
        ax_quiet(
            [
                "remember",
                f"Unrelated fixture note {index}: the build cache key is filler-{index:02d}-not-the-hint.",
                "--title",
                f"Filler {index}",
                "--kind",
                SENTENCE_KIND,
            ],
            fixture,
        )
    db = fixture / ".ax" / "ax.db"
    if not db.is_file():
        raise SystemExit(f"fixture db missing: {db}")
    return fixture, db


def pct(part: int, base: int) -> float:
    return 100.0 * part / base if base > 0 else 0.0


def run_task(task: dict, ctx: Context) -> dict:
    turns = int(task.get("turns", 1))
    try:
        without_text = execute(task["without"], turns, ctx)
        with_text = execute(task["with"], turns, ctx)
    finally:
        ctx.close()
    without_ok, without_problems = anchors_ok(without_text, task["include"], [])
    with_include = list(task["include"]) + list(task.get("include_with") or [])
    with_ok, with_problems = anchors_ok(with_text, with_include, task.get("exclude_from_with") or [])
    ok = without_ok and with_ok
    w_out, w_in = tokens(without_text), tokens(with_text)
    net = w_out - w_in if ok else 0
    return {
        "id": task["id"],
        "tier": task["tier"],
        "feature": task["feature"],
        "turns": turns,
        "prompt": task["prompt"],
        "without_tokens": w_out,
        "with_tokens": w_in,
        "net_tokens": net,
        "net_pct": pct(net, w_out),
        "pass": ok,
        "problems": without_problems + with_problems,
        "shortfall": task.get("shortfall") or "",
        "headroom": task.get("headroom") or "",
    }


def negative_control(task: dict, ctx: Context) -> dict:
    turns = int(task.get("turns", 1))
    first = tokens(execute(task["without"], turns, ctx))
    second = tokens(execute(task["without"], turns, ctx))
    delta = abs(first - second)
    return {"id": task["id"], "first_tokens": first, "second_tokens": second, "delta_tokens": delta, "delta_pct": pct(delta, max(first, 1))}


def feature_rollup(rows: list[dict]) -> list[dict]:
    out = []
    for feature in sorted({row["feature"] for row in rows}):
        group = [row for row in rows if row["feature"] == feature and row["pass"]]
        base = sum(row["without_tokens"] for row in group)
        net = sum(row["net_tokens"] for row in group)
        out.append({"feature": feature, "without_tokens": base, "net_tokens": net, "net_pct": pct(net, base)})
    return out


def gate(rows: list[dict], features: list[dict], control: dict, hash_problems: list[str]) -> list[str]:
    failures = list(hash_problems)
    failures += [f"{row['id']}: anchors failed ({'; '.join(row['problems'])})" for row in rows if not row["pass"]]
    failures += [f"{row['id']}: net {row['net_pct']:.1f}% < {MIN_NET_PCT}%" for row in rows if row["pass"] and row["net_pct"] < MIN_NET_PCT]
    failures += [f"feature {f['feature']}: net {f['net_pct']:.1f}% < {MIN_NET_PCT}%" for f in features if f["net_pct"] < MIN_NET_PCT]
    if control["delta_pct"] > CONTROL_LIMIT_PCT:
        failures.append(f"negative control {control['id']}: {control['delta_pct']:.1f}% > {CONTROL_LIMIT_PCT}%")
    return failures


def load_previous() -> dict[str, dict]:
    prev = OUT / "report.prev.json"
    if not prev.is_file():
        return {}
    data = json.loads(prev.read_text(encoding="utf-8"))
    return {row["id"]: row for row in data.get("tasks", [])}


def comparison_table(rows: list[dict], previous: dict[str, dict]) -> list[str]:
    lines = [
        "| Task | Without (prev) | Without (now) | With ax (prev) | With ax (now) | Net % (prev) | Net % (now) | Change (pts) |",
        "|---|---:|---:|---:|---:|---:|---:|---:|",
    ]
    for row in rows:
        old = previous.get(row["id"])
        if old is None:
            lines.append(f"| {row['id']} | new | {row['without_tokens']:,} | new | {row['with_tokens']:,} | new | {row['net_pct']:.1f} | new |")
            continue
        lines.append(
            f"| {row['id']} | {old['without_tokens']:,} | {row['without_tokens']:,} | {old['with_tokens']:,} | {row['with_tokens']:,} "
            f"| {old['net_pct']:.1f} | {row['net_pct']:.1f} | {row['net_pct'] - old['net_pct']:+.1f} |"
        )
    return lines


def change_text(row: dict) -> str:
    if not row["pass"]:
        return "Anchors failed: " + "; ".join(row["problems"])
    if row["net_tokens"] < 0:
        return row["shortfall"] or "With-arm is larger than the without-arm."
    return row["headroom"] or "none"


def summary_tables(rows: list[dict], features: list[dict], previous: dict[str, dict]) -> list[str]:
    has_prev = bool(previous)
    head = "| Task | Feature | Without | With ax | Net | Net % |" + (" Previous net |" if has_prev else "") + " What to change |"
    sep = "|---|---|---:|---:|---:|---:|" + ("---:|" if has_prev else "") + "---|"
    lines = [head, sep]
    for row in rows:
        prev = f" {previous[row['id']]['net_tokens']:,} |" if has_prev and row["id"] in previous else (" new |" if has_prev else "")
        lines.append(
            f"| {row['id']} | {row['feature']} | {row['without_tokens']:,} | {row['with_tokens']:,} | {row['net_tokens']:,} | {row['net_pct']:.1f} |{prev} {change_text(row)} |"
        )
    lines += ["", "| Feature | Net | Net % |", "|---|---:|---:|"]
    for feature in features:
        lines.append(f"| {feature['feature']} | {feature['net_tokens']:,} | {feature['net_pct']:.1f} |")
    if has_prev:
        lines += ["", "Comparison with the previous run:", ""] + comparison_table(rows, previous)
    return lines


def write_reports(rows, features, control, failures, live, cross, previous) -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    passed = [row for row in rows if row["pass"]]
    without_sum = sum(row["without_tokens"] for row in passed)
    with_sum = sum(row["with_tokens"] for row in passed)
    net_sum = sum(row["net_tokens"] for row in passed)
    totals = {"passed": len(passed), "tasks": len(rows), "without_tokens": without_sum, "with_tokens": with_sum, "net_tokens": net_sum, "net_pct": pct(net_sum, without_sum)}
    payload = {"totals": totals, "tasks": rows, "features": features, "negative_control": control, "gate_failures": failures, "live": live, "savings_cross_check": cross}
    (OUT / "report.json").write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")

    verdict = "green" if not failures else "red"
    head = [
        f"Gate: **{verdict}**. Passed tasks {len(passed)}/{len(rows)}. Tokens without {without_sum:,}, with ax {with_sum:,}, net {net_sum:,} ({totals['net_pct']:.1f}%).",
        "",
    ]
    tables = summary_tables(rows, features, previous)
    (OUT / "summary.md").write_text("\n".join(["# Savings gauntlet summary", "", *head, *tables, ""]), encoding="utf-8")

    lines = [
        "# Ax savings gauntlet",
        "",
        "Counts are o200k BPE. The without-arm is a fixed file read (frozen by `without_hashes.json`).",
        "The with-arm is the MCP `content.text` an agent receives, one fresh MCP session per task.",
        "Net is without minus with. A task or feature below 50% net fails the gate.",
        "This is not the `ax savings` counterfactual (whole file minus graph response).",
        "",
        *head,
        *tables,
        "",
        "## Gate",
        "",
    ]
    lines += [f"- {failure}" for failure in failures] or ["All gates pass: anchors, every task and feature net at least 50%, negative control, frozen without-blocks."]
    lines += [
        "",
        "## Negative control",
        "",
        f"Task `{control['id']}` ran its without-recipe twice: {control['first_tokens']:,} and {control['second_tokens']:,} tokens ({control['delta_pct']:.2f}% apart). Limit {CONTROL_LIMIT_PCT}%.",
        "",
        "## Live Claude arm",
        "",
        live.get("summary", ""),
        "",
    ]
    if live.get("rows"):
        lines += ["| Task | Arm | Median input tokens | Median tool calls | Pass |", "|---|---|---:|---:|---|"]
        lines += [f"| {r['id']} | {r['arm']} | {r['median_input_tokens']} | {r['median_tool_calls']} | {r['pass']} |" for r in live["rows"]]
    lines += ["", "## ax savings cross-check", "", "Cumulative product metric for this machine, not the headline of this gauntlet.", "", "```", cross.strip()[:4000], "```", ""]
    (OUT / "report.md").write_text("\n".join(lines), encoding="utf-8")


def claude_once(prompt: str, mcp_config: Path, repo: Path) -> dict:
    proc = subprocess.run(
        ["claude", "-p", "--strict-mcp-config", "--mcp-config", str(mcp_config), "--output-format", "json", "--dangerously-skip-permissions", "--max-budget-usd", "0.50", prompt],
        cwd=repo,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    try:
        payload = json.loads(proc.stdout or "")
    except json.JSONDecodeError:
        payload = {}
    usage = payload.get("usage") or {}
    result = payload.get("result")
    error = proc.returncode != 0 or bool(payload.get("is_error")) or payload.get("terminal_reason") == "api_error"
    return {
        "error": error,
        "input_tokens": int(usage.get("input_tokens") or 0),
        "tool_calls": int(payload.get("num_turns") or 0),
        "text": result if isinstance(result, str) else (proc.stdout or "")[:8000],
        "stderr": (proc.stderr or "")[:500],
    }


def live_arm(tasks: list[dict], repo: Path) -> dict:
    if os.environ.get("AX_GAUNTLET_LIVE", "1") == "0":
        return {"summary": "Skipped: AX_GAUNTLET_LIVE=0.", "rows": []}
    if shutil.which("claude") is None:
        return {"summary": "Skipped: `claude` is not on PATH.", "rows": []}
    OUT.mkdir(parents=True, exist_ok=True)
    empty, full = OUT / "mcp-empty.json", OUT / "mcp-ax.json"
    empty.write_text(json.dumps({"mcpServers": {}}), encoding="utf-8")
    full.write_text(json.dumps({"mcpServers": {"ax": {"command": AX, "args": ["serve", "--mcp", "--path", str(repo)]}}}), encoding="utf-8")
    runs = int(os.environ.get("AX_GAUNTLET_LIVE_RUNS", "3"))
    arms = (
        ("without", empty, " Do not use ax. Use search and file reads only. Answer with the facts you found."),
        ("with", full, " Use ax graph, policy, and memory tools. Answer with the facts you found."),
    )
    rows = []
    for task in tasks:
        for arm, cfg, extra in arms:
            samples = []
            for _ in range(runs):
                sample = claude_once(task["prompt"] + extra, cfg, repo)
                if sample["error"] and sample["input_tokens"] == 0:
                    reason = (sample["text"] or sample["stderr"])[:300]
                    return {"summary": f"Skipped: Claude Code did not complete a run ({task['id']}, {arm}): {reason}", "rows": rows}
                samples.append(sample)
            include = list(task["include"]) + (list(task.get("include_with") or []) if arm == "with" else [])
            exclude = list(task.get("exclude_from_with") or []) if arm == "with" else []
            ok, problems = anchors_ok("\n".join(s["text"] for s in samples), include, exclude)
            rows.append(
                {
                    "id": task["id"],
                    "arm": arm,
                    "median_input_tokens": int(statistics.median(s["input_tokens"] for s in samples)),
                    "median_tool_calls": int(statistics.median(s["tool_calls"] for s in samples)),
                    "pass": ok,
                    "problems": problems,
                }
            )
    return {"summary": f"Median of {runs} Claude Code headless runs per arm.", "rows": rows}


def main(argv: list[str]) -> int:
    spec = yaml.safe_load(TASKS.read_text(encoding="utf-8"))
    tasks = spec["tasks"]
    if "--freeze" in argv:
        HASHES.write_text(json.dumps({t["id"]: without_hash(t) for t in tasks}, indent=2) + "\n", encoding="utf-8")
        print(f"froze {len(tasks)} without-blocks into {HASHES}")
        return 0
    status = subprocess.run([AX, "status"], cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
    if status.returncode != 0:
        print(status.stdout)
        return 1
    hash_problems = check_hashes(tasks)

    OUT.mkdir(parents=True, exist_ok=True)
    if (OUT / "report.json").is_file():
        shutil.copyfile(OUT / "report.json", OUT / "report.prev.json")
    previous = load_previous()

    fixture, db = prepare_fixture(spec["fixture_sentence"])
    ctx = Context(ROOT, fixture, db)
    rows = []
    try:
        for task in tasks:
            try:
                row = run_task(task, ctx)
            except McpError as exc:
                row = {"id": task["id"], "tier": task["tier"], "feature": task["feature"], "turns": int(task.get("turns", 1)), "prompt": task["prompt"], "without_tokens": 0, "with_tokens": 0, "net_tokens": 0, "net_pct": 0.0, "pass": False, "problems": [f"MCP error: {exc}"], "shortfall": task.get("shortfall") or "", "headroom": task.get("headroom") or ""}
            rows.append(row)
            print(f"{row['id']}: without={row['without_tokens']} with={row['with_tokens']} net={row['net_tokens']} pass={row['pass']}")
        control = negative_control(tasks[0], ctx)
    finally:
        ctx.close()
        shutil.rmtree(fixture, ignore_errors=True)

    features = feature_rollup(rows)
    failures = gate(rows, features, control, hash_problems)
    cross = run([AX, "savings", "--period", "day", "--json"], ROOT)
    live = live_arm(tasks, ROOT)
    write_reports(rows, features, control, failures, live, cross, previous)
    print((OUT / "summary.md").read_text(encoding="utf-8"))
    for failure in failures:
        print(f"GATE: {failure}", file=sys.stderr)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
