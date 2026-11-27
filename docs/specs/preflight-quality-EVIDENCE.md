# EVIDENCE — Preflight and prompt-hook quality

- Spec: `docs/specs/preflight-quality.md` (revisions R1–R5)
- Spec approval: **not obtained (autonomous run).** The owner delegated the decisions and did
  not approve the text. Confidence is lower than for an approved spec; the spec is the artifact
  to review.
- Review loop: **not run.** No independent review round was done on this change.
- Source state of the final run: commit `4bc6509295e60c8effc4632619a946e08bf718c5`, 0 uncommitted files.
- Toolchain: rustc 1.100.0 (0ef1bc386 2026-11-09), Python 3.15.2, macOS arm64.
- Rerun everything: `bash tools/gauntlet-preflight-quality.sh` from the repository root.

All numbers below come from that one run, made after the last code edit.

## Behavior → test

| Scenario | Test |
|---|---|
| A1 Claude client gets bodies as text only | `server::policy_integration::a1_claude_client_gets_the_bodies_as_text_only` |
| A1 other clients keep the lean payload | `…::a1_other_clients_keep_the_lean_payload`; e2e row "lean client keeps structuredContent" |
| A2 session id and directive questions in the text | `…::a2_text_only_preflight_carries_session_and_directive_questions` |
| A3 every tool text-only for a Claude client, errors included | `…::a3_error_replies_are_text_only_for_claude_and_structured_for_others`; e2e: every row checks "no structuredContent" |
| A4 `AX_MCP_FULL` wins; `AX_MCP_STRUCTURED` override | `…::a_structured_mode_follows_env_then_client`, `…::a_structured_payload_per_mode` |
| A5 Claude client over the remote server keeps its chats and caches | `remote_mcp::remote_claude_client_gets_text_only_replies_and_keeps_its_chats_and_caches` |
| B1 at most 20 pending paths, newest first, total shown | `stats_format::tests::b1_at_most_twenty_pending_paths_are_listed_newest_first` |
| B2 short list unchanged | `stats_format::tests::b2_a_short_pending_list_is_unchanged` |
| C1–C5 skill triggers | `matcher::tests::c1_…` to `c5_…` |
| D1–D4 directive detection | `capture::tests::d1_…`, `d2_…` (two tests), `d3_…`, `d4_…` |
| E1–E3 prompt text and code tokens | `directory::prompt_tests::e1_…`, `e2_…`, `e3_…` |
| E1, E2, E4 through the real binary | `ax-cli/tests/prompt_hook_noise.rs`; e2e rows "hook: …" (full and slim) |
| F1 every MCP tool answers a realistic call | `tools/ax_agent_e2e.py`: 39 tools listed, 0 without a planned call |
| F2 preflight → explore → node → guard → remember → recall → session → preflight | same script: rows "preflight (first turn)" to "preflight (second turn) shows the note…" |
| G1 slim build passes the same run | gauntlet step 7 |

## Must-NOT clauses

| Clause | Evidence |
|---|---|
| N1 no rule or skill file edited | `git diff 90b244c9..HEAD --stat -- .agents` is empty |
| N2 alwaysApply rules still delivered | e2e "preflight (first turn)" requires `MARKER-RULE-CENTS`; nothing in budget code changed |
| N3 `AX_MCP_FULL=1` unchanged | `a_structured_mode_follows_env_then_client`, mutant `A-full-env-ignored` killed |
| N4' remote: lean kept internally, dropped last for text-only clients | test A5; mutants `A-remote-keeps-structured`, `A-remote-loses-its-ids` killed; the 14 existing remote tests pass unedited |
| N5 text-authoritative tools unchanged | `lean_structured` untouched; its 5 existing unit tests pass unedited |
| N6 rule trigger matching unchanged | `score_rule` untouched; `single_letter_trigger_needs_a_whole_word` passes unedited |
| N7 CLI `--json` keeps its fields | `pendingFilesTotal` added, nothing removed (diff of `status.rs`, `tools.rs`) — by inspection, no test |
| N8 no existing test weakened | no existing assertion was edited; all additions are new tests |
| N9 nothing under `.ax/` touched, no migration | no file under `crates/ax-db/src/migrations.rs` in the diff |

