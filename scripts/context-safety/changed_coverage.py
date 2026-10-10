#!/usr/bin/env python3
"""Fail when an instrumented changed Rust line has no execution evidence."""
import argparse
import json
import re
import subprocess
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("lcov", type=Path)
parser.add_argument("--base", default="9f6cbd09eac4a54810f7272f62ad2d5106b51c5d")
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
root = Path(subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip())
coverage = {}
current = None
for line in args.lcov.read_text().splitlines():
    if line.startswith("SF:"):
        path = Path(line[3:])
        try:
            current = path.resolve().relative_to(root.resolve()).as_posix()
        except ValueError:
            current = None
        if current:
            coverage.setdefault(current, {})
    elif current and line.startswith("DA:"):
        number, count, *_ = line[3:].split(",")
        table = coverage[current]
        table[int(number)] = table.get(int(number), 0) + int(count)

diff = subprocess.check_output(["git", "diff", "--no-ext-diff", "--unified=0", args.base, "--", "*.rs"], text=True)
changed = {}
file = None
number = 0
for line in diff.splitlines():
    if line.startswith("+++ b/"):
        file = line[6:]
    elif line.startswith("@@"):
        number = int(re.search(r"\+(\d+)", line).group(1))
    elif line.startswith("+") and file:
        changed.setdefault(file, set()).add(number)
        number += 1
    elif line.startswith(" "):
        number += 1

rows = []
for file, lines in sorted(changed.items()):
    table = coverage.get(file, {})
    measured = sorted(lines & table.keys())
    uncovered = [line for line in measured if table[line] == 0]
    rows.append({"path":file,"instrumentedChangedLines":len(measured),"covered":len(measured)-len(uncovered),"uncovered":uncovered,
                 "fileMeasured":file in coverage})
report = {"threshold":100,"files":rows,"instrumentedChangedLines":sum(r["instrumentedChangedLines"] for r in rows),
          "uncovered":sum(len(r["uncovered"]) for r in rows)}
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text(json.dumps(report, indent=2)+"\n")
print(json.dumps({k:v for k,v in report.items() if k != "files"}))
# Declarations and non-executable lines do not have DA entries. Unmeasured files
# are visible in the report and must be reviewed, never claimed as covered.
raise SystemExit(1 if report["uncovered"] else 0)
