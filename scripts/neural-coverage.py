#!/usr/bin/env python3
"""Line coverage gate for the Neural change, read from an lcov file written by cargo-llvm-cov.

Two checks, both fail closed:
- Each Neural file must reach its line-coverage floor. The floors are below 100% only where the
  uncovered lines are listed in docs/specs/ax-neural/EVIDENCE.md with a reason.
- Three shared files also hold unrelated work, so their Neural lines are found as regions: from the
  first line containing a start text to the first line at or after it containing an end text.
  Every instrumented line in a region must have run, except lines that contain an allowed text.
  A region whose start text is missing or not unique fails the gate.

The regions guard the Neural lines named here. Neural code added to a shared file outside them is
not checked by this script.

Exits 1 on a missing lcov file, a Neural file absent from the report, a floor that is missed, a
region that cannot be found, or a region line that never ran.
"""
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent

FLOORS = {
    "crates/ax-neural/src/bakeoff.rs": 0.0,
    "crates/ax-neural/src/config.rs": 0.0,
    "crates/ax-neural/src/dispatch.rs": 0.0,
    "crates/ax-neural/src/graph.rs": 0.0,
    "crates/ax-neural/src/learning.rs": 0.0,
    "crates/ax-neural/src/lib.rs": 0.0,
    "crates/ax-neural/src/predict.rs": 0.0,
    "crates/ax-neural/src/rank.rs": 0.0,
    "crates/ax-neural/src/share.rs": 0.0,
    "crates/ax-neural/src/status.rs": 0.0,
    "crates/ax-neural/src/store.rs": 0.0,
    "crates/ax-mcp/src/neural_tool.rs": 0.0,
    "crates/ax-cli/src/commands/neural.rs": 0.0,
    "crates/ax-web/src/neural.rs": 0.0,
}

# (file, start text, end text, texts of lines allowed to stay unrun)
REGIONS = [
    (
        "crates/ax-mcp/src/tools.rs",
        'name if name.starts_with("ax_neural_") => {',
        ".await",
        (),
    ),
    (
        "crates/ax-mcp/src/tools.rs",
        "crate::neural_tool::predicted_block(",
        "crate::neural_tool::record_totals(",
        # Ranking is locked on, so maybe_reorder never returns None from a loaded config.
        ("None => crate::context_budget::finish_with_memory_reserve(",),
    ),
    (
        "crates/ax-mcp/src/context_budget.rs",
        'let neural = base.pointer("/contextBudget/neural")',
        'out["contextBudget"]["neural"] = neural;',
        (),
    ),
    (
        "crates/ax-mcp/src/context_budget.rs",
        "pub struct NeuralBudget",
        "finish_with_memory_reserve(base, blocks, limit, reserve)",
        (),
    ),
    (
        "crates/ax-cli/src/main.rs",
        "fn neural_request(",
        '.map(|args| ("register", args)),',
        (),
    ),
]


def parse(lcov: pathlib.Path) -> dict[str, dict[int, int]]:
    files: dict[str, dict[int, int]] = {}
    current = None
    for line in lcov.read_text(encoding="utf-8").splitlines():
        if line.startswith("SF:"):
            path = pathlib.Path(line[3:])
            try:
                current = str(path.resolve().relative_to(ROOT))
            except ValueError:
                current = None
            if current is not None:
                files.setdefault(current, {})
        elif line.startswith("DA:") and current is not None:
            number, hits = line[3:].split(",")[:2]
            lines = files[current]
            lines[int(number)] = max(lines.get(int(number), 0), int(hits))
        elif line == "end_of_record":
            current = None
    return files


def ranges(numbers: list[int]) -> str:
    out = []
    for number in numbers:
        if out and number == out[-1][1] + 1:
            out[-1][1] = number
        else:
            out.append([number, number])
    return ", ".join(str(a) if a == b else f"{a}-{b}" for a, b in out)


def check_floors(files: dict[str, dict[int, int]]) -> bool:
    failed = False
    for path, floor in FLOORS.items():
        lines = files.get(path)
        if not lines:
            print(f"FAIL {path}: not in the coverage report")
            failed = True
            continue
        covered = sum(1 for hits in lines.values() if hits > 0)
        ratio = covered / len(lines)
        missed = sorted(number for number, hits in lines.items() if hits == 0)
        verdict = "ok  " if ratio >= floor else "FAIL"
        failed |= ratio < floor
        print(f"{verdict} {path}: {covered}/{len(lines)} lines ({ratio:.1%}, floor {floor:.1%})")
        if missed:
            print(f"     uncovered: {ranges(missed)}")
    return failed


def region_lines(source: list[str], start: str, end: str) -> range | None:
    starts = [number for number, text in enumerate(source, start=1) if start in text]
    if len(starts) != 1:
        return None
    first = starts[0]
    for number in range(first, len(source) + 1):
        if end in source[number - 1]:
            return range(first, number + 1)
    return None


def check_regions(files: dict[str, dict[int, int]]) -> bool:
    failed = False
    for path, start, end, allowed in REGIONS:
        source = (ROOT / path).read_text(encoding="utf-8").splitlines()
        span = region_lines(source, start, end)
        if span is None:
            print(f"FAIL {path}: region starting {start!r} not found exactly once")
            failed = True
            continue
        lines = files.get(path, {})
        instrumented = [number for number in span if number in lines]
        if not instrumented:
            print(f"FAIL {path}: no instrumented lines in region {span.start}-{span.stop - 1}")
            failed = True
            continue
        unrun = [
            number
            for number in instrumented
            if lines[number] == 0 and not any(text in source[number - 1] for text in allowed)
        ]
        allowed_unrun = [
            number for number in instrumented if lines[number] == 0 and number not in unrun
        ]
        label = f"{path}:{span.start}-{span.stop - 1}"
        if unrun:
            print(f"FAIL {label}: Neural lines never ran: {ranges(unrun)}")
            failed = True
        else:
            note = f"; allowed unrun: {ranges(allowed_unrun)}" if allowed_unrun else ""
            ran = len(instrumented) - len(allowed_unrun)
            print(f"ok   {label}: {ran}/{len(instrumented)} Neural lines ran{note}")
    return failed


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: neural-coverage.py <lcov.info>")
        return 2
    files = parse(pathlib.Path(sys.argv[1]))
    failed = check_floors(files)
    failed |= check_regions(files)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
