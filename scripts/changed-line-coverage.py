#!/usr/bin/env python3
"""Changed-line coverage: every instrumented line added since BASE must have run.

Usage: scripts/changed-line-coverage.py BASE LCOV --fail-under PERCENT [--no-code FILE]... PATH...

Fails closed: an unreadable lcov file, a changed Rust file the lcov report does not
cover at all (unless named with --no-code, for files that only declare and re-export),
or coverage under the threshold all exit nonzero. Lines that are not
instrumented (comments, blank lines, signatures) are not counted.
"""
import argparse
import os
import re
import subprocess
import sys


def added_lines(base, paths):
    diff = subprocess.run(
        ["git", "diff", "--unified=0", base, "HEAD", "--", *paths],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    lines, current = {}, None
    for row in diff.splitlines():
        if row.startswith("+++ "):
            name = row[4:]
            current = name[2:] if name.startswith("b/") and name.endswith(".rs") else None
            if current:
                lines.setdefault(current, set())
        elif current and row.startswith("@@"):
            match = re.match(r"@@ -\S+ \+(\d+)(?:,(\d+))? @@", row)
            if not match:
                raise SystemExit(f"unparsable hunk header: {row}")
            start, count = int(match.group(1)), int(match.group(2) or 1)
            lines[current].update(range(start, start + count))
    return lines


def lcov_hits(path, root):
    hits, current = {}, None
    with open(path) as handle:
        for row in handle:
            row = row.strip()
            if row.startswith("SF:"):
                current = os.path.relpath(row[3:], root)
                hits.setdefault(current, {})
            elif row.startswith("DA:") and current:
                line, count = row[3:].split(",")[:2]
                hits[current][int(line)] = hits[current].get(int(line), 0) + int(count)
    if not hits:
        raise SystemExit(f"{path}: no coverage records")
    return hits


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("base")
    parser.add_argument("lcov")
    parser.add_argument("--fail-under", type=float, required=True)
    parser.add_argument("--no-code", action="append", default=[])
    parser.add_argument("paths", nargs="+")
    args = parser.parse_args()
    root = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"], check=True, capture_output=True, text=True
    ).stdout.strip()
    added = added_lines(args.base, args.paths)
    if not added:
        raise SystemExit("no changed Rust files under the given paths")
    hits = lcov_hits(args.lcov, root)
    instrumented = covered = 0
    for file, lines in sorted(added.items()):
        if file not in hits and file in args.no_code:
            print(f"{file}: declarations only, not instrumented")
            continue
        if file not in hits:
            raise SystemExit(f"{file}: changed but absent from the coverage report")
        measured = sorted(line for line in lines if line in hits[file])
        missed = [line for line in measured if hits[file][line] == 0]
        instrumented += len(measured)
        covered += len(measured) - len(missed)
        print(f"{file}: {len(measured) - len(missed)}/{len(measured)} changed lines run")
        for line in missed:
            print(f"  not run: {file}:{line}")
    if instrumented == 0:
        raise SystemExit("no instrumented changed lines")
    percent = 100.0 * covered / instrumented
    print(f"changed-line coverage {percent:.1f}% ({covered}/{instrumented})")
    if percent < args.fail_under:
        print(f"FAILED: under {args.fail_under}%", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
