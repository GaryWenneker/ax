#!/usr/bin/env python3
"""Every test the Neural spec names must exist as a Rust test function under crates/.

Reads the last column of each Markdown table row in the spec. A backticked snake_case token there
is a test reference when it has a `::` path or at least three underscores; `learning` or `store`
alone name a module, not a test. Exits 1 when a referenced test is missing, when fewer than
MINIMUM references are found, or when the spec cannot be read.

Coverage of the rule: this proves the named function exists and is marked as a test. It does not
prove the test checks the scenario written next to it; that is what the mutation runs are for.
TypeScript tests are listed in the spec by file and title; this script does not check them.
"""
import pathlib
import re
import sys

MINIMUM = 40
TOKEN = re.compile(r"`([a-z0-9_]+(?:::[a-z0-9_]+)*)`")
TEST_ATTR = re.compile(r"#\[(?:tokio::)?test(?:\([^)]*\))?\]")
# Markdown writes a pipe inside a cell as `\|`; only an unescaped pipe ends a cell.
UNESCAPED_PIPE = re.compile(r"(?<!\\)\|")


def references(spec: str) -> list[str]:
    names = []
    for line in spec.splitlines():
        cells = [cell.strip() for cell in UNESCAPED_PIPE.split(line.strip().strip("|"))]
        if not line.lstrip().startswith("|") or len(cells) < 2:
            continue
        for token in TOKEN.findall(cells[-1]):
            if "::" in token or token.count("_") >= 3:
                names.append(token)
    return names


def test_functions(root: pathlib.Path) -> set[str]:
    found = set()
    for path in root.glob("crates/**/*.rs"):
        if "target" in path.parts:
            continue
        lines = path.read_text(encoding="utf-8").splitlines()
        for index, line in enumerate(lines):
            match = re.match(r"\s*(?:pub\s+)?(?:async\s+)?fn\s+([a-z0-9_]+)\s*\(", line)
            if not match:
                continue
            window = lines[max(0, index - 3) : index]
            if any(TEST_ATTR.search(above) for above in window):
                found.add(match.group(1))
    return found


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: neural-spec-map.py <spec.md>")
        return 2
    root = pathlib.Path(__file__).resolve().parent.parent
    spec = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
    names = sorted(set(references(spec)))
    tests = test_functions(root)
    missing = [name for name in names if name.split("::")[-1] not in tests]
    for name in missing:
        print(f"missing test: {name}")
    if len(names) < MINIMUM:
        print(f"only {len(names)} test references found; expected at least {MINIMUM}")
        return 1
    if missing:
        return 1
    print(f"spec map: {len(names)} referenced tests exist")
    return 0


if __name__ == "__main__":
    sys.exit(main())
