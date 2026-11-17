#!/usr/bin/env python3
"""Freeze a code-retrieval benchmark from one git snapshot.

The snapshot is the source trees that were not used to build the doc-comment
diagnosis or code_aware::select. Labels come from the snapshot text and from
git hunks. This script does not rank candidates and does not read a score.
"""

from __future__ import annotations

import collections
import json
import re
import subprocess
import sys
from pathlib import Path

SNAPSHOT = "85fa176a9b4b6ce5e33687bee268767d87731c60"
ROOTS = [
    "site",
    "src",
    "benchmarks",
    "examples",
    "test-smoke",
    "test-smoke-rust",
    "test-smoke-go",
    "test-smoke-workspace",
    "telemetry-worker",
    "ide",
]
SOURCE_EXT = {".ts", ".tsx", ".js", ".mjs", ".rs", ".go", ".java", ".kt", ".kts"}
SKIP_PARTS = ("/dist/", "/node_modules/", "/web-ui/dist/")
NEGATIVE_QUERIES = [
    "What is the weather in Utrecht tomorrow morning",
    "Bake a sponge cake with butter and sugar",
    "The train to Haarlem leaves at noon",
    "Who painted the ceiling of the chapel",
    "A quiet river runs through the valley",
    "Please book a table for two on Friday",
    "The recipe uses oats and honey only",
    "How tall is the tower by the harbour",
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


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], text=True, errors="replace")


def symbol_key(name: str) -> str:
    return "".join(ch.lower() for ch in name if ch.isascii() and ch.isalnum())


def strip_doc(text: str) -> str:
    cleaned = []
    for line in text.splitlines():
        line = line.strip()
        if line.startswith("///"):
            line = line[3:]
        elif line.startswith("//"):
            line = line[2:]
        elif line.startswith("/*"):
            line = line[2:]
        elif line.startswith("*"):
            line = line[1:]
        line = line.replace("*/", "")
        cleaned.append(line.strip())
    return re.sub(r"\s+", " ", " ".join(cleaned)).strip()


def function_name(line: str) -> str | None:
    raw = line.strip()
    match = FN_RE.match(raw) or CONST_RE.match(raw) or JAVA_RE.match(raw)
    if not match:
        return None
    return next(group for group in match.groups() if group)


def functions_in(path: str, text: str) -> list[dict]:
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
        doc_lines: list[str] = []
        cursor = index - 1
        while cursor >= 0:
            stripped = lines[cursor].strip()
            if stripped == "" and not doc_lines:
                cursor -= 1
                continue
            if stripped.startswith(("///", "//", "/*", "*")) or stripped.endswith("*/"):
                doc_lines.append(stripped)
                cursor -= 1
                continue
            break
        found.append(
            {
                "id": f"{path}:{index + 1}",
                "name": name,
                "file": path,
                "line": index + 1,
                "signature": line.strip()[:200],
                "doc": strip_doc("\n".join(reversed(doc_lines))),
                "body": "\n".join(body),
            }
        )
    return found


def source_paths() -> list[str]:
    listed = git("ls-tree", "-r", "--name-only", SNAPSHOT, *ROOTS).splitlines()
    raw = [
        path
        for path in listed
        if Path(path).suffix in SOURCE_EXT and not any(part in f"/{path}" for part in SKIP_PARTS)
        and not path.endswith(".d.ts")
        and "/dist/" not in path
    ]
    typed = {path[: -len(Path(path).suffix)] for path in raw if Path(path).suffix in {".ts", ".tsx"}}
    kept = []
    for path in raw:
        if path.endswith(".js") and path[:-3] in typed:
            continue
        if path.endswith(".js.map") or path.endswith(".min.js"):
            continue
        kept.append(path)
    return sorted(kept)


def load_functions() -> list[dict]:
    items = []
    for path in source_paths():
        text = git("show", f"{SNAPSHOT}:{path}")
        items.extend(functions_in(path, text))
    items.sort(key=lambda item: item["id"])
    return items


def same_file_edges(items: list[dict]) -> list[dict]:
    by_file: dict[str, list[dict]] = collections.defaultdict(list)
    for item in items:
        by_file[item["file"]].append(item)
    edges = []
    for file, group in sorted(by_file.items()):
        local: dict[str, str] = {}
        for item in group:
            if len(item["name"]) >= 5:
                local[item["name"]] = item["id"]
        for item in group:
            for name, target in sorted(local.items()):
                if name == item["name"] or target == item["id"]:
                    continue
                if re.search(r"(?<![A-Za-z0-9_.])" + re.escape(name) + r"\s*\(", item["body"]):
                    edges.append(
                        {
                            "source": item["id"],
                            "target": target,
                            "kind": "calls",
                            "confidence": "extracted",
                            "file": file,
                        }
                    )
    edges.sort(key=lambda edge: (edge["source"], edge["target"]))
    return edges


