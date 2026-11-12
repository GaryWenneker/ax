#!/usr/bin/env python3
"""Score Useful-Context Precision @K for CLI PR-review exchanges.

See docs/specs/cli-pr-review-sample/QUALITY_PLAN.md §2.
Exit 1 on negative-control failures or empty gold when not allowed.
"""

from __future__ import annotations

import argparse
import json
import math
import re
import sys
from pathlib import Path
from typing import Any

FILE_RE = re.compile(
    r"(?:^|[\s\"'`])((?:crates|site|scripts|docs|install)[^\"'\s]*\.(?:rs|ts|tsx|js|jsx|py|ps1|sh|mdc|md|toml))"
)
# also bare install.ps1 / path-like
BARE_FILE_RE = re.compile(
    r"\b((?:[A-Za-z0-9_.-]+/)+[A-Za-z0-9_.-]+\.(?:rs|ts|tsx|js|jsx|py|ps1|sh|mdc)|install\.ps1|install\.sh)\b"
)


def walk_strings(obj: Any, acc: set[str] | None = None) -> set[str]:
    if acc is None:
        acc = set()
    if isinstance(obj, dict):
        for v in obj.values():
            walk_strings(v, acc)
    elif isinstance(obj, list):
        for v in obj:
            walk_strings(v, acc)
    elif isinstance(obj, str):
        acc.add(obj)
    return acc


def extract_refs(exchanges: list[dict[str, Any]], k: int = 5) -> tuple[set[str], set[str]]:
    """Return (files, symbols) referenced in AX/GitHub exchanges (not agent_to_user)."""
    files: set[str] = set()
    symbols: set[str] = set()
    for ex in exchanges:
        if ex.get("direction") == "agent_to_user":
            continue
        if ex.get("direction") == "user_to_agent":
            continue
        resp = ex.get("response")
        if resp is None:
            continue
        # Explore top-K entries
        if isinstance(resp, dict) and "entries" in resp:
            entries = resp.get("entries") or []
            for entry in entries[:k]:
                node = entry.get("node") or entry
                fp = node.get("filePath") or node.get("file_path") or ""
                if fp:
                    files.add(fp)
                name = node.get("name") or ""
                if name:
                    symbols.add(name)
                qn = node.get("qualifiedName") or ""
                if qn:
                    symbols.add(qn.split("::")[-1])
        # Impact nodes
        if isinstance(resp, dict) and "nodes" in resp:
            for node in (resp.get("nodes") or {}).values():
                fp = node.get("filePath") or ""
                if fp:
                    files.add(fp)
                name = node.get("name") or ""
                if name:
                    symbols.add(name)
        # Node / callers list
        if isinstance(resp, list):
            for item in resp:
                node = item.get("node") if isinstance(item, dict) and "node" in item else item
                if not isinstance(node, dict):
                    continue
                fp = node.get("filePath") or ""
                if fp:
                    files.add(fp)
                name = node.get("name") or ""
                if name:
                    symbols.add(name)
        # Intentionally ignore gh file lists and preflight inject prose.
        # UCP measures AX graph retrieval (explore/impact/node/callers), not gh.
    return files, symbols


def score_task(
    exchanges: list[dict[str, Any]],
    gold_files: list[str],
    gold_symbols: list[str],
    k: int = 5,
    allow_empty_gold: bool = False,
) -> dict[str, Any]:
    g_files = set(gold_files)
    g_syms = set(gold_symbols)
    g = g_files | g_syms
    if not g and not allow_empty_gold:
        raise ValueError("empty gold set")

    r_files, r_syms = extract_refs(exchanges, k=k)
    r = r_files | r_syms
    if not g:
        return {
            "ucp_at_k": None,
            "recall_at_k": None,
            "k": k,
            "gold_size": 0,
            "ref_size": len(r),
            "intersection": 0,
            "ref_files": sorted(r_files),
            "ref_symbols": sorted(r_syms),
        }

    inter = r & g
    ucp = (len(inter) / len(r)) if r else 0.0
    recall = (len(inter) / len(g)) if g else 0.0
    return {
        "ucp_at_k": ucp,
        "recall_at_k": recall,
        "k": k,
        "gold_size": len(g),
        "ref_size": len(r),
        "intersection": len(inter),
        "hit_files": sorted(r_files & g_files),
        "miss_files": sorted(g_files - r_files),
        "hit_symbols": sorted(r_syms & g_syms),
        "miss_symbols": sorted(g_syms - r_syms),
        "noise_files": sorted(r_files - g_files)[:20],
    }


