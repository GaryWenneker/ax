# EVIDENCE: automatic per-turn memory (v5.1.0)

**Spec:** `docs/specs/per-turn-memory.md`, Revision 1 approved ("Approve Revision 1 (build it on a branch after v5.0.3, ships in v5.1.0)"). Revision 1a (hook input fields) and Revision 1b (ax runtime files are not turn changes) are recorded in the spec; neither changes an approved behavior.
**Tier:** 2. **Branch:** `feat/per-turn-memory`. **Source state:** `184fcce` (base `main` `faf1a9f`).
**Entry point:** `bash scripts/gauntlet-per-turn-memory.sh` reruns every layer below and exits nonzero on any failure.
**Tools:** cargo 1.98.0, rustfmt 1.9.0-stable, node v26.8.2, sqlite3 3.54.0, macOS.

## Outcome

After every agent turn that changed files or made a commit, ax saves a `turn` memory: the prompt (redacted, 300 characters), the files changed and the commits made. The turn is bracketed by `ax turn-hook start` (Cursor `beforeSubmitPrompt`, Claude Code `UserPromptSubmit`) and a turn end (`ax turn-hook end` for Cursor, inside `ax stop-hook` for Claude Code). Turn memories are found by `ax_recall` / `ax recall`, never injected by preflight, never exported, and deleted after 30 days. `ax install` adds the hooks; `ax.json` `memory.perTurn: false` or `AX_NO_STOP_HOOK=1` switches them off.

## Behavior to test mapping

| # | Test / check |
|---|---|
| T1 | `turn_hook::tests::t1_file_edited_during_the_turn_is_recorded`, `end_turn_saves_the_memory_and_prunes_old_turns`, `tests/turns.rs::saved_turn_is_a_local_turn_memory`, layer 7 |
| T2 | `t2_turn_without_changes_is_not_recorded`, layer 7 |
| T3 | `t3_file_dirty_before_the_turn_and_untouched_is_not_listed`, `file_dirty_before_the_turn_and_edited_again_is_listed` |
| T4 | `t4_commit_during_the_turn_lists_subject_and_files`, `new_file_in_a_new_directory_and_deleted_file_are_listed` |
| T5 | `t5_same_turn_end_twice_gives_the_same_id`, `t5_different_turns_get_different_ids`, `saving_the_same_turn_twice_keeps_one_memory`, layer 7 |
| T6 | `t6_no_snapshot_means_no_record` |
| T7 | `t7_per_turn_false_in_ax_json_disables_both_hooks`, `per_turn_true_or_other_ax_json_keeps_it_on`, `only_ax_no_stop_hook_1_disables`, layer 7 |
| T8 | `prune_removes_only_old_turn_memories`, `end_turn_saves_the_memory_and_prunes_old_turns` |
| T9 | `tools::tests::memory_titles_skip_turn_memories`, `memory_titles_are_empty_when_only_turns_exist`, `preflight_recall_skips_turn_memories_but_recall_finds_them`, `preflight_recall_still_returns_other_kinds_when_turns_outrank_them` |
| T10 | `preflight_recall_skips_turn_memories_but_recall_finds_them`, layer 7 (`ax recall` finds the turn) |
| T11 | `export_never_writes_turn_memories`, layer 7 (`ax memory export` has no turn) |
| T12 | `t12_secret_in_the_prompt_never_reaches_disk_or_the_record`, `redacts_api_keys_and_tokens`, `redacts_password_values`, `redacts_long_hex_and_base64_runs`, `leaves_ordinary_text_alone`, `prompt_is_cut_to_300_characters_and_title_to_80`, layer 7 |
| T13 | `hooks::tests::{cursor_turn_hook_entry_shape, cursor_turn_hooks_are_idempotent_and_keep_user_entries, turn_hook_and_read_guard_removal_leave_each_other_alone, turn_hook_files_install_and_uninstall}`, `targets::tests::claude_hooks_include_turn_start_once_and_uninstall_keeps_user_hooks`, layer 7 (install, reinstall, uninstall with `HOME` isolated) |
| T14 | `t14_directory_without_git_or_ax_writes_nothing`, `input_without_a_conversation_is_ignored`, `main::turn_hook_is_always_quiet`, layer 7 (no stdout, no stderr, exit 0) |
| T15 | layer 7 (Claude `Stop` through `ax stop-hook` writes the memory); the policy block path itself was not re-executed, see limits |
| 1b | `ax_runtime_files_are_not_changes_but_shared_ax_files_are`, `snapshot_file_itself_is_not_a_change`, `snapshots_are_ignored_by_git` |
| Hook input | `parses_cursor_hook_input`, `parses_claude_hook_input` (Revision 1a fields) |
| Must not change: Claude `Stop` behavior | existing `stop_hook` tests pass unchanged; the diff adds one line before the `stop_hook_active` check |
| Must not change: preflight without turns | existing preflight and memory title tests pass unchanged |
| Must not change: other memory kinds | `preflight_recall_still_returns_other_kinds_when_turns_outrank_them`, `prune_removes_only_old_turn_memories`, existing `ax-memory` tests |

