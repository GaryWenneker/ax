#!/usr/bin/env python3
"""Label an independent code-discovery benchmark from clean crate files.

The files are the ones whose bytes match snapshot 85fa176a. This script does
not rank candidates and does not read a score. Graph links come from the
code-graph tables, not from the label calls.
"""

from __future__ import annotations

import collections
import json
import re
import sqlite3
import subprocess
import sys
from pathlib import Path

SNAPSHOT = "85fa176a9b4b6ce5e33687bee268767d87731c60"
ROOT = Path(__file__).resolve().parents[2]
SOURCE_EXT = {".ts", ".tsx", ".js", ".mjs", ".rs", ".go", ".java", ".kt", ".kts"}
SKIP_PREFIX = (
    "crates/ax-memory/",
    "crates/ax-neural/",
    "crates/ax-web/web-ui/dist/",
)
DIAGNOSIS_FILES = {
    "crates/ax-core/src/lib.rs",
    "crates/ax-desktop-client/src/api/sse.rs",
    "crates/ax-global-db/src/access.rs",
    "crates/ax-global-db/src/policy.rs",
    "crates/ax-graph/src/traversal.rs",
    "crates/ax-mcp/src/daemon.rs",
    "crates/ax-mcp/src/remote/export.rs",
    "crates/ax-pi/src/integration.rs",
    "crates/ax-policy/src/config.rs",
    "crates/ax-policy/src/paths.rs",
    "crates/ax-policy/src/provenance.rs",
    "crates/ax-quality/src/container.rs",
    "crates/ax-usage/src/cursor_state.rs",
}
FROZEN_RENAMES = {
    ("crates/ax-core/src/project_config.rs", "load_from_disk", "load_merged"),
    ("crates/ax-installer/src/targets.rs", "has_agent_data_dir", "app_present"),
    ("crates/ax-policy/src/index.rs", "prune_rules", "prune_rules_hybrid"),
    ("crates/ax-policy/src/index.rs", "prune_skills", "prune_skills_hybrid"),
    ("crates/ax-web/src/mcp_trace.rs", "active_log_path", "hub_project"),
}
NEGATIVE_QUERIES = [
    "The orchard sells pears in late September",
    "Knit a wool scarf for cold mornings",
    "The ferry schedule changes on Sundays",
    "Add cinnamon to the porridge",
    "A marble statue stands in the square",
    "The library closes early on Monday",
    "Pack a picnic for the river bank",
    "The violin lesson starts at four",
    "The lighthouse blinks twice at dusk",
    "Stir the soup before serving",
    "The red bicycle leans on the fence",
    "Hang the laundry in the courtyard",
]
FN_RE = re.compile(
    r"^(?:export\s+)?(?:default\s+)?(?:async\s+)?function\s+([A-Za-z_][A-Za-z0-9_]*)"
    r"|^(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(?:const\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)"
    r"|^func\s+(?:\([^)]*\)\s*)?([A-Za-z_][A-Za-z0-9_]*)"
)
CONST_RE = re.compile(
    r"^(?:export\s+)?const\s+([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(?:async\s*)?(?:function\b|\([^)]*\)\s*=>)"
)
JAVA_RE = re.compile(
    r"^(?:public|private|protected)\s+(?:static\s+)?(?:final\s+)?(?:synchronized\s+)?"
    r"(?:[\w.<>,\[\]?]+\s+)+([A-Za-z_][A-Za-z0-9_]*)\s*\("
)
DIFF_FN_RE = re.compile(
    r"^([+-])(?:export\s+)?(?:default\s+)?(?:async\s+)?function\s+([A-Za-z_][A-Za-z0-9_]*)"
    r"|^([+-])(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)"
    r"|^([+-])func\s+(?:\([^)]*\)\s*)?([A-Za-z_][A-Za-z0-9_]*)"
    r"|^([+-])(?:public|private|protected)\s+(?:static\s+)?(?:final\s+)?"
    r"(?:[\w.<>,\[\]?]+\s+)+([A-Za-z_][A-Za-z0-9_]*)\s*\("
)
CALL_RE = re.compile(r"(?<![A-Za-z0-9_.])([A-Za-z_][A-Za-z0-9_]{4,})\s*\(")
KEYWORDS = {
    "async",
    "function",
    "export",
    "default",
    "pub",
    "fn",
    "func",
    "public",
    "private",
    "protected",
    "static",
    "final",
    "const",
    "unsafe",
}


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True, errors="replace")


