#!/usr/bin/env python3
"""Manual mutation runner for docs/specs/preflight-quality.md.

Each mutant is one source edit that reintroduces a bug the spec forbids. The
named test command must FAIL with the mutant applied. The file is restored
after every mutant, also on error.

Fails closed: a search string that is missing or not unique, a test command
that cannot run, or a surviving mutant makes the run exit nonzero. The
`control` entry is an edit that changes no behavior; it must SURVIVE, which
proves the runner can tell a survivor from a kill.

Usage: tools/preflight_quality_mutants.py   (run from the repository root)
"""

import os
import subprocess
import sys

TARGET_DIR = os.environ.get("CARGO_TARGET_DIR", "target-dev")

MUTANTS = [
    {
        "id": "C-substring",
        "file": "crates/ax-policy/src/matcher.rs",
        "find": "if !t.is_empty() && contains_trigger_word(prompt_lc, &t) {",
        "replace": "if !t.is_empty() && prompt_lc.contains(&t) {",
        "test": ["cargo", "test", "-p", "ax-policy", "--lib", "matcher::tests::c"],
    },
    {
        "id": "C-no-inflection",
        "file": "crates/ax-policy/src/matcher.rs",
        "find": "tail.is_empty() || (inflects && TRIGGER_SUFFIXES.contains(&tail))",
        "replace": "tail.is_empty()",
        "test": ["cargo", "test", "-p", "ax-policy", "--lib", "matcher::tests::c"],
    },
    {
        "id": "C-any-suffix",
        "file": "crates/ax-policy/src/matcher.rs",
        "find": "tail.is_empty() || (inflects && TRIGGER_SUFFIXES.contains(&tail))",
        "replace": "tail.is_empty() || inflects",
        "test": ["cargo", "test", "-p", "ax-policy", "--lib", "matcher::tests::c"],
    },
    {
        "id": "D-questions-count",
        "file": "crates/ax-policy/src/capture.rs",
        "find": "if sentence.len() < 8 || sentence.ends_with('?') {",
        "replace": "if sentence.len() < 8 {",
        "test": ["cargo", "test", "-p", "ax-policy", "--lib", "capture::tests"],
    },
    {
        "id": "D-first-person",
        "file": "crates/ax-policy/src/capture.rs",
        "find": "&& !FIRST_PERSON.contains(&previous)",
        "replace": "",
        "test": ["cargo", "test", "-p", "ax-policy", "--lib", "capture::tests"],
    },
    {
        "id": "D-mid-sentence-word",
        "file": "crates/ax-policy/src/capture.rs",
        "find": "            .strip_prefix(prefix)\n            .is_some_and(|rest| ends_word(rest) && !rest.trim().is_empty())",
        "replace": "            .split_once(prefix)\n            .is_some_and(|(_, rest)| ends_word(rest) && !rest.trim().is_empty())",
        "test": ["cargo", "test", "-p", "ax-policy", "--lib", "capture::tests"],
    },
    {
        "id": "E-notification-kept",
        "file": "crates/ax-context/src/directory.rs",
        "find": "if text.starts_with(HARNESS_NOTIFICATION) {",
        "replace": "if false {",
        "test": ["cargo", "test", "-p", "ax-context", "--lib", "prompt_tests"],
    },
    {
        "id": "E-reminder-kept",
        "file": "crates/ax-context/src/directory.rs",
        "find": "    while let Some(open) = rest.find(HARNESS_BLOCK_OPEN) {",
        "replace": "    while let Some(open) = rest.find(\"<never-present>\") {",
        "test": ["cargo", "test", "-p", "ax-context", "--lib", "prompt_tests"],
    },
    {
        "id": "E-single-word-token",
        "file": "crates/ax-context/src/directory.rs",
        "find": 'r"\\b([A-Z][a-z0-9]+(?:[A-Z][a-z0-9]*)+|[a-z][a-z0-9]*(?:[A-Z][a-z0-9]*)+)\\b"',
        "replace": 'r"\\b([A-Z][a-z0-9]+(?:[A-Z][a-z0-9]*)*|[a-z][a-z0-9]*(?:[A-Z][a-z0-9]*)+)\\b"',
        "test": ["cargo", "test", "-p", "ax-context", "--lib", "prompt_tests"],
    },
    {
        "id": "E-hook-uses-raw-prompt",
        "file": "crates/ax-cli/src/commands/prompt_hook.rs",
        "find": "    let prompt = user_prompt_text(&prompt);",
        "replace": "    let prompt = prompt.clone();",
        "test": ["cargo", "test", "-p", "ax-cli", "--test", "prompt_hook_noise"],
    },
    {
        "id": "B-no-cap",
        "file": "crates/ax-core/src/stats_format.rs",
        "find": "    listed.truncate(PENDING_LISTED);",
        "replace": "",
        "test": ["cargo", "test", "-p", "ax-core", "--lib", "stats_format"],
    },
    {
        "id": "B-oldest-first",
        "file": "crates/ax-core/src/stats_format.rs",
        "find": "    listed.sort_by_key(|file| std::cmp::Reverse(file.last_seen_ms));",
        "replace": "    listed.sort_by_key(|file| file.last_seen_ms);",
        "test": ["cargo", "test", "-p", "ax-core", "--lib", "stats_format"],
    },
    {
        "id": "A-text-only-keeps-lean",
        "file": "crates/ax-mcp/src/server.rs",
        "find": "        StructuredMode::TextOnly => None,",
        "replace": "        StructuredMode::TextOnly => lean_structured(name, value),",
        "test": ["cargo", "test", "-p", "ax-mcp", "--lib", "policy_integration::a"],
    },
    {
        "id": "A-claude-not-recognized",
        "file": "crates/ax-mcp/src/server.rs",
        "find": 'const STRUCTURED_FIRST_CLIENTS: &[&str] = &["claude"];',
        "replace": 'const STRUCTURED_FIRST_CLIENTS: &[&str] = &["never-a-client"];',
        "test": ["cargo", "test", "-p", "ax-mcp", "--lib", "policy_integration::a"],
    },
    {
        "id": "A-error-reply-keeps-structured",
        "file": "crates/ax-mcp/src/server.rs",
        "find": "        if let Some(reply) = wrapped.as_object_mut() {\n            reply.remove(\"structuredContent\");",
        "replace": "        if let Some(reply) = wrapped.as_object_mut() {\n            let _ = &reply;",
        "test": ["cargo", "test", "-p", "ax-mcp", "--lib", "policy_integration::a"],
    },
    {
        "id": "A-full-env-ignored",
        "file": "crates/ax-mcp/src/server.rs",
        "find": "    if full {\n        return StructuredMode::Full;\n    }",
        "replace": "    let _ = full;",
        "test": ["cargo", "test", "-p", "ax-mcp", "--lib", "policy_integration::a"],
    },
    {
        "id": "A-remote-keeps-structured",
        "file": "crates/ax-mcp/src/remote/server.rs",
        "find": "        crate::server::strip_for_text_only_client(&mut engine.engine, &mut value);",
        "replace": "        let _ = &mut value;",
        "test": ["cargo", "test", "-p", "ax-mcp", "--test", "remote_mcp", "remote_claude"],
    },
    {
        "id": "A-remote-loses-its-ids",
        "file": "crates/ax-mcp/src/server.rs",
        "find": "        StructuredMode::TextOnly if engine.is_remote() => StructuredMode::Lean,",
        "replace": "",
        "test": ["cargo", "test", "-p", "ax-mcp", "--test", "remote_mcp", "remote_claude"],
    },
    {
        "id": "control",
        "expect": "survive",
        "file": "crates/ax-core/src/stats_format.rs",
        "find": "    listed.truncate(PENDING_LISTED);",
        "replace": "    listed.truncate(PENDING_LISTED); // control: no behavior change",
        "test": ["cargo", "test", "-p", "ax-core", "--lib", "stats_format"],
    },
]