def cross_file_calls(items: list[dict]) -> list[tuple[dict, dict]]:
    by_name: dict[str, list[dict]] = collections.defaultdict(list)
    for item in items:
        if len(item["name"]) >= 5:
            by_name[item["name"]].append(item)
    unique = {name: group[0] for name, group in by_name.items() if len(group) == 1}
    calls = []
    for item in items:
        if len(item["name"]) < 5:
            continue
        for name, target in unique.items():
            if name == item["name"] or target["file"] == item["file"]:
                continue
            if re.search(r"(?<![A-Za-z0-9_.])" + re.escape(name) + r"\s*\(", item["body"]):
                calls.append((item, target))
    calls.sort(key=lambda pair: (pair[0]["id"], pair[1]["id"]))
    return calls


def diff_names(line: str) -> tuple[str, str] | None:
    match = DIFF_FN_RE.match(line.rstrip("\n"))
    if not match:
        return None
    groups = match.groups()
    sign = next(group for group in groups[0::2] if group)
    name = next(group for group in groups[1::2] if group)
    return sign, name


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
SKIP_CRATE_PREFIX = (
    "crates/ax-memory/",
    "crates/ax-neural/",
    "crates/ax-web/web-ui/dist/",
)


def crate_symbol_renames(existing_names: set[str]) -> tuple[list[dict], list[dict]]:
    """Signature-preserving renames from crates that were not diagnosis files.

    Doc-comment tasks are not taken from these files. The old name is the query.
    """
    command = ["git", "log", "-p", "-U0", "--pretty=format:COMMIT %H", SNAPSHOT, "--", "crates"]
    process = subprocess.Popen(command, stdout=subprocess.PIPE, text=True, errors="replace")
    commit = ""
    path = ""
    removed: list[tuple[str, str]] = []
    added: list[tuple[str, str]] = []
    found: list[dict] = []
    keywords = {"async", "function", "export", "pub", "fn", "const", "unsafe"}

    def flush() -> None:
        nonlocal removed, added
        if len(removed) == 1 and len(added) == 1 and removed[0][0] != added[0][0]:
            old, old_line = removed[0]
            new, new_line = added[0]
            if path.startswith(SKIP_CRATE_PREFIX) or path in DIAGNOSIS_FILES or not path.endswith(".rs"):
                removed, added = [], []
                return
            same = re.sub(r"\b" + re.escape(old) + r"\b", "NAME", old_line, count=1) == re.sub(
                r"\b" + re.escape(new) + r"\b", "NAME", new_line, count=1
            )
            tokens = re.findall(r"[A-Za-z_][A-Za-z0-9_]*", old_line)
            if (
                same
                and len(old) >= 5
                and len(new) >= 5
                and old not in existing_names
                and any(token not in keywords and token not in {old, new} for token in tokens)
            ):
                found.append({"commit": commit, "file": path, "old": old, "new": new, "line": old_line})
        removed, added = [], []

    assert process.stdout is not None
    for line in process.stdout:
        if line.startswith("COMMIT "):
            flush()
            commit = line[7:].strip().split()[0]
            path = ""
            continue
        if line.startswith("diff --git "):
            flush()
            parts = line.split()
            path = parts[-1][2:] if parts[-1].startswith("b/") else parts[-1]
            continue
        if line.startswith("@@"):
            flush()
            continue
        parsed = diff_names(line)
        if parsed is None or line.startswith(("---", "+++")) or not line[1:].lstrip().startswith(("pub ", "async ", "fn ", "const ")):
            continue
        sign, name = parsed
        text = line[1:].strip()
        if sign == "-":
            removed.append((name, text))
        else:
            added.append((name, text))
        if len(found) >= 8:
            flush()
            break
    flush()
    process.kill()
    kept = []
    extra: list[dict] = []
    seen = set()
    for row in found:
        key = (row["file"], row["old"], row["new"])
        if key in seen:
            continue
        seen.add(key)
        try:
            text = git("show", f"{SNAPSHOT}:{row['file']}")
        except subprocess.CalledProcessError:
            continue
        funcs = functions_in(row["file"], text)
        match = [item for item in funcs if item["name"] == row["new"]]
        if len(match) != 1:
            continue
        row["id"] = match[0]["id"]
        kept.append(row)
        extra.extend(funcs)
    return kept[:8], extra


