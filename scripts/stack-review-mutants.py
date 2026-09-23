#!/usr/bin/env python3
"""Manual mutation run for the stack review skills (docs/specs/dotnet-code-review-v2.md, nextjs-review-v2.md, stack-review-skills-v2.md).

Each mutant is a plausible bug. The suite must report a failing test for every
one; a mutant that does not apply, does not compile, or survives fails the run.
Sources are restored byte-for-byte and verified by hash after each mutant.

Usage: python3 scripts/stack-review-mutants.py   (from the repo root)
"""

import hashlib
import os
import subprocess
import sys

SKILL = "crates/ax-policy/templates/stacks/dotnet/skills/dotnet-code-review/SKILL.md"
NEXT = "crates/ax-policy/templates/stacks/nextjs/skills/nextjs-review/SKILL.md"
CATALOG = "crates/ax-policy/src/stack_catalog.rs"
TESTS = ["cargo", "test", "-p", "ax-policy", "--lib", "stacks::"]


def stack_skill(stack: str) -> str:
    return f"crates/ax-policy/templates/stacks/{stack}/skills/{stack}-review/SKILL.md"


def bullet_lines(path: str) -> list[str]:
    with open(path, encoding="utf-8") as f:
        return [line for line in f if line.startswith("- ")]


def first_bullet(path: str) -> str:
    lines = bullet_lines(path)
    if not lines:
        raise SystemExit(f"no rule lines in {path}")
    return lines[0]


def truncated_tail(path: str, keep: int) -> tuple[str, str]:
    """Old/new pair that deletes every rule line after the first `keep`."""
    with open(path, encoding="utf-8") as f:
        text = f.read()
    lines = bullet_lines(path)
    cut = lines[keep]
    start = text.index(cut)
    end = text.index("## ", text.index(lines[-1]))
    kept = "".join(l for l in text[start:end].splitlines(keepends=True) if not l.startswith("- "))
    return text[start:end], kept

MUTANTS = [
    ("EF Core section dropped", SKILL, "## 7. EF Core\n", "## 7. Data\n", TESTS),
    ("async ban lost", SKILL,
     "- No `async void`, except native UI event handlers. Use `async Task` or `async ValueTask`.\n", "", TESTS),
    ("bullet duplicated across sections", SKILL,
     "- Use `.Any()`, not `.Count() > 0`.\n",
     "- Use `.Any()`, not `.Count() > 0`.\n- use `.any()`, not `.count() > 0`.\n", TESTS),
    ("verdict set reverted", SKILL, "`[APPROVED WITH WARNINGS]`", "`[CRITICAL BLOCKER]`", TESTS),
    ("pack version not bumped", CATALOG,
     'StackDef { id: "dotnet", version: "1.2.0"', 'StackDef { id: "dotnet", version: "1.1.0"', TESTS),
    ("skill swapped for the old short body", SKILL,
     "## 1. Naming and casing", "## 1. Naming", TESTS),
    ("nextjs: Server Action auth check dropped", NEXT,
     "- Every Server Action checks the session and the caller's authorization inside the action. It never trusts the client or the form.\n",
     "", TESTS),
    ("nextjs: Server Action validation weakened", NEXT,
     "validates its input with Zod before", "validates its input before", TESTS),
    ("nextjs: react-review rule copied in", NEXT,
     "## 6. Client state and React 19\n\n",
     "## 6. Client state and React 19\n\n- Keys are stable ids from the data. Do not use the array index when the list can reorder.\n", TESTS),
    ("nextjs: redirect try/catch note lost", NEXT,
     "call them outside `try/catch`, or rethrow", "call them anywhere", TESTS),
    ("nextjs: pack version not bumped", CATALOG,
     'StackDef { id: "nextjs", version: "1.2.0"', 'StackDef { id: "nextjs", version: "1.1.0"', TESTS),
    ("pascal: a review section lost", stack_skill("pascal"), "## 10. Testing\n", "## Testing\n", TESTS),
    ("rust: output format renamed", stack_skill("rust"), "## 12. Output format\n", "## 12. Report\n", TESTS),
    ("svelte: Location label dropped", stack_skill("svelte"), "**Location:**", "**Where:**", TESTS),
    ("python: rule duplicated", stack_skill("python"),
     first_bullet(stack_skill("python")), first_bullet(stack_skill("python")) * 2, TESTS),
    ("laravel: php-review rule copied in", stack_skill("laravel"),
     first_bullet(stack_skill("laravel")), first_bullet(stack_skill("laravel")) + first_bullet(stack_skill("php")), TESTS),
    ("typescript: intro no longer names javascript-review", stack_skill("typescript"),
     "This skill builds on `javascript-review`;", "This skill builds on the JavaScript skill;", TESTS),
    ("r: skill truncated to 49 rules", stack_skill("r"), *truncated_tail(stack_skill("r"), 49), TESTS),
    ("go: pack.toml not bumped", "crates/ax-policy/templates/stacks/go/pack.toml",
     'version = "1.2.0"', 'version = "1.1.0"', TESTS),
    ("kotlin: catalog not bumped", CATALOG,
     'StackDef { id: "kotlin", version: "1.2.0"', 'StackDef { id: "kotlin", version: "1.1.0"', TESTS),
]


def read_bytes(path: str) -> bytes:
    with open(path, "rb") as f:
        return f.read()


def sha(path: str) -> str:
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


def run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    env = dict(os.environ)
    env.pop("CARGO_TARGET_DIR", None)
    return subprocess.run(cmd, capture_output=True, text=True, env=env)


def main() -> int:
    paths = sorted({m[1] for m in MUTANTS})
    originals = {p: read_bytes(p) for p in paths}
    hashes = {p: sha(p) for p in paths}
    for cmd in (TESTS,):
        base = run(cmd)
        if base.returncode != 0:
            print(f"BASELINE RED: {' '.join(cmd)}\n{base.stdout[-2000:]}{base.stderr[-2000:]}")
            return 1
    killed, problems = 0, []
    for name, path, old, new, cmd in MUTANTS:
        src = originals[path].decode()
        if src.count(old) != 1:
            problems.append(f"NOT APPLIED ({src.count(old)} matches): {name}")
            continue
        try:
            with open(path, "w") as f:
                f.write(src.replace(old, new))
            out = run(cmd)
        finally:
            with open(path, "wb") as f:
                f.write(originals[path])
        if sha(path) != hashes[path]:
            print(f"RESTORE FAILED for {path}")
            return 1
        text = out.stdout + out.stderr
        if "error[E" in text or "could not compile" in text:
            problems.append(f"DID NOT COMPILE: {name}")
        elif out.returncode != 0 and "test result: FAILED" in text:
            failed = [l.split()[1] for l in text.splitlines() if l.startswith("test ") and l.endswith("FAILED")]
            killed += 1
            print(f"killed   {name}  <- {', '.join(failed[:3])}")
        elif out.returncode == 0:
            problems.append(f"SURVIVED: {name}")
        else:
            problems.append(f"UNEXPECTED EXIT {out.returncode}: {name}")
    print(f"\n{killed}/{len(MUTANTS)} mutants killed; sources restored (sha256 verified)")
    for p in problems:
        print(p)
    return 0 if killed == len(MUTANTS) and not problems else 1


if __name__ == "__main__":
    sys.exit(main())