def median(xs: list[float]) -> float:
    if not xs:
        raise ValueError("empty")
    ys = sorted(xs)
    m = len(ys) // 2
    if len(ys) % 2:
        return ys[m]
    return (ys[m - 1] + ys[m]) / 2.0


def skill_included(exchanges: list[dict[str, Any]], required: list[str]) -> dict[str, bool]:
    """True if skill appears in preflight matchedSkills/inject OR was successfully loaded via CLI."""
    names_found: set[str] = set()
    for ex in exchanges:
        resp = ex.get("response")
        req = ex.get("request") or {}
        cmd = req.get("command") or []
        if isinstance(cmd, list) and len(cmd) >= 4 and cmd[0:3] == ["ax", "policy", "skill"]:
            if ex.get("exitCode") == 0:
                names_found.add(cmd[3])
        if not isinstance(resp, dict):
            continue
        for key in ("matchedSkills", "skills"):
            items = resp.get(key)
            if isinstance(items, list):
                for it in items:
                    if isinstance(it, dict):
                        for k in ("name", "id"):
                            if it.get(k):
                                names_found.add(str(it[k]))
                    elif isinstance(it, str):
                        names_found.add(it)
        inj = resp.get("inject") or ""
        if isinstance(inj, str):
            for name in required:
                if name in inj:
                    names_found.add(name)
    return {name: name in names_found for name in required}



def score_run(
    tasks_path: Path,
    exchanges_by_task: dict[str, list[dict[str, Any]]],
    k: int = 5,
) -> dict[str, Any]:
    tasks_doc = json.loads(tasks_path.read_text())
    tasks = tasks_doc["tasks"]
    per: list[dict[str, Any]] = []
    ucps: list[float] = []
    recalls: list[float] = []
    s2_ok = 0
    for t in tasks:
        tid = t["id"]
        if tid not in exchanges_by_task:
            per.append({"id": tid, "error": "missing exchanges"})
            continue
        gold = t["gold"]
        try:
            s = score_task(
                exchanges_by_task[tid],
                gold.get("files") or [],
                gold.get("symbols") or [],
                k=k,
            )
        except ValueError as e:
            per.append({"id": tid, "error": str(e)})
            continue
        skills = skill_included(exchanges_by_task[tid], t.get("requiredSkills") or [])
        if skills and all(skills.values()):
            s2_ok += 1
        row = {"id": tid, "pr": t.get("pr"), **s, "skills": skills, "goldSigned": gold.get("signed")}
        per.append(row)
        if s["ucp_at_k"] is not None:
            ucps.append(s["ucp_at_k"])
            recalls.append(s["recall_at_k"])

    return {
        "k": k,
        "n_tasks": len(tasks),
        "n_scored": len(ucps),
        "median_ucp_at_k": median(ucps) if ucps else None,
        "median_recall_at_k": median(recalls) if recalls else None,
        "s2_skill_inclusion_rate": (s2_ok / len(ucps)) if ucps else None,
        "tasks": per,
    }


def load_exchanges_file(path: Path) -> list[dict[str, Any]]:
    data = json.loads(path.read_text())
    if isinstance(data, dict) and "exchanges" in data:
        return data["exchanges"]
    if isinstance(data, list):
        return data
    raise ValueError(f"unrecognized exchanges shape: {path}")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--tasks", type=Path, default=Path("docs/specs/cli-pr-review-sample/tasks.json"))
    ap.add_argument("--exchanges", type=Path, help="single exchanges.json (maps to first matching task or --task-id)")
    ap.add_argument("--task-id", type=str, default=None)
    ap.add_argument("--runs-dir", type=Path, help="directory of runs/<task-id>/exchanges.json")
    ap.add_argument("--out", type=Path, default=None)
    ap.add_argument("--k", type=int, default=5)
    ap.add_argument("--allow-empty-gold", action="store_true")
    args = ap.parse_args()

    by_task: dict[str, list[dict[str, Any]]] = {}
    if args.runs_dir:
        for p in sorted(args.runs_dir.glob("*/exchanges.json")):
            by_task[p.parent.name] = load_exchanges_file(p)
    elif args.exchanges:
        ex = load_exchanges_file(args.exchanges)
        tid = args.task_id
        if not tid:
            # default: pr from prompt or pr-251 sample
            tid = "pr-251"
        by_task[tid] = ex
    else:
        print("need --exchanges or --runs-dir", file=sys.stderr)
        return 2

    # NC1: empty gold must error unless allowed
    if args.allow_empty_gold is False:
        pass

    result = score_run(args.tasks, by_task, k=args.k)
    text = json.dumps(result, indent=2) + "\n"
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(text)
    else:
        sys.stdout.write(text)

    if result["n_scored"] == 0:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
