#!/usr/bin/env python3
"""Manual mutation runner for docs/specs/macro-calls-history-subject.md.

Uses the runner in preflight_quality_mutants.py with this spec's mutants: each
one reintroduces a bug the spec forbids and its test command must fail. The
`control` entry changes no behavior and must survive.

Usage: tools/graph_history_mutants.py   (run from the repository root)
"""

import sys

import preflight_quality_mutants as runner

MACRO_TESTS = ["cargo", "test", "-p", "ax-extraction", "--lib", "macro_call_tests"]
HISTORY_TESTS = ["cargo", "test", "-p", "ax-memory", "--test", "history"]
REFS = "crates/ax-extraction/src/languages/refs.rs"
HISTORY = "crates/ax-memory/src/history.rs"

runner.MUTANTS = [
    {
        "id": "M-macros-not-walked",
        "file": "crates/ax-extraction/src/languages/rust.rs",
        "find": "        append_rust_macro_call_refs(&mut result, tree, &ctx);",
        "replace": "",
        "test": MACRO_TESTS,
    },
    {
        "id": "M-no-nesting",
        "file": REFS,
        "find": "                if tokens.kind() == \"token_tree\" {",
        "replace": "                if tokens == args {",
        "test": MACRO_TESTS,
    },
    {
        "id": "M-any-bracket",
        "file": REFS,
        "find": "if ctx.source.get(args.start_byte()) != Some(&b'(') {",
        "replace": "if false {",
        "test": MACRO_TESTS,
    },
    {
        "id": "M-any-token-before",
        "file": REFS,
        "find": "token(i - 1).filter(|n| n.kind() == \"identifier\")",
        "replace": "token(i - 1).filter(|_| true)",
        "test": MACRO_TESTS,
    },
    {
        "id": "M-definition-is-a-call",
        "file": REFS,
        "find": "if before(2).is_some_and(|n| n.kind() == \"fn\") {",
        "replace": "if false {",
        "test": MACRO_TESTS,
    },
    {
        "id": "H-topic-skips-commits",
        "file": HISTORY,
        "find": "        found.extend(git_commits(root, CommitFilter::Words(&words), query.since_ms, limit).await);\n",
        "replace": "",
        "test": HISTORY_TESTS,
    },
    {
        "id": "H-any-word",
        "file": HISTORY,
        "find": '"--regexp-ignore-case", "--all-match"]',
        "replace": '"--regexp-ignore-case"]',
        "test": HISTORY_TESTS,
    },
    {
        "id": "H-case-sensitive",
        "file": HISTORY,
        "find": '["--fixed-strings", "--regexp-ignore-case", "--all-match"]',
        "replace": '["--fixed-strings", "--all-match"]',
        "test": HISTORY_TESTS,
    },
    {
        "id": "H-no-words-lists-all",
        "file": HISTORY,
        "find": "            if words.is_empty() {\n                return Vec::new();\n            }\n",
        "replace": "",
        "test": HISTORY_TESTS,
    },
    {
        "id": "H-since-ignored",
        "file": HISTORY,
        "find": "CommitFilter::Words(&words), query.since_ms, limit)",
        "replace": "CommitFilter::Words(&words), None, limit)",
        "test": HISTORY_TESTS,
    },
    {
        "id": "control",
        "expect": "survive",
        "file": HISTORY,
        "find": "            if words.is_empty() {",
        "replace": "            if words.is_empty() { // control: no behavior change",
        "test": HISTORY_TESTS,
    },
]

if __name__ == "__main__":
    sys.exit(runner.main())
