#!/usr/bin/env python3
"""Verdict for one `cargo test --no-fail-fast` run, read from stdin (not run with -q).

usage: neural-test-verdict.py <label> <baseline file> <cargo exit status>

The baseline file lists failures that exist without the Neural change, one per line as
`<label> <test name>`, with `#` comments. Fails closed: a compile error, a run without any
`test result:` line, a test binary that started but printed no result (a crash or abort), a failing
test that is not in the baseline for this label, or a non-zero exit that no failing test explains
exits 1. A baseline test that passes is reported, not failed.
"""
import pathlib
import re
import sys

RESULT = re.compile(r"^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored", re.M)
FAILED = re.compile(r"^test (.+?) \.\.\. FAILED$", re.M)
STARTED = re.compile(r"^\s+(?:Running|Doc-tests) ", re.M)


def baseline(path: pathlib.Path, label: str) -> set[str]:
    names = set()
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        owner, name = line.split(None, 1)
        if owner == label:
            names.add(name.strip())
    return names


def main() -> int:
    if len(sys.argv) != 4:
        print(__doc__)
        return 2
    label, path, status = sys.argv[1], pathlib.Path(sys.argv[2]), int(sys.argv[3])
    out = sys.stdin.read()
    known = baseline(path, label)
    if "could not compile" in out:
        print(f"FAIL {label}: compile error")
        print("\n".join(line for line in out.splitlines() if line.startswith("error"))[:4000])
        return 1
    results = RESULT.findall(out)
    if not results:
        print(f"FAIL {label}: no test result line; the tests did not run")
        print(out[-4000:])
        return 1
    started = len(STARTED.findall(out))
    if started != len(results):
        print(f"FAIL {label}: {started} test binaries started, {len(results)} printed a result")
        print(out[-4000:])
        return 1
    passed, failed, ignored = (sum(int(row[i]) for row in results) for i in range(3))
    failing = set(FAILED.findall(out))
    if status != 0 and not failing:
        print(f"FAIL {label}: exit {status} with no failing test to explain it")
        print(out[-4000:])
        return 1
    unexpected = sorted(failing - known)
    print(
        f"{'FAIL' if unexpected else 'ok  '} {label}: {passed} passed, {failed} failed, "
        f"{ignored} ignored, {len(results)} test binaries"
    )
    for name in sorted(failing & known):
        print(f"     baseline failure: {name}")
    for name in sorted(known - failing):
        print(f"     baseline test passed this run: {name}")
    for name in unexpected:
        print(f"     NEW failure: {name}")
    return 1 if unexpected else 0


if __name__ == "__main__":
    sys.exit(main())
