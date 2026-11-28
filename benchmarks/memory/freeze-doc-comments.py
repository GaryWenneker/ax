#!/usr/bin/env python3
"""Freeze the doc-comment benchmark input from one git snapshot.

Each pair is a `///` doc comment of at least 40 characters and the
`pub fn` / `pub async fn` signature line it documents, taken from `crates/`
at SNAPSHOT, outside `crates/ax-memory` and `crates/ax-neural`. The
extraction rule is the one the doc-comment measurement was recorded with.
This script does not rank anything and does not read a score.

Usage: python3 benchmarks/memory/freeze-doc-comments.py [output.json]
"""

from __future__ import annotations

import io
import json
import subprocess
import sys
import tarfile
from pathlib import Path

SNAPSHOT = "1f9de3ea"
SKIPPED_CRATES = {"ax-memory", "ax-neural"}
DEFAULT_OUTPUT = Path(__file__).with_name("doc-comment-frozen.json")


def documented_functions(files: dict[str, str]) -> list[dict[str, str]]:
    out = []
    for rel, text in files.items():
        doc: list[str] = []
        lines = text.split("\n")
        for index, line in enumerate(lines):
            line = line.removesuffix("\r")
            trimmed = line.strip()
            if trimmed.startswith("///"):
                doc.append(trimmed[3:].strip())
                continue
            if trimmed.startswith("#") or trimmed.startswith("//") or not trimmed:
                if trimmed.startswith("//") and not trimmed.startswith("///"):
                    doc.clear()
                continue
            if doc and (trimmed.startswith("pub fn ") or trimmed.startswith("pub async fn ")):
                query = " ".join(doc)
                if len(query) >= 40:
                    out.append({"id": f"{rel}:{index + 1}", "query": query, "signature": trimmed})
            doc.clear()
    out.sort(key=lambda item: item["id"])
    return out


def snapshot_files(repo: Path) -> tuple[str, dict[str, str]]:
    commit = subprocess.run(
        ["git", "rev-parse", f"{SNAPSHOT}^{{commit}}"],
        cwd=repo, check=True, capture_output=True, text=True,
    ).stdout.strip()
    archive = subprocess.run(
        ["git", "archive", "--format=tar", commit, "crates"],
        cwd=repo, check=True, capture_output=True,
    ).stdout
    files = {}
    with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
        for member in tar.getmembers():
            parts = member.name.split("/")
            if not member.isfile() or not member.name.endswith(".rs"):
                continue
            if len(parts) > 2 and parts[1] in SKIPPED_CRATES:
                continue
            data = tar.extractfile(member).read()
            try:
                files[member.name] = data.decode("utf-8")
            except UnicodeDecodeError:
                continue
    return commit, files


def main() -> int:
    output = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_OUTPUT
    repo = Path(__file__).resolve().parents[2]
    commit, files = snapshot_files(repo)
    pairs = documented_functions(files)
    frozen = {"protocol": "doc-comment-affix-1", "snapshot": commit, "pairs": pairs}
    output.write_text(json.dumps(frozen, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"{len(pairs)} pairs from {commit} -> {output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