def symbol_key(name: str) -> str:
    return "".join(ch.lower() for ch in name if ch.isascii() and ch.isalnum())


def function_name(line: str) -> str | None:
    match = FN_RE.match(line.strip()) or CONST_RE.match(line.strip()) or JAVA_RE.match(line.strip())
    if not match:
        return None
    return next(group for group in match.groups() if group)


def excluded(path: str) -> bool:
    if path in DIAGNOSIS_FILES or any(path.startswith(prefix) for prefix in SKIP_PREFIX):
        return True
    if path in {row[0] for row in FROZEN_RENAMES}:
        return True
    if any(part in f"/{path}" for part in ("/dist/", "/node_modules/")):
        return True
    return path.endswith(".d.ts") or path.endswith(".min.js")


def clean_paths() -> list[str]:
    changed = set(git("diff", "--name-only", SNAPSHOT).splitlines())
    listed = git("ls-tree", "-r", "--name-only", SNAPSHOT, "crates").splitlines()
    typed = {
        path[: -len(Path(path).suffix)]
        for path in listed
        if Path(path).suffix in {".ts", ".tsx"}
    }
    kept = []
    for path in listed:
        if path in changed or excluded(path) or Path(path).suffix not in SOURCE_EXT:
            continue
        if path.endswith(".js") and path[:-3] in typed:
            continue
        kept.append(path)
    return sorted(kept)


def functions_in(path: str, text: str) -> list[dict]:
    if len(text) > 400_000:
        return []
    lines = text.splitlines()
    found = []
    for index, line in enumerate(lines):
        name = function_name(line)
        if not name or len(name) < 2:
            continue
        body = [line]
        depth = line.count("{") - line.count("}")
        started = "{" in line
        if not (started and depth <= 0):
            for cursor in range(index + 1, len(lines)):
                if function_name(lines[cursor]) and depth <= 0:
                    break
                body.append(lines[cursor])
                depth += lines[cursor].count("{") - lines[cursor].count("}")
                if "{" in lines[cursor]:
                    started = True
                if started and depth <= 0:
                    break
                if len(body) > 200:
                    break
        found.append(
            {
                "id": f"{path}:{index + 1}",
                "name": name,
                "file": path,
                "line": index + 1,
                "signature": line.strip()[:200],
                "body": "\n".join(body),
            }
        )
    return found


def load_functions(paths: list[str]) -> list[dict]:
    items = []
    for path in paths:
        text = (ROOT / path).read_text(encoding="utf-8", errors="replace")
        items.extend(functions_in(path, text))
    items.sort(key=lambda item: item["id"])
    return items


def indexes(items: list[dict]) -> tuple[dict[str, dict], dict[tuple[str, str], dict], collections.Counter]:
    counts = collections.Counter(item["name"] for item in items)
    unique = {item["name"]: item for item in items if counts[item["name"]] == 1 and len(item["name"]) >= 5}
    per_file: dict[str, list[dict]] = collections.defaultdict(list)
    for item in items:
        per_file[item["file"]].append(item)
    local: dict[tuple[str, str], dict] = {}
    for file, group in per_file.items():
        local_counts = collections.Counter(item["name"] for item in group)
        for item in group:
            if local_counts[item["name"]] == 1 and len(item["name"]) >= 5:
                local[(file, item["name"])] = item
    return unique, local, counts


def call_targets(item: dict, unique: dict[str, dict], local: dict[tuple[str, str], dict]) -> list[dict]:
    seen = set()
    found = []
    for match in CALL_RE.finditer(item["body"]):
        name = match.group(1)
        if len(name) < 5 or name == item["name"] or name in seen:
            continue
        seen.add(name)
        target = unique.get(name)
        if target is not None and target["file"] != item["file"]:
            found.append(target)
            continue
        nearby = local.get((item["file"], name))
        if nearby is not None and nearby["id"] != item["id"]:
            found.append(nearby)
    return found


def take_capped(rows: list[dict], limit: int, per_file: int) -> list[dict]:
    kept = []
    used: collections.Counter = collections.Counter()
    for row in rows:
        if used[row["file"]] >= per_file:
            continue
        used[row["file"]] += 1
        kept.append(row)
        if len(kept) >= limit:
            break
    return kept


