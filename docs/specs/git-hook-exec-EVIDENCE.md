# EVIDENCE: git hooks that actually run, quiet hooks (v5.0.3)

**Spec:** `docs/specs/git-hook-exec.md`, approved ("Approve, build it and release v5.0.3"), Revision 1 approved ("Approve Revision 1").
**Tier:** 2. **Branch:** `fix/git-hook-exec`. **Source state:** `4190ce3` (base `main` `080c6a2`).
**Entry point:** `bash scripts/gauntlet-git-hooks.sh` reruns every layer below and exits nonzero on any failure.
**Tools:** cargo 1.98.0, rustfmt 1.9.0-stable, node v26.8.2, macOS.

## Outcome

Git hooks written by ax now start with `#!/bin/sh` and are executable, so git runs them again and every commit is captured as a memory. Broken hooks are repaired on install and when the MCP server starts. The hooks run the quality gate with `--quiet`: a passing gate prints nothing, a failing one prints one line, and `--quiet` hides ax INFO logs.

## Behavior to test mapping

| # | Test / check |
|---|---|
| H1 | `git_hooks::tests::new_hook_gets_shebang_and_exec_bit` |
| H2 | `broken_ax_hook_is_repaired_on_install` |
| H3 | `user_shebang_and_lines_are_kept` |
| H4 | `complete_hook_is_left_untouched` (bytes, mtime and mode) |
| H5 | `all_lines_but_no_shebang_is_repaired` |
| H6 | `repair_fixes_broken_ax_hook` |
| H7 | `repair_ignores_hooks_without_ax_lines` |
| H8 | `missing_hooks_dir_is_a_no_op` |
| H9 | `server::hook_repair::startup_repairs_broken_ax_hook`, `startup_repair_never_panics_on_unreadable_repo` |
| H10 | gauntlet layer 7 (temp repo, fresh binary, `ax init`, commit, `ax recall` finds `[git] feat: gauntlet quiet hook`) |
| Q1 | `ship::tests::passing_gate_prints_nothing`, plus layer 7 (commit printed only git's summary) |
| Q2 | `failing_gate_lists_failed_steps`, `failing_gate_without_failed_step_still_says_so` (unit only, see limits) |
| Q3 | `log_directive_tests::{quiet_hides_info, default_keeps_info, quiet_as_a_value_does_not_count}`, plus layer 7 |
| Q4 | `new_hooks_run_the_gate_quietly` |
| Q5 | `legacy_ship_line_is_replaced_in_place_on_install`, `repair_replaces_legacy_ship_line` |
| Q6 | `user_ship_variant_is_left_alone`, `user_ship_variant_next_to_legacy_line_is_left_alone` |
| Must not change: line set and order | existing `hook_lines` tests pass unchanged |
| Must not change: remove only ax lines | `remove_strips_every_ax_line_and_keeps_the_rest` |
| Must not change: Windows no chmod | `#[cfg(unix)]` on `ensure_executable`; not executed here (no Windows run) |
| Must not change: non-ax hooks untouched | H7 test |

## Gauntlet (one run on `4190ce3`)

| Layer | Result |
|---|---|
| 1. Targeted tests `ax-sync`, `ax-mcp`, `ax-cli` | 137 passed, 0 failed |
| 2. Workspace suite | 727 passed, 3 failed; all 3 in the known baseline (`bootstrap::tests::legacy_prefix_from_workspace_folder`, `bootstrap::tests::resolves_placeholder_to_folder_name`, `savings::tests::cursor_transcript_path_filter`) |
| 3. Clippy on changed crates | 0 findings on changed lines |
| 4. rustfmt | no new drift: `git_hooks.rs` 0 (was 0), `server.rs` 6 (was 7), `ship.rs` 2 (was 2), `main.rs` 50 (was 51) |
| 5. Release build + CLI docs | all 130 commands documented |
| 6. Mutants (`scripts/mutants-git-hooks.sh`) | 16/16 killed |
| 7. Real execution | hooks executable with shebang and quiet ship line; commit printed only git's summary; git memory captured |

Negative controls (each layer's failure path proven once, then restored from git):

- Layer 2: an extra `panic!` test made the run fail with "new test failures: gauntlet_control_new_failure".
- Layer 3: `if line.len() == 0` on a changed line made it fail (`clippy::len_zero`).
- Layer 4: extra spaces in a `const` made it fail ("rustfmt drift grew").
- Layer 6: M9 survived in an earlier run and the script reported 15/16. That led to the Q6 test next to a legacy line.
- Layer 7: making `log_directive` always return `ax=info` made it fail on an INFO line on stderr.

A noisy-user-hook control for layer 7 was blocked by the approval tool and replaced by the log control above.

## What failed along the way

- Mutant M9 (legacy match by `contains`) survived. I added `user_ship_variant_next_to_legacy_line_is_left_alone` and it is now killed.
- The rustfmt layer first counted hunks from submodules (rustfmt follows `mod` declarations), so a clean tree failed. It now counts only the file's own hunks.
- The compile check first matched cargo's `error: test failed` lines. It now matches only real compile errors.
- `seed::tests::seeds_sonar_project_key_from_folder_name` (`ax-remote`, untouched by this branch) failed once in a full run. It passed 6 out of 6 times alone. It is listed as a known flake next to `transcript_import_does_not_wipe_state_tokens`.

## Review rounds

| Round | Skills (status) | Findings | Fixed |
|---|---|---|---|
| 1 | `old-coder` usable, `rust-review` usable | 2 minor: R1-1 missing blank line before the test module in `server.rs`; R1-2 `cli.md` said `--quiet` "always exits 0" | `dd7aea9` |
| 2 | same | 0 | — |

## Known limits

- The `if quiet` branch inside `ship::run` has no unit test. Layer 7 covers the passing path. The failing path (Q2) is covered only by the `quiet_failure_line` unit tests, because there is no cheap way to force a failing gate in a temp repo.
- A hook runs with the PATH of the program that commits. GUI git clients without `ax` on PATH still skip the ax lines.
- Windows was not executed.
- Suite order randomization was not run (cargo test has no built-in shuffle). The baseline list includes two known flaky tests.
- Clippy's pre-existing `too_many_arguments` on `ship::run` went from 10 to 11 arguments.