def history_tasks(items: list[dict]) -> tuple[list[dict], list[dict]]:
    by_file_name = {(item["file"], item["name"]): item for item in items}
    names_now = {item["name"] for item in items}
    command = [
        "git",
        "log",
        "-p",
        "-U0",
        "--pretty=format:COMMIT %H %s",
        SNAPSHOT,
        "--",
        *ROOTS,
    ]
    process = subprocess.Popen(command, stdout=subprocess.PIPE, text=True, errors="replace")
    commit = ""
    subject = ""
    path = ""
    removed: list[tuple[str, str]] = []
    added: list[tuple[str, str]] = []
    commit_added: list[tuple[str, str]] = []
    renames: list[dict] = []
    fixes: list[dict] = []
    shaped = 0
    missed_file = 0

    def flush_hunk() -> None:
        nonlocal removed, added, shaped, missed_file
        if len(removed) == 1 and len(added) == 1 and removed[0][0] != added[0][0]:
            old, old_line = removed[0]
            new, new_line = added[0]
            shared_tokens = re.findall(r"[A-Za-z_][A-Za-z0-9_]*", old_line)
            keywords = {"async", "function", "export", "default", "pub", "fn", "func", "public", "private", "protected", "static", "final", "const", "unsafe", old, new}
            same_shape = re.sub(r"\b" + re.escape(old) + r"\b", "NAME", old_line, count=1) == re.sub(
                r"\b" + re.escape(new) + r"\b", "NAME", new_line, count=1
            ) and any(token not in keywords for token in shared_tokens)
            if same_shape and len(old) >= 5 and len(new) >= 5 and old not in names_now:
                shaped += 1
                item = by_file_name.get((path, new))
                if item is None:
                    missed_file += 1
                else:
                    renames.append(
                        {
                            "commit": commit,
                            "file": path,
                            "old": old,
                            "new": new,
                            "id": item["id"],
                        }
                    )
        removed, added = [], []

    assert process.stdout is not None
    for line in process.stdout:
        if line.startswith("COMMIT "):
            flush_hunk()
            if len(commit_added) == 1 and re.search(r"(^|[^A-Za-z])(fix|bug)([^A-Za-z]|$)", subject, re.I):
                file, name = commit_added[0]
                item = by_file_name.get((file, name))
                if item is not None and Path(file).suffix in SOURCE_EXT:
                    fixes.append({"commit": commit, "subject": subject, "id": item["id"]})
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
        text = line[1:].strip()
        if sign == "-" and not line.startswith("---"):
            removed.append((name, text))
        elif sign == "+" and not line.startswith("+++"):
            added.append((name, text))
            if len(name) >= 5:
                commit_added.append((path, name))
    flush_hunk()
    process.wait()
    dedup_renames = []
    seen = set()
    for row in sorted(renames, key=lambda item: (item["commit"], item["file"], item["old"], item["new"])):
        key = (row["file"], row["old"], row["new"])
        if key in seen:
            continue
        seen.add(key)
        dedup_renames.append(row)
    dedup_fixes = []
    seen_fix = set()
    for row in sorted(fixes, key=lambda item: (item["commit"], item["id"])):
        if row["id"] in seen_fix:
            continue
        seen_fix.add(row["id"])
        dedup_fixes.append(row)
    return dedup_renames[:20], dedup_fixes[:12]


def prefix_pairs(items: list[dict], calls: set[tuple[str, str]]) -> list[tuple[dict, dict]]:
    unique: dict[str, dict] = {}
    counts = collections.Counter(item["name"] for item in items)
    for item in items:
        if counts[item["name"]] == 1 and len(item["name"]) >= 5:
            unique[item["name"]] = item
    ordered = sorted(unique.values(), key=lambda item: symbol_key(item["name"]))
    pairs = []
    for left in ordered:
        left_key = symbol_key(left["name"])
        for right in ordered:
            if right["id"] <= left["id"]:
                continue
            right_key = symbol_key(right["name"])
            if left_key == right_key or not left_key or not right_key:
                continue
            shared = 0
            for a, b in zip(left_key, right_key):
                if a != b:
                    break
                shared += 1
            if shared < 4 or shared >= min(len(left_key), len(right_key)):
                continue
            if (left["id"], right["id"]) in calls or (right["id"], left["id"]) in calls:
                continue
            pairs.append((left, right))
            break
    return pairs[:20]