## Gauntlet

| Layer | Command | Result |
|---|---|---|
| Tests | `cargo test -p ax-policy -p ax-context -p ax-core -p ax-mcp -p ax-cli` with an empty `HOME` | 31 suites ok, 816 tests passed, 0 failed |
| Lint | `cargo clippy` (same crates) `--all-targets -- -D warnings` | clean |
| Format | `rustfmt --check` on the 8 Rust files this spec changed | clean |
| Mutation | `tools/preflight_quality_mutants.py` | 18 of 18 killed; the no-op control survived, as it must |
| Release build | `cargo build --release -p ax-cli` | ok, 165,673,728 bytes |
| End-to-end, full build | `tools/ax_agent_e2e.py target-dev/release/ax` | 47 passed, 0 failed, 2 known gaps |
| End-to-end, slim build | `cargo build --profile release-slim -p ax-cli --no-default-features --features "minimal-grammars mimalloc"`, then the same script | ok, 31,409,552 bytes; 47 passed, 0 failed, 2 known gaps |
| Coverage | skipped: no coverage tool is configured in this repository |
| Property tests | skipped: none written |
| Supply chain | no production dependency added; `tempfile` added as a dev-dependency of `ax-extraction` in the earlier commit `2ee31faa` |

## What failed on the way, and how it was resolved

1. `clippy` rejected `sort_by` in `listed_pending`; changed to `sort_by_key` with `Reverse`.
2. `ax-mcp/tests/ice_preflight.rs::preflight_returns_exact_indexed_manifest_line` fails when the
   tests read this machine's `~/.ax`: the global policy alone is 12,856 tokens against the
   test's 12,000 budget, so the source block is dropped. It passes with an empty `HOME`
   (4 of 4, 1.2 s). The gauntlet runs test binaries with an empty `HOME`. The test was not edited.
   This was not run against the commit before this work, so "pre-existing" is inferred, not measured.
3. The first gauntlet runs failed in the build step: npm cannot build the web UI under an empty
   `HOME`. Builds now use the real `HOME`; only the test binaries get the empty one.
4. The disk filled up during a build (134 MB free). I deleted my own build cache in this worktree
   and my scratch copies; nothing else.
5. R5: the first version of the text-only mode broke cache ownership on the remote server for
   Claude clients (`ax_expand` → 403). Found by reading the remote server for the slim request,
   reproduced with a failing test, then fixed.
6. The mutation runner stopped once because a search string matched two places after that fix.
   It failed closed as designed; the mutant now names one place.

## Known gaps the end-to-end run reports on every run (not fixed)

- **Calls inside macro arguments get no graph edge.** `println!("{}", invoice_total(items))`
  does not make `print_invoice` a caller of `invoice_total`. The same holds for `assert!` and
  `format!`. Callers, impact and affected-tests answers for Rust are incomplete because of it.
- **`ax_history` finds a commit by file path, not by the words of its subject.**
  Query `src/billing.rs` returns the commit; query `invoice billing` returns nothing.

## Observed, out of scope, not changed

- A new project made by `ax init` gets a first preflight of about 54,000 characters: the seeded
  rules include ax-repository rules (`docs-with-features`, `codegraph-parity` with Windows
  paths, `dutch-pr-comments`).
- Only two contextual skills are delivered per preflight. A project's own skill with a matching
  trigger lost to two seeded review skills for the prompt "review the invoice total code".
- On this machine every preflight carries "AX BUDGET HARD LIMIT … % of monthly budget consumed".
- Sentence-shaped one-off task constraints ("Do NOT delete existing rows.") are still proposed
  as rules; stated in the spec.
- Not verified live: the running MCP servers on this machine still use the old build. The
  text-only mode was verified through the stdio and remote test harnesses, not in a live
  Claude Code session.
