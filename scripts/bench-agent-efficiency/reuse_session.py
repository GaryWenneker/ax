#!/usr/bin/env python3
"""L5/L6: scripted multi-turn MCP sessions against a real `ax` binary, cache off vs on.

L5  same 5-turn script twice (AX_CONTEXT_CACHE=off, then on). Asserts:
    - every miss reply is identical across arms, and every hit expands to the off-arm reply
    - the on-arm sends fewer response tokens in total
    - every repeated call is a hit
    - a second conversation id gets zero hits for the same calls
L6  explore, edit the cited file, sync, ask again. Asserts the last call is a miss with the
    new code; any hit fails. Reports net savings for the session including the miss.
L5S 8 turns with no hook file: the chat id comes from preflight and the agent passes it back,
    with the last notes hash as known_context. A repeat hits iff its first reply wrote a row
    (replies of 200 tokens or fewer are not stored), at least 4 repeats hit, known turns get one unchanged line,
    a summarized turn (no hash) gets the full block, and a second chat starts cold.
L6S edit+sync mid-session: notes come back stale with a nudge; compact confirms them.

Exits nonzero on any failed assertion or on any harness error. Prints a JSON report.
Usage: AX_BIN=/path/to/ax python3 reuse_session.py [--out report.json] [--control-no-edit]
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import sqlite3
import subprocess
import sys
import tempfile
from contextlib import closing
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


def env_for(home: Path, cache_on: bool, conversation: str | None) -> dict[str, str]:
    """`conversation=None` writes no hook file: the chat id then comes from preflight alone."""
    (home / ".ax").mkdir(parents=True, exist_ok=True)
    marker = home / ".ax" / "active-cursor-session"
    if conversation is None:
        marker.unlink(missing_ok=True)
    else:
        marker.write_text(conversation + "\n", encoding="utf-8")
    env = os.environ.copy()
    env["HOME"] = str(home)
    env["AX_USAGE_DB"] = str(home / "usage.db")
    env["AX_CONTEXT_CACHE"] = "on" if cache_on else "off"
    env["NO_COLOR"] = "1"
    return env


def run(cmd: list[str], cwd: Path, env: dict[str, str]) -> None:
    done = subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True)
    if done.returncode != 0:
        raise SystemExit(f"command failed ({done.returncode}): {' '.join(cmd)}\n{done.stdout}\n{done.stderr}")


PROJECTS: list[Path] = []


def make_project(binary: str, root: Path, env: dict[str, str]) -> Path:
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
    [("ax_callers", {"symbol": "compute_total"}), ("ax_node", {"name": "apply_discount"})],
    [("ax_node", {"name": "compute_total"}), ("ax_explore", {"query": "how does compute_total work"})],
    [("ax_node", {"name": "apply_discount"}), ("ax_callees", {"symbol": "compute_total"})],
    [("ax_explore", {"query": "how does compute_total work"}), ("ax_node", {"name": "compute_total"})],
]


class Arm:
    def __init__(self, binary: str, project: Path, env: dict[str, str]) -> None:
        self.session = McpSession(project, binary, env=env)
        self.db = Path(env["AX_USAGE_DB"])
        self.calls: list[dict] = []

    def stored_rows(self, conversation: str) -> int:
        """Reuse rows for one chat; small replies are deliberately not stored, so a repeat of one misses."""
        if not self.db.exists():
            return 0
        with closing(sqlite3.connect(f"file:{self.db}?mode=ro", uri=True)) as db:
            scope = conversation + "\x1f"
            sql = "SELECT COUNT(*) FROM mcp_reuse_cache WHERE substr(conversation, 1, length(?1)) = ?1"
            return int(db.execute(sql, (scope,)).fetchone()[0])

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


def session_blocks(text: str) -> list[str]:
    """`<ax_session_context>` blocks; the cache instruction line also names the tag, so match the newline."""
    out, rest = [], text
    while (start := rest.find("<ax_session_context>\n")) >= 0:
        end = rest.find("</ax_session_context>", start)
        if end < 0:
            raise SystemExit("unterminated <ax_session_context> block")
        end += len("</ax_session_context>")
        out.append(rest[start:end])
        rest = rest[end:]
    return out


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
    blocks = sum(tokens(b) for c in on.calls if c["tool"] == "ax_preflight" for b in session_blocks(c["text"]))
    check(blocks > 0, "L5 preflight never listed known context", failures)
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
        "session_context_block_tokens": blocks,
        "reuse_net_saved": graph_off - graph_on - blocks,
        "reuse_net_saved_pct": round(100 * (graph_off - graph_on - blocks) / (graph_off + pre_off), 1),
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


CHAT = re.compile(r"<ax_chat session=([A-Za-z0-9_-]+) graph=([0-9a-f]{16})>")
WORKING = re.compile(r"<ax_working_context hash=([0-9a-f]{16})( unchanged/>| stale=(true|false)>)")
NUDGE = "<ax_session_nudge>"
NOTES = {
    "action": "add",
    "objective": "Understand compute_total",
    "facts": ["compute_total sums lines, applies the discount, then adds tax"],
    "files": ["src/orders.rs", "src/pricing.rs"],
    "symbols": ["compute_total", "apply_discount", "tax_for"],
}
MORE_NOTES = {"action": "add", "decisions": ["Keep apply_discount's signature"]}

# (graph calls, notes to write, pass known_context?) per turn. Turn 6 plays a summarized chat: no hash.
SESSION_SCRIPT = [
    ([("ax_explore", {"query": "how does compute_total work"}), ("ax_node", {"name": "compute_total"})], NOTES, True),
    ([("ax_callers", {"symbol": "compute_total"}), ("ax_node", {"name": "apply_discount"})], None, True),
    ([("ax_node", {"name": "compute_total"}), ("ax_explore", {"query": "how does compute_total work"})], None, True),
    ([("ax_node", {"name": "apply_discount"})], MORE_NOTES, True),
    ([("ax_callees", {"symbol": "compute_total"})], None, True),
    ([], None, False),
    ([("ax_explore", {"query": "how does compute_total work"}), ("ax_node", {"name": "compute_total"})], None, True),
    ([("ax_callees", {"symbol": "compute_total"})], None, True),
]


def chat_of(text: str) -> str:
    found = CHAT.search(text)
    if not found:
        raise SystemExit(f"preflight printed no <ax_chat session=… graph=…>:\n{text[:2000]}")
    return found.group(1)


def working_of(text: str) -> tuple[str, str] | None:
    """(hash, "unchanged" | "stale" | "fresh") of the working-context block, or None."""
    found = WORKING.search(text)
    if not found:
        return None
    kind = "unchanged" if found.group(2).startswith(" unchanged") else ("stale" if found.group(3) == "true" else "fresh")
    return found.group(1), kind


def play_session(arm: Arm, send_known: bool) -> dict:
    """One chat that carries its session id; returns per-turn preflight tokens and observations."""
    session, known, turns = None, None, []
    for graph, notes, pass_known in SESSION_SCRIPT:
        args = {"prompt": "continue the task"}
        if session:
            args["session"] = session
        if send_known and pass_known and known:
            args["known_context"] = known
        pre = arm.call("ax_preflight", args)
        printed = chat_of(pre["text"])
        session = session or printed
        block = working_of(pre["text"])
        if block:
            known = block[0]
        replies = []
        for tool, a in graph:
            before = arm.stored_rows(session)
            reply = arm.call(tool, {**a, "session": session})
            reply["stored"] = arm.stored_rows(session) > before
            replies.append(reply)
        if notes:
            wrote = arm.call("ax_session", {**notes, "session": session})
            known = (working_of(wrote["text"]) or (None,))[0]
        turns.append({"printed": printed, "block": block, "nudge": NUDGE in pre["text"], "preflight_tokens": pre["tokens"], "replies": replies})
    return {"session": session, "turns": turns}


def level5_session(binary: str, base: Path, failures: list[str]) -> dict:
    env = env_for(base / "home-l5s", True, None)
    project = make_project(binary, base / "l5s", env)
    arm = Arm(binary, project, env)
    sent_once = play_session(arm, send_known=True)
    arm.close()
    other = Arm(binary, project, env)
    resent = play_session(other, send_known=False)
    other.close()

    a, b = sent_once["turns"], resent["turns"]
    check(sent_once["session"] != resent["session"], "L5S two chats on one daemon got the same session id", failures)
    check(all(t["printed"] == sent_once["session"] for t in a), "L5S preflight did not echo the chat's session id", failures)
    seen: set[str] = set()
    for n, turn in enumerate(a, 1):
        for r in turn["replies"]:
            key = json.dumps([r["tool"], {k: v for k, v in r["args"].items() if k != "session"}], sort_keys=True)
            check(r["hit"] == (key in seen), f"L5S turn {n} {key}: hit={r['hit']}", failures)
            check(not (r["hit"] and r["stored"]), f"L5S turn {n} {key}: a hit wrote a new row", failures)
            if r["stored"]:
                seen.add(key)
        if n == 1:
            check(turn["block"] is None, "L5S turn 1 showed notes before any were written", failures)
        elif SESSION_SCRIPT[n - 1][2]:
            check(turn["block"] is not None and turn["block"][1] == "unchanged", f"L5S turn {n}: expected one unchanged line, got {turn['block']}", failures)
        else:
            check(turn["block"] is not None and turn["block"][1] == "fresh", f"L5S turn {n} (no hash): expected the full block, got {turn['block']}", failures)
        check(not turn["nudge"], f"L5S turn {n} nudged although notes were written within 5 turns", failures)
    hits = sum(r["hit"] for t in a for r in t["replies"])
    check(hits >= 4, f"L5S only {hits} repeats hit (expected at least 4)", failures)
    first_other = b[0]["replies"]
    check(not any(r["hit"] for r in first_other), "L5S the second chat hit the first chat's answers", failures)
    check(b[0]["block"] is None, "L5S the second chat saw the first chat's notes", failures)
    check(all(t["block"] is None or t["block"][1] != "unchanged" for t in b), "L5S an unchanged line without known_context", failures)

    per_turn_known = [t["preflight_tokens"] for t in a]
    per_turn_full = [t["preflight_tokens"] for t in b]
    saved = sum(per_turn_full) - sum(per_turn_known)
    check(saved > 0, f"L5S known_context saved no preflight tokens ({saved})", failures)
    graph_tokens = sum(r["tokens"] for t in a for r in t["replies"])
    return {
        "turns": len(a),
        "graph_calls": sum(len(t["replies"]) for t in a),
        "hits": sum(r["hit"] for t in a for r in t["replies"]),
        "graph_tokens": graph_tokens,
        "not_stored": sorted({r["tool"] for t in a for r in t["replies"] if not r["hit"] and not r["stored"]}),
        "preflight_tokens_per_turn_with_known_context": per_turn_known,
        "preflight_tokens_per_turn_without": per_turn_full,
        "known_context_saved_tokens": saved,
        "second_chat_first_turn_hits": sum(r["hit"] for r in first_other),
    }


def level6_session(binary: str, base: Path, failures: list[str]) -> dict:
    env = env_for(base / "home-l6s", True, None)
    project = make_project(binary, base / "l6s", env)
    arm = Arm(binary, project, env)
    pre = arm.call("ax_preflight", {"prompt": "start"})
    session = chat_of(pre["text"])
    graph_before = CHAT.search(pre["text"]).group(2)
    arm.call("ax_node", {"name": "apply_discount", "session": session})
    wrote = arm.call("ax_session", {**NOTES, "session": session})
    known = working_of(wrote["text"])[0]
    pricing = project / "src" / "pricing.rs"
    original = pricing.read_text(encoding="utf-8")
    pricing.write_text(original.replace("value - value * pct / 100", "value - value * pct / 100 + l6s_marker()"), encoding="utf-8")
    arm.call("ax_sync", {})
    stale = arm.call("ax_preflight", {"prompt": "after edit", "session": session, "known_context": known})
    block = working_of(stale["text"])
    check(block == (known, "stale"), f"L6S expected stale notes with the same hash, got {block}", failures)
    check(NUDGE in stale["text"], "L6S stale notes got no nudge", failures)
    check(CHAT.search(stale["text"]).group(2) != graph_before, "L6S graph version did not move after sync", failures)
    after = arm.call("ax_node", {"name": "apply_discount", "session": session})
    check(not after["hit"] and "l6s_marker" in after["text"], "L6S graph call after edit+sync was not a fresh miss", failures)
    compact = {**NOTES, "action": "compact", "decisions": [], "open_questions": [], "session": session}
    confirmed = arm.call("ax_session", compact)
    check(working_of(confirmed["text"]) == (known, "fresh"), "L6S compact did not confirm the same notes", failures)
    quiet = arm.call("ax_preflight", {"prompt": "confirmed", "session": session, "known_context": known})
    check(working_of(quiet["text"]) == (known, "unchanged"), "L6S confirmed notes were sent again", failures)
    check(NUDGE not in quiet["text"], "L6S nudge stayed after compact", failures)
    arm.close()
    return {"stale_block_tokens": stale["tokens"], "confirmed_preflight_tokens": quiet["tokens"]}


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
            "L5S": level5_session(binary, base, failures),
            "L6S": level6_session(binary, base, failures),
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