def diff_names(line: str) -> tuple[str, str] | None:
    match = DIFF_FN_RE.match(line.rstrip("\n"))
    if not match:
        return None
    groups = match.groups()
    sign = next(group for group in groups[0::2] if group)
    name = next(group for group in groups[1::2] if group)
    return sign, name


def history(items: list[dict], paths: set[str]) -> tuple[list[dict], list[dict]]:
    by_file_name = {(item["file"], item["name"]): item for item in items}
    names_now = {item["name"] for item in items}
    command = [
        "git",
        "log",
        "-p",
        "-U0",
        "--pretty=format:COMMIT %H %s",
        "-n",
        "400",
        SNAPSHOT,
        "--",
        "crates",
    ]
    process = subprocess.Popen(command, cwd=ROOT, stdout=subprocess.PIPE, text=True, errors="replace")
    commit = ""
    subject = ""
    path = ""
    removed: list[tuple[str, str]] = []
    added: list[tuple[str, str]] = []
    commit_added: list[tuple[str, str]] = []
    renames: list[dict] = []
    fixes: list[dict] = []

    def flush_hunk() -> None:
        nonlocal removed, added
        if path in paths and len(removed) == 1 and len(added) == 1 and removed[0][0] != added[0][0]:
            old, old_line = removed[0]
            new, new_line = added[0]
            pair = (path, old, new)
            same = re.sub(r"\b" + re.escape(old) + r"\b", "NAME", old_line, count=1) == re.sub(
                r"\b" + re.escape(new) + r"\b", "NAME", new_line, count=1
            )
            tokens = re.findall(r"[A-Za-z_][A-Za-z0-9_]*", old_line)
            item = by_file_name.get((path, new))
            if (
                same
                and pair not in FROZEN_RENAMES
                and len(old) >= 5
                and len(new) >= 5
                and old not in names_now
                and item is not None
                and any(token not in KEYWORDS and token not in {old, new} for token in tokens)
            ):
                renames.append({"old": old, "id": item["id"], "file": path, "query": old})
        removed, added = [], []

    assert process.stdout is not None
    for line in process.stdout:
        if line.startswith("COMMIT "):
            flush_hunk()
            if len(commit_added) == 1 and re.search(r"(^|[^A-Za-z])(fix|bug)([^A-Za-z]|$)", subject, re.I):
                file, name = commit_added[0]
                item = by_file_name.get((file, name))
                named = re.search(r"\b" + re.escape(name) + r"\b", subject) is not None
                if item is not None and not named:
                    fixes.append({"query": subject.strip(), "id": item["id"], "file": file, "commit": commit})
            commit_added = []
            rest = line[7:].strip()
            commit, _, subject = rest.partition(" ")
            path = ""
            continue
        if line.startswith("diff --git "):
            flush_hunk()
            parts = line.split()
            path = parts[-1][2:] if parts[-1].startswith("b/") else parts[-1]
            continue
        if line.startswith("@@"):
            flush_hunk()
            continue
        parsed = diff_names(line)
        if parsed is None or line.startswith(("---", "+++")):
            continue
        sign, name = parsed
        if sign == "-":
            removed.append((name, line[1:].strip()))
        else:
            added.append((name, line[1:].strip()))
            if len(name) >= 5 and path in paths:
                commit_added.append((path, name))
        if len(renames) >= 8 and len(fixes) >= 12:
            break
    flush_hunk()
    process.kill()
    process.wait()
    return dedup(renames)[:8], dedup(fixes)[:12]


def dedup(rows: list[dict]) -> list[dict]:
    kept = []
    seen = set()
    for row in rows:
        if row["id"] in seen:
            continue
        seen.add(row["id"])
        kept.append(row)
    return kept