def build() -> dict:
    base = load_functions()
    crate_rows, extra = crate_symbol_renames({item["name"] for item in base})
    known_ids = {item["id"] for item in base}
    corpus = list(base)
    for item in extra:
        if item["id"] not in known_ids:
            corpus.append(item)
            known_ids.add(item["id"])
    corpus.sort(key=lambda item: item["id"])
    edges = same_file_edges(corpus)
    cross = cross_file_calls(base)
    call_ids = {(edge["source"], edge["target"]) for edge in edges}
    call_ids.update((left["id"], right["id"]) for left, right in cross)
    renames, fixes = history_tasks(base)
    renames.extend(crate_rows)
    tasks = []
    for row in renames:
        tasks.append(
            {
                "id": f"rename:{row['file']}:{row['old']}:{row['new']}",
                "kind": "symbol_rename",
                "query": row["old"],
                "relevant": [row["id"]],
            }
        )
    outgoing: dict[str, list[str]] = collections.defaultdict(list)
    for edge in edges:
        outgoing[edge["source"]].append(edge["target"])
    by_caller: dict[str, list[dict]] = collections.defaultdict(list)
    for caller, callee in cross:
        by_caller[caller["id"]].append(callee)
    for caller_id, callees in sorted(by_caller.items()):
        caller = next(item for item in base if item["id"] == caller_id)
        other_dir = [callee for callee in callees if Path(callee["file"]).parent != Path(caller["file"]).parent]
        if other_dir:
            tasks.append(
                {
                    "id": f"cross:{caller_id}",
                    "kind": "cross_module_call",
                    "query": caller["name"],
                    "relevant": sorted({callee["id"] for callee in other_dir}),
                }
            )
        further = []
        for callee in other_dir:
            for nxt in outgoing.get(callee["id"], []):
                if nxt != caller["id"]:
                    further.append(nxt)
        if further:
            relevant = {callee["id"] for callee in other_dir}
            relevant.update(further)
            tasks.append(
                {
                    "id": f"trace:{caller_id}",
                    "kind": "multi_file_dependency",
                    "query": caller["name"],
                    "relevant": sorted(relevant),
                }
            )
    for left, _right in prefix_pairs(base, call_ids):
        tasks.append(
            {
                "id": f"similar:{left['id']}",
                "kind": "similar_unrelated",
                "query": left["name"],
                "relevant": [left["id"]],
            }
        )
    for item in base:
        if len(item["doc"]) < 40:
            continue
        tasks.append(
            {
                "id": f"doc:{item['id']}",
                "kind": "function_documentation",
                "query": item["doc"],
                "relevant": [item["id"]],
            }
        )
    for row in fixes:
        tasks.append(
            {
                "id": f"bug:{row['commit']}:{row['id']}",
                "kind": "bug_localization",
                "query": row["subject"],
                "relevant": [row["id"]],
            }
        )
    known = {symbol_key(item["name"]) for item in corpus if symbol_key(item["name"])}
    for query in NEGATIVE_QUERIES:
        tokens = re.split(r"[^A-Za-z0-9_]+", query)
        if any(symbol_key(token) in known and symbol_key(token) for token in tokens):
            continue
        tasks.append(
            {
                "id": f"negative:{symbol_key(query)[:24]}",
                "kind": "negative",
                "query": query,
                "relevant": [],
            }
        )
    tasks.sort(key=lambda task: task["id"])
    candidates = [
        {
            "id": item["id"],
            "name": item["name"],
            "file": item["file"],
            "signature": item["signature"],
        }
        for item in corpus
    ]
    return {
        "snapshot": SNAPSHOT,
        "roots": ROOTS,
        "excludedFromDocTasks": ["crates"],
        "labelRule": (
            "Gold ids are the snapshot function a signature-preserving rename, "
            "a doc comment, or a bare cross-file call names. Same-file call "
            "edges are the only edges given to the ranker. Doc tasks stay "
            "outside crates. Rename tasks may name a function in crates, "
            "excluding the diagnosed files. No score was consulted."
        ),
        "candidates": candidates,
        "edges": [
            {
                "source": edge["source"],
                "target": edge["target"],
                "kind": edge["kind"],
                "confidence": edge["confidence"],
            }
            for edge in edges
        ],
        "tasks": tasks,
    }


def main() -> None:
    payload = build()
    counts = collections.Counter(task["kind"] for task in payload["tasks"])
    destination = Path(__file__).with_name("code-retrieval-frozen.json")
    destination.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    print(f"candidates {len(payload['candidates'])}")
    print(f"edges {len(payload['edges'])}")
    print(f"tasks {len(payload['tasks'])}")
    for kind, count in sorted(counts.items()):
        print(f"  {kind} {count}")


if __name__ == "__main__":
    sys.exit(main())
