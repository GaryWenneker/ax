#!/usr/bin/env python3
"""Manual mutation run for the read-guard hook (docs/specs/read-guard-hook.md).

Each mutant is a plausible bug. The suite must report a failing test for every
one; a mutant that does not apply, does not compile, or survives fails the run.
Sources are restored byte-for-byte and verified by hash after each mutant.

Usage: python3 scripts/read-guard-mutants.py   (from the repo root)
"""

import hashlib
import os
import subprocess
import sys

GUARD = "crates/ax-cli/src/commands/read_guard.rs"
HOOKS = "crates/ax-installer/src/hooks.rs"
GUARD_TESTS = ["cargo", "test", "-p", "ax-cli", "--bin", "ax", "read_guard"]
HOOK_TESTS = ["cargo", "test", "-p", "ax-installer", "hooks::"]

MUTANTS = [
    ("ttl comparison inverted", GUARD,
     "now - ts < TTL_SECS)", "now - ts > TTL_SECS)", GUARD_TESTS),
    ("null offset counts as partial read", GUARD,
     "tool_input.get(*k).is_some_and(|v| !v.is_null())", "tool_input.get(*k).is_some()", GUARD_TESTS),
    ("piped cat treated as whole read", GUARD,
     "            if followed {\n                return Probe::Pass;\n            }\n", "", GUARD_TESTS),
    ("state write failure still denies (deny loop)", GUARD,
     "state.save(state_path).ok()?;", "let _ = state.save(state_path);", GUARD_TESTS),
    ("minimum symbol length dropped", GUARD,
     "(last.len() >= MIN_SYMBOL_LEN)", "(!last.is_empty())", GUARD_TESTS),
    ("doc nodes counted as symbols", GUARD,
     "\"('file','doc','table')\"", "\"('file','table')\"", GUARD_TESTS),
    ("table nodes counted as symbols", GUARD,
     "\"('file','doc','table')\"", "\"('file','doc')\"", GUARD_TESTS),
    ("windsurf deny does not block", GUARD,
     "stderr: Some(d.full.clone()), exit_code: 2 }", "stderr: Some(d.full.clone()), exit_code: 0 }", GUARD_TESTS),
    ("cursor user_message loses the guidance", GUARD,
     "\"user_message\": d.full,", "\"user_message\": \"denied\",", GUARD_TESTS),
    ("search scope ignored", GUARD,
     "substr(file_path, 1, length(?)) = ?)", "length(?) = length(?))", GUARD_TESTS),
    ("retry never allowed", GUARD,
     "    if state.seen(&key, &target, now) {\n        return None;\n    }\n", "", GUARD_TESTS),
    ("every hook treated as ours", HOOKS,
     ".is_some_and(|c| c.contains(MARKER))", ".is_some()", HOOK_TESTS),
    ("install always appends (not idempotent)", HOOKS,
     "match items.iter().position(&matches) {", "match None::<usize>.filter(|_| items.iter().any(&matches)) {", HOOK_TESTS),
    ("invalid JSON silently becomes {}", HOOKS,
     "Ok(raw) => serde_json::from_str(&raw).map_err(|e| format!(\"{} is not valid JSON ({e}); left unchanged\", path.display())),",
     "Ok(raw) => Ok(serde_json::from_str(&raw).unwrap_or_else(|_| json!({}))),", HOOK_TESTS),
    ("emptied-event check uses the global flag", HOOKS,
     "if event_changed && items.is_empty() {", "if changed && items.is_empty() {", HOOK_TESTS),
]


def sha(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


def run(cmd):
    env = dict(os.environ)
    env.pop("CARGO_TARGET_DIR", None)
    return subprocess.run(cmd, capture_output=True, text=True, env=env)


def main():
    originals = {p: open(p, "rb").read() for p in (GUARD, HOOKS)}
    hashes = {p: sha(p) for p in originals}
    for cmd in (GUARD_TESTS, HOOK_TESTS):
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
