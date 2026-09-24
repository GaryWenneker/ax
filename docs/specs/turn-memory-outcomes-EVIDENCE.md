# EVIDENCE: turn memories with outcomes, related turns in preflight, and "when did I change X"

**Spec:** `docs/specs/turn-memory-outcomes.md`, Revision 1 approved ("Approve Revision 1, build it on branch feat/turn-memory-outcomes") and Revision 1a approved ("Approve Revision 1a"). Revision 1b (found during implementation and review) is recorded in the spec and changes no behavior row.
**Tier:** 2. **Branch:** `feat/turn-memory-outcomes` (worktree `/Users/gary/io/ax-turn-outcomes`). **Source state:** `5d1ebda` (base `7bfe150`, the `main` this branch started from).
**Entry point:** `bash scripts/gauntlet-turn-outcomes.sh 7bfe150` reruns every layer below and exits nonzero on any failure. Pass the base explicitly: after the merge, `main` itself contains the change.
**Tools:** cargo 1.98.0, rustfmt 1.9.0-stable, node v26.8.2, sqlite3 3.54.0, macOS.

## Outcome

- **Outcome capture.** A turn memory now also stores the agent's final reply as `Outcome:` (redacted, capped at 20,000 characters with `[truncated]`).
  - Cursor delivers it through a new `afterAgentResponse` hook (`ax turn-hook response`); ax keeps the latest reply in the turn snapshot.
  - Claude Code delivers it as `last_assistant_message` on `Stop`.
  - `SubagentStop` no longer ends the turn.
- **Related turns in preflight.** Preflight adds `<ax_turn_history>` with at most 3 related past turns, newest first, within 1,200 characters. A turn is related when it changed an open file, or when recall finds it and it shares 2 or more words with the prompt.
- **History questions.** A question like "wanneer heb ik … aangepast" adds a hint to call `ax_history`.
- **`ax_history` / `ax history`.** New tool and CLI command that list dated turns (prompt, outcome, files, commits) and git commits for a file, symbol or topic. `--since` limits the dates; `--id` shows one turn's full outcome.
- **Retention.** Turns are kept 90 days. Before deletion they are appended to `.ax/backups/turn-memories-YYYY-MM-DD.jsonl`; if that write fails, nothing is deleted.

## Behavior to test mapping