## Gauntlet (one run on `184fcce`)

| Layer | Result |
|---|---|
| 1. Targeted tests (`ax-memory`, `ax-mcp`, `ax-installer`, `ax-cli`) | 194 passed, 0 failed; turn hook tests green 5/5 repeats |
| 2. Workspace suite | 766 passed, 4 failed; all 4 in the known baseline (`bootstrap::tests::legacy_prefix_from_workspace_folder`, `bootstrap::tests::resolves_placeholder_to_folder_name`, `savings::tests::cursor_transcript_path_filter`, and the known flake `seed::tests::seeds_sonar_project_key_from_folder_name`) |
| 3. Clippy on changed crates | 0 findings on changed lines |
| 4. rustfmt | new files 0 hunks; no changed file grew (`targets.rs` 29, was 31; the rest equal to `main`) |
| 5. Release build + CLI docs | all 130 commands documented |
| 6. Mutants (`scripts/mutants-per-turn-memory.sh`) | 33/33 killed (T1–T21, M1–M7, P1, I1–I4) |
| 7. Real execution with the release binary | T1 T2 T5 T7 T10 T11 T12 T13 T14 T15 verified; turn end took 65 ms |

Negative controls (each proven once, then restored from git):

- Layer 3: a planted `usize.clone()` on a changed line made it fail.
- Layer 4: it failed on real formatting drift in the new files before they were formatted.
- Layer 6: the script fails when a mutant pattern does not apply (T15 after rustfmt) and when a mutant survives (T12, see below).
- Layer 7: removing the `end_from_input` line from `stop_hook.rs` made it fail with the T15 message; removing the `turn-hook` rule from `log_directive` made it fail on an INFO line on stderr.

## What failed along the way

- A whitespace-boundary test failed because `turn_body` trimmed the prompt. Fixed in the implementation; the test was not changed.
- Mutants T8, T14 and T16 survived the first mutation run. I strengthened the tests and all three are now killed.
- Layer 7 first used `( … ) || fail`, which makes bash ignore `set -e` inside the subshell (fail-open). It now runs the subshell with `set -e` and checks its exit code. `scripts/gauntlet-git-hooks.sh` (v5.0.3) has the same construct; its explicit checks still fail on the tested paths, but it should get the same fix.
- The hook printed ax INFO logs on stderr. `log_directive` now always uses `ax=warn` for `turn-hook`.
- Final run 1 failed: in a project that does not gitignore `.ax/`, `ax.db` and its WAL files counted as turn changes. That was a real bug. I wrote a RED test, fixed it, and recorded it as spec Revision 1b.
- Final run 2 failed at layer 6. T12 survived because nothing tested that the snapshots stay out of git (`snapshots_are_ignored_by_git` added, confirmed red under the mutant). T15 did not apply because rustfmt had split the line (pattern updated).

## Review rounds

| Round | Skills (status) | Findings | Fixed |
|---|---|---|---|
| 1 | `old-coder` usable, `rust-review` usable | 4 minor: git work blocked the async runtime (moved to `spawn_blocking`); ignored errors without a reason; no doc on `TurnPhase`; no docs on the turn constants | `f09fb8f` |
| 2 | same | 0 | — |
| 3 | same, after Revision 1b | 0 | — |
| 4 | same, after `184fcce` (one test, one mutant pattern) | 0 | — |

## Known limits

- The T15 policy-block path (a CRITICAL violation on Claude `Stop`) was not re-executed. The diff adds one line before it and the existing stop-hook tests pass.
- In Claude Code, if a `Stop` is blocked and re-invoked, the first memory of that turn is kept (`INSERT OR IGNORE`).
- Monorepo roots without `.ax/` capture nothing.
- Renames reported only in git's worktree column (` R`) are not handled specially.
- Edits made outside the agent during a turn are counted as part of that turn.
- The memory records what changed, not why. The why still needs `ax_remember`.
- Windows was not executed.
- Suite order randomization was not run (cargo test has no built-in shuffle). The baseline includes two known flaky tests.
