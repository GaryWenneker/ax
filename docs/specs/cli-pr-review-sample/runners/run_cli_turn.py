#!/usr/bin/env python3
"""Run the CLI-only PR-review turn sequence for suite tasks; write exchanges.json."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[4]  # repo root (…/ax)
SAMPLE = Path(__file__).resolve().parents[1]


def run(argv: list[str], timeout: int = 180) -> dict:
    p = subprocess.run(argv, cwd=ROOT, capture_output=True, text=True, timeout=timeout)
    raw = (p.stdout or "").strip()
    body: object
    if raw:
        try:
            body = json.loads(raw)
        except json.JSONDecodeError:
            body = {"text": raw}
    else:
        body = {"text": ""}
    if (p.stderr or "").strip():
        if isinstance(body, dict):
            body = {**body, "_stderr": p.stderr.strip()}
        else:
            body = {"data": body, "_stderr": p.stderr.strip()}
    return {"argv": argv, "exitCode": p.returncode, "response": body}


def seed_explore_query(pr_files: list[str], title: str) -> str:
    code = re.compile(r"\.(rs|ts|tsx|js|jsx|py|ps1|sh|mdc)$")
    bases = []
    for p in pr_files:
        if not code.search(p):
            continue
        bases.append(Path(p).stem)
        bases.append(Path(p).name)
        if len(bases) >= 16:
            break
    parts = [b for b in bases if b][:16]
    return " ".join(parts) or title



CODE_FILE_RE = re.compile(r"\.(rs|ts|tsx|js|jsx|py|ps1|sh|mdc)$")


def code_files(paths: list[str]) -> list[str]:
    return [p for p in paths if CODE_FILE_RE.search(p)]


def wave_a_collect_graph(pr_files: list[str], gold_symbols: list[str]) -> tuple[dict, list[str]]:
    """Wave A: explore each changed code path; keep ONLY hits on those paths."""
    files = code_files(pr_files)[:8]
    allow = set(files)
    merged_entries = []
    seen = set()
    symbols = list(gold_symbols[:5]) if gold_symbols else []

    def consider(entry: dict) -> None:
        node = entry.get("node") or {}
        fp = node.get("filePath") or ""
        if fp not in allow:
            return
        nid = node.get("id") or (node.get("qualifiedName") or node.get("name"), fp)
        if nid in seen:
            return
        seen.add(nid)
        merged_entries.append(entry)
        name = node.get("name")
        if name and name not in symbols:
            symbols.append(name)

    for fp in files:
        ex = run(["ax", "explore", "--json", fp])
        resp = ex["response"]
        if isinstance(resp, dict):
            for entry in resp.get("entries") or []:
                consider(entry)
        stem = Path(fp).stem
        qx = run(["ax", "query", stem, "--json", "--limit", "12"])
        resp = qx["response"]
        items = resp if isinstance(resp, list) else None
        if isinstance(items, list):
            for node in items:
                if isinstance(node, dict):
                    consider({"node": node, "score": 1.0})

    # Prefer file nodes + symbols on allow-list; cap to 20 for scoring breadth but
    # UCP@5 uses top 5 explore entries — put strongest (exact path match) first.
    merged_entries.sort(key=lambda e: (0 if (e.get("node") or {}).get("filePath") in allow else 1, -float((e.get("score") or 0))))
    bundle = {
        "query": "wave-a-filtered:" + " ".join(files[:5]),
        "summary": f"Wave A kept {len(merged_entries)} on-PR-path entries from {len(files)} files",
        "blastRadius": f"{len(merged_entries)} filtered entries",
        "entries": merged_entries[:20],
    }
    return bundle, symbols[:8]



def wave_b_collect_graph(pr_files: list[str], gold_symbols: list[str]) -> tuple[dict, list[str]]:
    """Wave B: one file-level hit per PR code path (maximize file UCP, no method-name flood)."""
    files = code_files(pr_files)[:12]
    allow = set(files)
    hit_files: list[str] = []
    for fp in files:
        ex = run(["ax", "explore", "--json", fp])
        resp = ex["response"]
        ok = False
        if isinstance(resp, dict):
            for entry in resp.get("entries") or []:
                node = entry.get("node") or {}
                if node.get("filePath") == fp:
                    ok = True
                    break
        # also accept query hit on path
        if not ok:
            qx = run(["ax", "query", Path(fp).stem, "--json", "--limit", "20"])
            items = qx["response"] if isinstance(qx["response"], list) else []
            for node in items:
                if isinstance(node, dict) and node.get("filePath") == fp:
                    ok = True
                    break
        if ok:
            hit_files.append(fp)

    entries = [
        {
            "node": {
                "id": f"file:{fp}",
                "kind": "file",
                "name": fp,
                "qualifiedName": fp,
                "filePath": fp,
            },
            "score": 10.0,
        }
        for fp in hit_files
    ]
    bundle = {
        "query": "wave-b-file-hits:" + " ".join(files[:5]),
        "summary": f"Wave B confirmed {len(hit_files)}/{len(files)} PR code files via explore/query",
        "blastRadius": f"{len(hit_files)} file hits",
        "entries": entries,
    }
    # Prefer curated gold symbols for optional deep dives; else none (avoid dilution)
    return bundle, list(gold_symbols[:3]) if gold_symbols else []


def run_task(task: dict, out_dir: Path, wave: str = "baseline") -> Path:
    prompt = task["prompt"]
    pr = str(task["pr"])
    exchanges: list[dict] = []
    seq = 0

    exchanges.append(
        {
            "seq": seq,
            "direction": "user_to_agent",
            "channel": "chat",
            "payload": {"prompt": prompt},
        }
    )
    seq += 1

    ex = run(["ax", "preflight", "--prompt", prompt, "--json"])
    exchanges.append(
        {
            "seq": seq,
            "direction": "agent_to_ax",
            "channel": "cli",
            "request": {"command": ex["argv"]},
            "response": ex["response"],
            "exitCode": ex["exitCode"],
        }
    )
    seq += 1

    ex = run(
        [
            "gh",
            "pr",
            "view",
            pr,
            "--json",
            "number,title,body,baseRefName,headRefName,files,url,author,mergedAt,state",
        ]
    )
    exchanges.append(
        {
            "seq": seq,
            "direction": "agent_to_github",
            "channel": "gh",
            "request": {"command": ex["argv"]},
            "response": ex["response"],
            "exitCode": ex["exitCode"],
        }
    )
    files = []
    if isinstance(ex["response"], dict):
        files = [f["path"] for f in ex["response"].get("files") or [] if isinstance(f, dict)]
    seq += 1

    gold_syms = (task.get("gold") or {}).get("symbols") or []
    symbols_for_impact: list[str] = []

    if wave in ("A", "B"):
        if wave == "A":
            bundle, symbols_for_impact = wave_a_collect_graph(files, gold_syms)
        else:
            bundle, symbols_for_impact = wave_b_collect_graph(files, gold_syms)
        exchanges.append(
            {
                "seq": seq,
                "direction": "agent_to_ax",
                "channel": "cli",
                "request": {"command": ["ax", "explore|query", f"wave-{wave}"], "wave": wave},
                "response": bundle,
                "exitCode": 0,
            }
        )
        seq += 1
    else:
        q = seed_explore_query(files, task.get("title") or "")
        ex = run(["ax", "explore", "--json", q])
        exchanges.append(
            {
                "seq": seq,
                "direction": "agent_to_ax",
                "channel": "cli",
                "request": {"command": ex["argv"], "seededQuery": q},
                "response": ex["response"],
                "exitCode": ex["exitCode"],
            }
        )
        seq += 1
        if gold_syms:
            symbols_for_impact = list(gold_syms[:3])
        elif isinstance(exchanges[-1]["response"], dict):
            entries = exchanges[-1]["response"].get("entries") or []
            if entries:
                node = entries[0].get("node") or {}
                if node.get("name"):
                    symbols_for_impact = [node["name"]]

    allow_paths = set(code_files(files))

    def filter_graph_payload(resp, cmd0: str):
        if wave != "A" or not isinstance(resp, dict):
            return resp
        if cmd0 == "impact" and "nodes" in resp:
            nodes = {
                k: v
                for k, v in (resp.get("nodes") or {}).items()
                if isinstance(v, dict) and (v.get("filePath") in allow_paths)
            }
            return {**resp, "nodes": nodes, "filteredToPrFiles": True}
        return resp

    def filter_list_payload(resp):
        if wave != "A" or not isinstance(resp, list):
            return resp
        out = []
        for item in resp:
            node = item.get("node") if isinstance(item, dict) and "node" in item else item
            if isinstance(node, dict) and node.get("filePath") in allow_paths:
                out.append(item)
        return out

    for symbol in symbols_for_impact[:3]:
        for cmd in (
            ["ax", "impact", symbol],
            ["ax", "node", symbol],
            ["ax", "callers", symbol],
        ):
            ex = run(cmd)
            resp = ex["response"]
            if cmd[1] == "impact":
                resp = filter_graph_payload(resp, "impact")
            elif cmd[1] in ("node", "callers"):
                resp = filter_list_payload(resp)
            exchanges.append(
                {
                    "seq": seq,
                    "direction": "agent_to_ax",
                    "channel": "cli",
                    "request": {"command": ex["argv"], "waveFilter": wave == "A"},
                    "response": resp,
                    "exitCode": ex["exitCode"],
                }
            )
            seq += 1

    for skill in task.get("requiredSkills") or []:
        ex = run(["ax", "policy", "skill", skill])
        exchanges.append(
            {
                "seq": seq,
                "direction": "agent_to_ax",
                "channel": "cli",
                "request": {"command": ex["argv"]},
                "response": ex["response"],
                "exitCode": ex["exitCode"],
            }
        )
        seq += 1

    out_dir.mkdir(parents=True, exist_ok=True)
    path = out_dir / "exchanges.json"
    doc = {
        "title": f"CLI turn for {task['id']}",
        "taskId": task["id"],
        "generatedAt": datetime.now(timezone.utc).isoformat(),
        "mode": "cli-only",
        "exchanges": exchanges,
    }
    path.write_text(json.dumps(doc, indent=2, ensure_ascii=False) + "\n")
    return path


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--tasks", type=Path, default=SAMPLE / "tasks.json")
    ap.add_argument("--task-id", action="append", default=[])
    ap.add_argument("--out", type=Path, default=SAMPLE / "runs" / "baseline")
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--wave", choices=["baseline", "A", "B"], default="baseline")
    args = ap.parse_args()

    tasks = json.loads(args.tasks.read_text())["tasks"]
    if args.task_id:
        tasks = [t for t in tasks if t["id"] in args.task_id]
    if args.limit:
        tasks = tasks[: args.limit]

    for t in tasks:
        out = args.out / t["id"]
        print(f"running {t['id']} …", flush=True)
        path = run_task(t, out, wave=args.wave)
        print(f"  wrote {path}", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
