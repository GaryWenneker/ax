#!/usr/bin/env python3
"""L5/L6: scripted multi-turn MCP sessions against a real `ax` binary, cache off vs on.

L5  same 5-turn script twice (AX_CONTEXT_CACHE=off, then on). Asserts:
    - every miss reply is identical across arms, and every hit expands to the off-arm reply
    - the on-arm sends fewer response tokens in total
    - every repeated call is a hit
    - a second conversation id gets zero hits for the same calls
L6  explore, edit the cited file, sync, ask again. Asserts the last call is a miss with the
    new code; any hit fails. Reports net savings for the session including the miss.

Exits nonzero on any failed assertion or on any harness error. Prints a JSON report.
Usage: AX_BIN=/path/to/ax python3 reuse_session.py [--out report.json] [--control-no-edit]
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import tiktoken

sys.path.insert(0, str(Path(__file__).resolve().parent))
from mcp_client import McpSession  # noqa: E402

ENC = tiktoken.get_encoding("o200k_base")
HIT = "[ax cache hit]"


def tokens(text: str) -> int:
    return len(ENC.encode(text))


def fixture(root: Path) -> None:
    src = root / "src"
    src.mkdir(parents=True)
    body = "\n".join(f"    let step_{i} = value.wrapping_mul({i + 3}).rotate_left({i % 7});" for i in range(40))
    (src / "pricing.rs").write_text(
        "pub fn apply_discount(value: u64, pct: u64) -> u64 {\n"
        + body
        + "\n    value - value * pct / 100\n}\n\n"
        "pub fn tax_for(value: u64) -> u64 {\n" + body + "\n    value / 5\n}\n",
        encoding="utf-8",
    )
    (src / "orders.rs").write_text(
        "use crate::pricing::{apply_discount, tax_for};\n\n"
        "pub fn sum_lines(lines: &[u64]) -> u64 {\n" + body.replace("value", "lines[0]") + "\n    lines.iter().sum()\n}\n\n"
        "pub fn compute_total(lines: &[u64], pct: u64) -> u64 {\n"
        "    let gross = sum_lines(lines);\n"
        "    let net = apply_discount(gross, pct);\n"
        "    net + tax_for(net)\n}\n",
        encoding="utf-8",
    )
    (src / "report.rs").write_text(
        "use crate::orders::compute_total;\n\n"
        "pub fn monthly_report(lines: &[u64]) -> String {\n"
        "    format!(\"total={}\", compute_total(lines, 10))\n}\n",
        encoding="utf-8",
    )
    (src / "lib.rs").write_text("pub mod orders;\npub mod pricing;\npub mod report;\n", encoding="utf-8")


def env_for(home: Path, cache_on: bool, conversation: str) -> dict:
    (home / ".ax").mkdir(parents=True, exist_ok=True)
    (home / ".ax" / "active-cursor-session").write_text(conversation + "\n", encoding="utf-8")
    env = os.environ.copy()
    env["HOME"] = str(home)
    env["AX_USAGE_DB"] = str(home / "usage.db")
    env["AX_CONTEXT_CACHE"] = "on" if cache_on else "off"
    env["NO_COLOR"] = "1"
    return env


def run(cmd: list[str], cwd: Path, env: dict) -> None:
    done = subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True)
    if done.returncode != 0:
        raise SystemExit(f"command failed ({done.returncode}): {' '.join(cmd)}\n{done.stdout}\n{done.stderr}")


PROJECTS: list[Path] = []


def make_project(binary: str, root: Path, env: dict) -> Path:
    """Each arm gets its own project: `ax serve --mcp` proxies to one daemon per project root,
    and that daemon keeps the environment of whichever arm started it."""
    fixture(root)
    run([binary, "init", str(root)], root, env)
    run([binary, "index", str(root), "--quiet"], root, env)
    PROJECTS.append(root)
    return root


def stop_daemons(binary: str, failures: list[str]) -> None:
    for root in PROJECTS:
        done = subprocess.run([binary, "daemon", str(root), "stop"], capture_output=True, text=True)
        check(done.returncode == 0, f"daemon stop failed for {root}: {done.stderr.strip()}", failures)


SCRIPT = [
    [("ax_explore", {"query": "how does compute_total work"}), ("ax_node", {"name": "compute_total"})],
    [("ax_callers", {"name": "compute_total"}), ("ax_node", {"name": "apply_discount"})],
    [("ax_node", {"name": "compute_total"}), ("ax_explore", {"query": "how does compute_total work"})],
    [("ax_node", {"name": "apply_discount"}), ("ax_callees", {"name": "compute_total"})],
    [("ax_explore", {"query": "how does compute_total work"}), ("ax_node", {"name": "compute_total"})],
]


class Arm:
    def __init__(self, binary: str, project: Path, env: dict) -> None:
        os.environ.update(env)
        self.session = McpSession(project, binary)
        self.calls: list[dict] = []

    def call(self, tool: str, args: dict) -> dict:
        result = self.session.request("tools/call", {"name": tool, "arguments": args})
        text = "\n".join(i.get("text", "") for i in result.get("content") or [] if i.get("type") == "text")
        meta = result.get("structuredContent") or {}
        rec = {"tool": tool, "args": args, "text": text, "tokens": tokens(text), "hit": text.startswith(HIT), "meta": meta}
        self.calls.append(rec)
        return rec

    def expand(self, cache_id: str) -> str:
        out, offset = [], 0
        while True:
            result = self.session.request("tools/call", {"name": "ax_expand", "arguments": {"id": cache_id, "offset": offset, "limit": 12000}})
            text = "\n".join(i.get("text", "") for i in result.get("content") or [] if i.get("type") == "text")
            marker = "\n\n[ax context cache] more remains; ax_expand id offset="
            if marker in text:
                page, nxt = text.split(marker, 1)
                out.append(page)
                offset = int(nxt.strip())
                continue
            out.append(text)
            return "".join(out)

    def close(self) -> None:
        self.session.close()


def play(arm: Arm) -> list[dict]:
    replies = []
    for turn in SCRIPT:
        arm.call("ax_preflight", {"prompt": "continue the task"})
        for tool, args in turn:
            replies.append(arm.call(tool, args))
    return replies


def same_lines(a: str, b: str) -> bool:
    """Arms index separate copies, and ax lists equal-score callees in index order."""
    return sorted(a.splitlines()) == sorted(b.splitlines())


def check(cond: bool, label: str, failures: list[str]) -> None:
    if not cond:
        failures.append(label)


def level5(binary: str, base: Path, failures: list[str]) -> dict:
    env_off = env_for(base / "home-off", False, "conv-l5")
    off = Arm(binary, make_project(binary, base / "l5-off", env_off), env_off)
    off_replies = play(off)
    off.close()
    check(not any(r["hit"] for r in off_replies), "L5 AX_CONTEXT_CACHE=off still served a hit", failures)

    home_on = base / "home-on"
    env_on = env_for(home_on, True, "conv-l5")
    project = make_project(binary, base / "l5-on", env_on)
    on = Arm(binary, project, env_on)
    on_replies = play(on)
    seen: set[str] = set()
    first_on: dict[str, str] = {}
    expected_hits = 0
    for off_r, on_r in zip(off_replies, on_replies):
        key = json.dumps([off_r["tool"], off_r["args"]], sort_keys=True)
        repeat = key in seen
        seen.add(key)
        expected_hits += repeat
        if on_r["hit"]:
            body = on.expand(on_r["meta"]["contextCacheHit"])
            check(body == first_on.get(key), f"L5 hit for {key} does not expand byte-for-byte to its first reply", failures)
            check(same_lines(body, off_r["text"]), f"L5 hit for {key} does not expand to the cache-off reply", failures)
            check(on_r["meta"]["tokensAvoided"] == on_r["meta"]["originalTokens"] - on_r["meta"]["sentTokens"], f"L5 tokensAvoided arithmetic for {key}", failures)
        else:
            first_on.setdefault(key, on_r["text"])
            check(same_lines(on_r["text"], off_r["text"]), f"L5 miss reply differs from cache-off for {key}", failures)
        check(on_r["hit"] == repeat, f"L5 {key}: hit={on_r['hit']} but repeat={repeat}", failures)
    on.close()

    other = Arm(binary, project, env_for(home_on, True, "conv-other"))
    other_replies = play(other)
    other.close()
    cross = [r for r in other_replies[:2] if r["hit"]]
    check(not cross, "L5 another conversation got a hit on its first calls", failures)

    def total(rs: list[dict]) -> int:
        return sum(r["tokens"] for r in rs)

    graph_off = total(off_replies)
    graph_on = total(on_replies)
    check(graph_on < graph_off, f"L5 cache-on tokens {graph_on} not below cache-off {graph_off}", failures)
    pre_off = sum(c["tokens"] for c in off.calls if c["tool"] == "ax_preflight")
    pre_on = sum(c["tokens"] for c in on.calls if c["tool"] == "ax_preflight")
    return {
        "graph_calls": len(on_replies),
        "repeats": expected_hits,
        "hits": sum(r["hit"] for r in on_replies),
        "graph_tokens_off": graph_off,
        "graph_tokens_on": graph_on,
        "graph_tokens_saved": graph_off - graph_on,
        "graph_saved_pct": round(100 * (graph_off - graph_on) / graph_off, 1),
        "preflight_tokens_off": pre_off,
        "preflight_tokens_on": pre_on,
        "session_tokens_off": graph_off + pre_off,
        "session_tokens_on": graph_on + pre_on,
        "session_saved_pct": round(100 * ((graph_off + pre_off) - (graph_on + pre_on)) / (graph_off + pre_off), 1),
        "other_conversation_first_turn_hits": len(cross),
    }


def level6(binary: str, base: Path, failures: list[str], control_no_edit: bool) -> dict:
    env = env_for(base / "home-l6", True, "conv-l6")
    project = make_project(binary, base / "l6", env)
    arm = Arm(binary, project, env)
    first = arm.call("ax_node", {"name": "apply_discount"})
    repeat = arm.call("ax_node", {"name": "apply_discount"})
    check(repeat["hit"], "L6 repeat before edit should hit", failures)
    pricing = project / "src" / "pricing.rs"
    original = pricing.read_text(encoding="utf-8")
    pricing.write_text(original.replace("value - value * pct / 100", "value - value * pct / 100 + l6_edit_marker()"), encoding="utf-8")
    if control_no_edit:
        pricing.write_text(original, encoding="utf-8")
    arm.call("ax_sync", {})
    after = arm.call("ax_node", {"name": "apply_discount"})
    arm.close()
    check(not after["hit"], "L6 call after edit+sync was served from cache", failures)
    check("l6_edit_marker" in after["text"], "L6 reply after edit does not show the new code", failures)
    calls = [first, repeat, after]
    off_equiv = first["tokens"] * 2 + after["tokens"]
    on_actual = sum(c["tokens"] for c in calls)
    return {
        "calls": len(calls),
        "hits": sum(c["hit"] for c in calls),
        "tokens_without_cache": off_equiv,
        "tokens_with_cache": on_actual,
        "net_saved": off_equiv - on_actual,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out")
    parser.add_argument("--control-no-edit", action="store_true", help="negative control: undo the edit before sync; L6 must then report a hit and fail")
    opts = parser.parse_args()
    binary = os.environ.get("AX_BIN")
    if not binary or not Path(binary).is_file():
        print("AX_BIN must point at the ax binary under test", file=sys.stderr)
        return 2
    base = Path(tempfile.mkdtemp(prefix="ax-reuse-session-"))
    failures: list[str] = []
    try:
        report = {
            "binary": binary,
            "L5": level5(binary, base, failures),
            "L6": level6(binary, base, failures, opts.control_no_edit),
        }
    finally:
        stop_daemons(binary, failures)
        shutil.rmtree(base, ignore_errors=True)
    report["failures"] = failures
    text = json.dumps(report, indent=2)
    print(text)
    if opts.out:
        Path(opts.out).write_text(text + "\n", encoding="utf-8")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