def similar_tasks(items: list[dict], calls: set[tuple[str, str]]) -> list[dict]:
    counts = collections.Counter(item["name"] for item in items)
    unique = [item for item in items if counts[item["name"]] == 1 and len(symbol_key(item["name"])) >= 5]
    buckets: dict[str, list[dict]] = collections.defaultdict(list)
    for item in unique:
        buckets[symbol_key(item["name"])[:4]].append(item)
    tasks = []
    for bucket in buckets.values():
        ordered = sorted(bucket, key=lambda item: item["id"])
        for index, left in enumerate(ordered):
            left_key = symbol_key(left["name"])
            for right in ordered[index + 1 :]:
                right_key = symbol_key(right["name"])
                if left_key == right_key:
                    continue
                shared = 0
                for one, two in zip(left_key, right_key):
                    if one != two:
                        break
                    shared += 1
                if shared < 4 or shared >= min(len(left_key), len(right_key)):
                    continue
                if (left["id"], right["id"]) in calls or (right["id"], left["id"]) in calls:
                    continue
                tasks.append(
                    {
                        "id": f"similar:{left['id']}",
                        "kind": "similar_unrelated",
                        "query": left["name"],
                        "relevant": [left["id"]],
                        "file": left["file"],
                        "endpoints": [left["id"], right["id"]],
                    }
                )
                break
    tasks.sort(key=lambda task: task["id"])
    return tasks[:15]


def graph_links(corpus: dict[str, dict]) -> tuple[list[dict], dict[str, int]]:
    database = ROOT / ".ax" / "ax.db"
    connection = sqlite3.connect(f"file:{database}?mode=ro", uri=True)
    nodes = connection.execute(
        "SELECT id, name, file_path, start_line FROM nodes WHERE kind IN ('function', 'method')"
    ).fetchall()
    by_exact: dict[tuple[str, str, int], str] = {}
    ambiguous = set()
    names: dict[str, list[tuple[str, str]]] = collections.defaultdict(list)
    node_file = {}
    node_name = {}
    for node_id, name, file_path, start_line in nodes:
        key = (file_path, name, start_line)
        if key in by_exact:
            ambiguous.add(key)
        by_exact[key] = node_id
        names[name].append((node_id, file_path))
        node_file[node_id] = file_path
        node_name[node_id] = name
    for key in ambiguous:
        by_exact.pop(key, None)
    corpus_of = {}
    matched = 0
    for item in corpus.values():
        node_id = by_exact.get((item["file"], item["name"], item["line"]))
        if node_id is None:
            continue
        corpus_of[node_id] = item["id"]
        matched += 1
    unique_name = {
        name: rows[0]
        for name, rows in names.items()
        if len(rows) == 1
    }
    chosen: dict[tuple[str, str], str] = {}

    def add(source_node: str, target_node: str, proof: str) -> None:
        source = corpus_of.get(source_node)
        target = corpus_of.get(target_node)
        if source is None or target is None or source == target:
            return
        current = chosen.get((source, target))
        if current == "extracted":
            return
        if proof == "extracted" or current is None:
            chosen[(source, target)] = proof

    for source, target, _kind, confidence in connection.execute(
        "SELECT source, target, kind, confidence FROM edges WHERE kind = 'calls'"
    ):
        if confidence == "extracted":
            add(source, target, "extracted")
            continue
        if confidence != "inferred":
            continue
        target_rows = names.get(node_name.get(target, ""), [])
        file_source = node_file.get(source, "")
        file_target = node_file.get(target, "")
        if len(target_rows) == 1 and file_source and file_target and file_source != file_target:
            add(source, target, "unique-cross-file")
    for from_node, reference_name, file_path in connection.execute(
        """
        SELECT from_node_id, reference_name, file_path
        FROM unresolved_refs
        WHERE reference_kind = 'calls'
          AND instr(reference_name, '.') = 0
          AND instr(reference_name, '(') = 0
        """
    ):
        hit = unique_name.get(reference_name)
        if hit is None:
            continue
        target_node, target_file = hit
        if not file_path or not target_file or file_path == target_file:
            continue
        add(from_node, target_node, "unique-cross-file")
    links = [
        {"source": source, "target": target, "kind": "calls", "proof": proof}
        for (source, target), proof in sorted(chosen.items())
    ]
    return links, {"candidatesMatched": matched, "candidates": len(corpus), "links": len(links)}