| # | Test / check |
|---|---|
| O1 | `turn_hook::tests::o1_cursor_reply_becomes_the_outcome`, `parses_cursor_after_agent_response_and_claude_stop_replies`, `a_new_turn_starts_without_the_previous_reply`, layer 7 (two replies, the last one is stored) |
| O2 | `o2_claude_last_assistant_message_wins_over_the_snapshot`, layer 7 (`ax stop-hook` with `last_assistant_message`) |
| O3 | `o3_full_reply_is_kept_up_to_20000_characters` |
| O4 | `o4_secret_in_the_reply_never_reaches_disk_or_the_record`, layer 7 (token absent from the snapshot and the database) |
| O5 | `o5_no_reply_means_no_outcome_section`, `reply_without_a_snapshot_writes_nothing`, `outcome_is_read_back_from_the_body` |
| O6 | `o6_subagent_stop_does_not_end_the_turn`, layer 7 |
| R1 | `r1_turn_that_changed_an_open_file_is_related`, `r1_turn_sharing_two_words_with_the_prompt_is_related` (a single shared word is rejected), `tools::tests::r1_preflight_injects_turns_that_changed_an_open_file` (absolute, relative and symlinked paths) |
| R2 | `r2_nothing_related_gives_no_block` (including a non-turn memory), `disabled_turns_are_not_related`, `tools::tests::r2_preflight_without_related_turns_has_no_block` |
| R3 | `r3_at_most_three_newest_first_within_1200_characters`, `r3_prompt_matches_are_capped_at_three_newest_first`, `r3_newer_turns_that_fail_the_word_rule_do_not_crowd_out_file_matches` |
| H1 | `h1_path_lists_the_turn_and_the_commit_newest_first`, `h1_symbol_name_resolves_to_its_file`, `h1_free_text_uses_recall_on_turns`, `h1_free_text_needs_a_shared_word_beyond_recall`, `h1_path_suffix_finds_a_turn_on_an_unindexed_file`, `history_outside_git_still_lists_turns`, `tools::tests::ax_history_lists_turns_and_gives_one_in_full_by_id`, layer 7 (`ax history main.rs`) |
| H2 | `h2_history_questions_are_recognised`, `tools::tests::h2_history_question_points_the_agent_to_ax_history` |
| H3 | `h3_since_drops_older_entries`, `h3_since_drops_older_turns`, `h3_since_in_the_future_drops_a_commit_made_just_now`, `tools::tests::ax_history_since_filters_and_rejects_bad_dates`, layer 7 (`--since 2999-01-01`, bad date rejected) |
| H4 | `h4_listing_cuts_the_outcome_and_the_id_gives_it_in_full`, `history_entry_is_only_for_turn_memories`, layer 7 (`ax history --id`) |
| P1 | `p1_p2_turn_older_than_retention_is_backed_up_then_deleted`, `backups_append_to_the_same_day_file`, `the_backup_holds_only_turn_memories`, `retention_is_90_days`, layer 7 (91-day-old turn pruned at the next turn end, found in the backup) |
| P2 | `p1_p2_turn_older_than_retention_is_backed_up_then_deleted` (89 days kept), `nothing_to_prune_writes_no_backup_file` |
| P3 | `p3_failed_backup_deletes_nothing` |
| 1b: quoted marker | `a_prompt_quoting_the_outcome_marker_is_not_an_outcome` (red before the fix) |
| Catalog | `tools::tests::ax_history_is_in_the_default_catalog`, `catalog_payload_size` |
| Installer | `hooks::tests` (the `afterAgentResponse` entry shape, idempotence, removal), layer 7 (install twice gives one entry under `afterAgentResponse`; uninstall removes it) |
| Must not change: no-change turns write nothing | existing `t2_turn_without_changes_is_not_recorded` passes; mutants T1–T21 of the per-turn script all killed |
| Must not change: turns never exported | existing `export_never_writes_turn_memories` passes; per-turn mutant M3 killed |
| Must not change: hooks never block or print | existing `main::turn_hook_is_always_quiet`; layer 7 runs every `turn-hook` call through `quiet` (no stdout, no stderr, exit 0) |
| Gauntlet leaves the user's agent hooks alone | layer 7 of both gauntlets runs with a temp `HOME` and fails if `~/.cursor/hooks.json` or `~/.claude/settings.json` changed |
| Must not change: other memory kinds and preflight blocks | `prune_removes_only_old_turn_memories`, `the_backup_holds_only_turn_memories`, existing preflight tests pass unchanged |

## Gauntlet (one run on `5d1ebda`)

