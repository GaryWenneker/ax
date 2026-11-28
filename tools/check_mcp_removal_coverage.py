#!/usr/bin/env python3
"""Fail on uncovered changed executable Rust lines in llvm-cov JSON."""
import argparse
import json
import re
import subprocess
from pathlib import Path


def check(report: dict[str, object], diff: str, root: Path) -> dict[str, object]:
    files = {}
    for data in report["data"]:
        for entry in data["files"]:
            filename = str(Path(entry["filename"]).resolve())
            files[filename] = entry["segments"]
    changed = {}
    path, line = None, 0
    for text in diff.splitlines():
        if text.startswith("+++ b/"):
            path = text[6:]
        elif text.startswith("@@"):
            match = re.search(r"\+(\d+)", text)
            if not match: raise ValueError("invalid diff hunk")
            line = int(match.group(1))
        elif text.startswith("+") and not text.startswith("+++"):
            if path and path.startswith("crates/ax-mcp/src/") and path.endswith(".rs"):
                changed.setdefault(path, {})[line] = text[1:]
            line += 1
        elif not text.startswith("-") and not text.startswith("\\"):
            line += 1
    covered, total, missing, declarations = 0, 0, [], []
    for path, lines in changed.items():
        absolute = str((root / path).resolve())
        if absolute not in files:
            # LLVM omits crate roots containing only module declarations. Only
            # those exact declaration/comment lines are exempt; missing
            # executable source still fails closed.
            if all(not text.strip() or text.strip().startswith("//") or re.fullmatch(r"(?:pub(?:\([^)]*\))?\s+)?mod\s+[A-Za-z_]\w*\s*;", text.strip()) for text in lines.values()):
                declarations.append(path)
                continue
            raise ValueError(f"coverage missing changed source: {path}")
        segments = files[absolute]
        # LLVM segments describe half-open source regions. A line is executable
        # if a counted, nongap region overlaps it, including zero-count regions.
        executable = {}
        for index, segment in enumerate(segments):
            start, column, count, has_count, _ = segment[:5]
            is_gap = len(segment) > 5 and segment[5]
            if not has_count or is_gap: continue
            end_line, end_column = segments[index + 1][:2] if index + 1 < len(segments) else (start, column + 1)
            last_line = end_line if end_column > 1 else end_line - 1
            for current in range(start, last_line + 1):
                executable[current] = executable.get(current, False) or count > 0
        for current in lines:
            if current in executable:
                total += 1
                if executable[current]: covered += 1
                else: missing.append(f"{path}:{current}")
    if not total: raise ValueError("no changed executable lines were checked")
    return {"covered": covered, "total": total, "missing": missing, "declaration_only_files": declarations,
            "scope": "changed executable line coverage; not branch coverage"}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", required=True)
    parser.add_argument("--base", required=True)
    args = parser.parse_args()
    try:
        root = Path(subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip())
        diff = subprocess.check_output(["git", "diff", "--unified=0", args.base, "--", "crates/ax-mcp/src"], text=True)
        result = check(json.loads(Path(args.report).read_text(encoding="utf-8")), diff, root)
    except (OSError, ValueError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"coverage check failed: {error}\n")
    print(json.dumps(result, indent=2))
    if result["missing"]: parser.exit(1, "changed executable lines are uncovered\n")


if __name__ == "__main__": main()