def build() -> dict:
    paths = clean_paths()
    items = load_functions(paths)
    unique, local, counts = indexes(items)
    path_set = set(paths)
    outgoing = {item["id"]: call_targets(item, unique, local) for item in items}
    call_pairs = {(item["id"], target["id"]) for item in items for target in outgoing[item["id"]]}
    cross_rows = []
    indirect_rows = []
    for item in items:
        if counts[item["name"]] != 1 or len(item["name"]) < 5:
            continue
        direct = [target for target in outgoing[item["id"]] if target["file"] != item["file"] and target["name"] in unique]
        if direct:
            cross_rows.append(
                {
                    "id": f"cross:{item['id']}",
                    "kind": "cross_file_call",
                    "query": item["name"],
                    "relevant": sorted({target["id"] for target in direct}),
                    "file": item["file"],
                    "endpoints": [item["id"], *[target["id"] for target in direct]],
                }
            )
        further = []
        direct_ids = {target["id"] for target in direct}
        for mid in direct:
            for nxt in outgoing[mid["id"]]:
                if nxt["id"] in direct_ids or nxt["id"] == item["id"] or nxt["file"] == item["file"]:
                    continue
                further.append(nxt["id"])
        if further:
            indirect_rows.append(
                {
                    "id": f"indirect:{item['id']}",
                    "kind": "indirect_dependency",
                    "query": item["name"],
                    "relevant": sorted(set(further)),
                    "file": item["file"],
                    "endpoints": [item["id"], *further],
                }
            )
    renames, fixes = history(items, path_set)
    tasks = take_capped(cross_rows, 20, 2)
    tasks.extend(take_capped(indirect_rows, 15, 2))
    for row in fixes:
        tasks.append(
            {
                "id": f"bug:{row['commit']}:{row['id']}",
                "kind": "bug_localization",
                "query": row["query"],
                "relevant": [row["id"]],
                "file": row["file"],
                "endpoints": [row["id"]],
            }
        )
    for row in renames:
        tasks.append(
            {
                "id": f"rename:{row['id']}:{row['old']}",
                "kind": "symbol_rename",
                "query": row["query"],
                "relevant": [row["id"]],
                "file": row["file"],
                "endpoints": [row["id"]],
            }
        )
    tasks.extend(similar_tasks(items, call_pairs))
    by_id = {item["id"]: item for item in items}
    endpoints = []
    for task in tasks:
        for endpoint in task["endpoints"]:
            if endpoint not in endpoints:
                endpoints.append(endpoint)
    files = {by_id[endpoint]["file"] for endpoint in endpoints if endpoint in by_id}
    corpus_ids = list(endpoints)
    for item in items:
        if item["file"] in files and item["id"] not in corpus_ids:
            corpus_ids.append(item["id"])
        if len(corpus_ids) >= 800:
            break
    corpus = {item_id: by_id[item_id] for item_id in corpus_ids if item_id in by_id}
    known = {symbol_key(item["name"]) for item in corpus.values() if symbol_key(item["name"])}
    negatives = []
    for query in NEGATIVE_QUERIES:
        tokens = re.split(r"[^A-Za-z0-9_]+", query)
        if any(symbol_key(token) in known and len(symbol_key(token)) >= 4 for token in tokens):
            continue
        negatives.append(query)
        if len(negatives) == 8:
            break
    for query in negatives:
        tasks.append(
            {
                "id": f"negative:{symbol_key(query)[:24]}",
                "kind": "negative",
                "query": query,
                "relevant": [],
                "file": "",
                "endpoints": [],
            }
        )
    links, coverage = graph_links(corpus)
    public_tasks = [
        {"id": task["id"], "kind": task["kind"], "query": task["query"], "relevant": task["relevant"]}
        for task in tasks
    ]
    return {
        "snapshot": SNAPSHOT,
        "protocol": "graph-discovery-independent-1",
        "labelRule": (
            "Labels come from clean crate files at the snapshot and from the "
            "first 400 commits. Links come from extracted call edges, unique "
            "inferred cross-file call edges, and unresolved unique bare calls. "
            "No score was consulted."
        ),
        "coverage": coverage,
        "candidates": [
            {
                "id": item["id"],
                "name": item["name"],
                "file": item["file"],
                "signature": item["signature"],
            }
            for item in (corpus[item_id] for item_id in corpus_ids if item_id in corpus)
        ],
        "links": links,
        "tasks": public_tasks,
    }


def main() -> None:
    payload = build()
    counts = collections.Counter(task["kind"] for task in payload["tasks"])
    destination = Path(__file__).with_name("graph-discovery-independent.json")
    destination.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    print(f"candidates {len(payload['candidates'])}")
    print(f"links {len(payload['links'])}")
    print(f"coverage {payload['coverage']}")
    print(f"tasks {len(payload['tasks'])}")
    for kind, count in sorted(counts.items()):
        print(f"  {kind} {count}")


if __name__ == "__main__":
    sys.exit(main())