| Layer | Result |
|---|---|
| 1. Targeted tests (`ax-cli`, `ax-memory`, `ax-installer`, `ax-mcp` lib + `catalog_payload_size`) | 235 passed, 0 failed; turn hook and memory tests green 5/5 repeats |
| 2. Workspace suite | 809 passed, 4 failed; all 4 in the baseline: `bootstrap::tests::legacy_prefix_from_workspace_folder`, `bootstrap::tests::resolves_placeholder_to_folder_name`, `savings::tests::cursor_transcript_path_filter` (as on `main`), and `cycles_api_path_handlers_work` (needs the repo's own `.ax/`, which a fresh worktree lacks) |
| 3. Clippy on changed crates | 0 findings on changed lines |
| 4. rustfmt | new files 0 hunks; every changed file equal to `7bfe150` (for example `tools.rs` 31, `main.rs` 50) |
| 5. Release build + CLI docs | all 131 commands documented |
| 6. Mutants | `scripts/mutants-turn-outcomes.sh` 44/44 killed (O1–O9, P1–P8, R1–R10, H1–H3, H5–H11, C1–C6, I1); `scripts/mutants-per-turn-memory.sh` 33/33 killed |
| 7. Real execution with the release binary | O1 O2 O4 O6 H1 H3 H4 P1 and the installer verified in a temp repo with a temp `HOME`; the user's hook files unchanged |

**Negative controls** (each proven once, then restored):

- **Layer 7.**
  - A binary built with the redaction removed from `outcome_text` failed with `O4: snapshot holds the token`.
  - With the `HOME` isolation removed (run against a synthetic `HOME`), the layer failed with `real execution changed ~/.cursor/hooks.json or ~/.claude/settings.json`.
  - An earlier, stricter version of the backup check failed on a real fresh project (see below).
- **Layer 4.** It failed on real drift in `tests/history.rs` before that file was formatted.
- **Layer 6.** It fails when a mutant pattern does not apply (H3 after the async change) and when a mutant survives (H6, P7, R4, see below).

## What failed along the way

- **Mutant H6 survived the first mutation run.** Nothing tested that a turn recall finds only through character trigrams, sharing no whole word with the query, is left out. I added `h1_free_text_needs_a_shared_word_beyond_recall`; it failed under the mutant.
- **The first restore in the mutant script aborted.** `git checkout -- file` runs ax's `post-checkout` hook, and that hook exits nonzero in a tree without `.ax/`. Both mutant scripts now restore with hooks disabled for that one command.
- **The backup check in layer 7 first required `.ax/backups` to be ignored by git.** `ax init` does not ignore `.ax/` at all: `ax.db` itself is untracked in a fresh project. The check now requires that the backup is never more exposed than `ax.db`, and the finding is reported separately (spec Revision 1b point 5).
- **After the review fixes, the final run failed at layer 6.**
  - P7 survived: nothing checked that the backup holds only turns.
  - R4 survived: the cap of 3 was only tested with file matches, which the SQL limit already enforces.
  - H3 no longer applied, because the line gained `.await`.

  I added the two tests, both confirmed red under their mutants, and updated the H3 pattern. The next run passed on `2f8518d`.
- **My gauntlets rewrote the user's global agent hooks.** Layer 7 of both gauntlets ran `ax init` with the real `HOME`. `ax init` writes `~/.cursor/hooks.json` and `~/.claude/settings.json` with the path of the running binary, so those hooks pointed at `/tmp/ax-gauntlet-*/bin/ax`, which only works until `/tmp` is cleaned. I found it while installing: the Claude `turn-hook start` still pointed at the per-turn gauntlet's temp dir. I repaired both files to `~/.local/bin/ax` (only the command paths changed), and layer 7 now uses a temp `HOME` plus the check above.
- **With `HOME` isolated, H3 failed in every real run.** git rejects `--since` timestamps from 2100-01-01 on and falls back to about "now", so `--since 2999-01-01` listed a commit made seconds earlier. With the real `HOME`, the ax git hooks did not run in the test repo, and the timing happened to pass. I wrote the RED test `h3_since_in_the_future_drops_a_commit_made_just_now` and added a `since` filter in `ax` (spec Revision 1b point 7).
  - Hand-written mutant H4 (drop git's `--since`) is no longer a bug: after the filter, git's `--since` only shortens the walk. It was replaced by H11 (drop the filter).
  - H11 first survived one run, because the test's commit only slipped past git within the same second. The commit is now dated an hour ahead, and the kill was 3/3 red.

## Review rounds

| Round | Skills (status) | Findings | Fixed |
|---|---|---|---|
| 1 | `old-coder` usable, `rust-review` usable | 1 high, 3 medium, 2 low: `git log` blocked the async runtime; preflight loaded every turn body; per-row delete without a transaction; duplicated LIKE escaping; blocking `canonicalize` in preflight; a prompt quoting `"\n\nOutcome: "` posed as the outcome | `5c09558` (plus the RED test for the marker) |
| 2 | same | 0 | — |
| 3 | same, after `2f8518d` (two tests, one mutant pattern) | 0 | — |
| 4 | same, after `5d1ebda` (the `since` filter, `HOME` isolation, one test) | 1 low: `user_hooks` died silently under `pipefail` when `~/.claude/settings.json` does not exist | fixed before `d5e1386` |
| 5 | same | 0 | — |

## Known limits

- **Cursor timing.** If Cursor ran `stop` before the last `afterAgentResponse` hook finished, the outcome is the previous message.
- **Claude re-Stop.** When a Claude `Stop` is blocked and re-invoked, the first memory of that turn is kept (`INSERT OR IGNORE`), including its outcome.
- **Redaction coverage.** Redaction covers the patterns in `redact_secrets` (API keys, tokens, passwords, long hex and base64 runs), not every secret a reply can contain.
- **MCP path not run through the server.** The MCP side (preflight block, hint, `ax_history`) is covered by `ToolHandler` tests against a real initialized database, not by driving `ax serve --mcp`, which attaches to or spawns a background daemon.
- **Unmutated code.**
  - The `response` phase dispatch in `turn_hook::run` and the CLI `ax history` wiring are covered only by layer 7.
  - The no-snapshot `?` in `record_response` is not mutated.
  - The LIKE escaping is not mutated; an unescaped wildcard would match more, not less.
- **Pruning cap.** Pruning handles at most 10,000 expired turns per turn end; the rest go at the next one.
- **Backups and git.** The backup files are not ignored by git in a fresh project, exactly like `ax.db` (a pre-existing `ax init` gap).
- **Spec approval.** Revision 1b was not approved separately. It changes no behavior row: it records implementation details and review fixes.
