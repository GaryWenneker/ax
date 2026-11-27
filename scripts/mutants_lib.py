"""Shared engine for the Python mutant runners (scripts/*-mutants.py).

A runner defines a MUTANTS table of (name, path, old, new, test_command) and
calls run_mutants(MUTANTS, suites). The engine:

* runs every suite once on the unmodified tree and stops if one is red;
* applies each mutant exactly once (the old text must occur exactly once);
* restores the file byte for byte in a `finally`, so an interrupt or crash
  never leaves mutated source behind, and checks the restore by sha256;
* counts a mutant as killed only when a test failed. A build error, a survivor,
  a mutant that does not apply, or any other exit fails the run.

The shell runners use scripts/lib/mutate.sh, which follows the same rules.
"""

from __future__ import annotations

import hashlib
import os
import subprocess
from collections.abc import Callable, Iterable, Sequence

Mutant = tuple[str, str, str, str, Sequence[str]]
Runner = Callable[[Sequence[str]], "subprocess.CompletedProcess[str]"]


def sha(path: str) -> str:
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


def run(cmd: Sequence[str]) -> subprocess.CompletedProcess[str]:
    env = dict(os.environ)
    env.pop("CARGO_TARGET_DIR", None)
    return subprocess.run(list(cmd), capture_output=True, text=True, env=env)


def classify(returncode: int, text: str) -> str:
    """Return killed, build-error, survived or unexpected for one test run."""
    if "error[E" in text or "could not compile" in text:
        return "build-error"
    if returncode != 0 and "test result: FAILED" in text:
        return "killed"
    if returncode == 0:
        return "survived"
    return "unexpected"


def failed_tests(text: str) -> list[str]:
    return [l.split()[1] for l in text.splitlines() if l.startswith("test ") and l.endswith("FAILED")]


def run_mutants(mutants: Sequence[Mutant], suites: Iterable[Sequence[str]], runner: Runner = run) -> int:
    """Run every mutant; return 0 only when all were killed by a failing test."""
    paths = sorted({m[1] for m in mutants})
    originals = {}
    for p in paths:
        with open(p, "rb") as f:
            originals[p] = f.read()
    hashes = {p: sha(p) for p in paths}
    for cmd in suites:
        base = runner(cmd)
        if base.returncode != 0:
            print(f"BASELINE RED: {' '.join(cmd)}\n{base.stdout[-2000:]}{base.stderr[-2000:]}")
            return 1
    killed, problems = 0, []
    for name, path, old, new, cmd in mutants:
        src = originals[path].decode()
        if src.count(old) != 1:
            problems.append(f"NOT APPLIED ({src.count(old)} matches): {name}")
            continue
        try:
            with open(path, "w", encoding="utf-8", newline="") as f:
                f.write(src.replace(old, new))
            out = runner(cmd)
        finally:
            with open(path, "wb") as f:
                f.write(originals[path])
        if sha(path) != hashes[path]:
            print(f"RESTORE FAILED for {path}")
            return 1
        text = out.stdout + out.stderr
        verdict = classify(out.returncode, text)
        if verdict == "killed":
            killed += 1
            print(f"killed   {name}  <- {', '.join(failed_tests(text)[:3])}")
        elif verdict == "build-error":
            problems.append(f"DID NOT COMPILE: {name}")
        elif verdict == "survived":
            problems.append(f"SURVIVED: {name}")
        else:
            problems.append(f"UNEXPECTED EXIT {out.returncode}: {name}")
    print(f"\n{killed}/{len(mutants)} mutants killed; sources restored (sha256 verified)")
    for p in problems:
        print(p)
    return 0 if killed == len(mutants) and not problems else 1