def tests_fail(command):
    env = dict(os.environ, CARGO_TARGET_DIR=TARGET_DIR)
    done = subprocess.run(command, env=env, capture_output=True, text=True)
    output = done.stdout + done.stderr
    if done.returncode == 0:
        return False
    # A mutant that does not compile is not a kill by a test.
    if "test result: FAILED" not in output:
        raise RuntimeError(f"{' '.join(command)} failed without a test failure:\n{output[-600:]}")
    return True


def main():
    bad = 0
    for mutant in MUTANTS:
        path = mutant["file"]
        with open(path, encoding="utf-8") as handle:
            original = handle.read()
        if original.count(mutant["find"]) != 1:
            print(f"ERROR {mutant['id']}: search string occurs {original.count(mutant['find'])} times in {path}")
            bad += 1
            continue
        want_survive = mutant.get("expect") == "survive"
        try:
            with open(path, "w", encoding="utf-8") as handle:
                handle.write(original.replace(mutant["find"], mutant["replace"]))
            killed = tests_fail(mutant["test"])
        except Exception as error:  # fail closed
            print(f"ERROR {mutant['id']}: {error}")
            bad += 1
            continue
        finally:
            with open(path, "w", encoding="utf-8") as handle:
                handle.write(original)
        if want_survive:
            verdict = "ok (survived, as a control must)" if not killed else "BAD (control was killed)"
            bad += killed
        else:
            verdict = "killed" if killed else "SURVIVED"
            bad += not killed
        print(f"{mutant['id']:<32} {verdict}")
    real = sum(1 for m in MUTANTS if m.get("expect") != "survive")
    print(f"{real} mutants, 1 control, {bad} problem(s)")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
