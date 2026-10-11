# EVIDENCE: install and daemon hygiene

**Spec:** `docs/specs/install-daemon-hygiene.md`, approved ("ja", commit `c6fb85d`). No revision: implementation and review changed no behavior row.
**Tier:** 3 for the daemon and proxy parts (D, E), 2 for the rest.
**Branch:** `fix/install-daemon-hygiene` (worktree `/Users/gary/io/ax-hygiene`). **Source state:** `7cb2b5c` (base `c6ed438`). The EVIDENCE commit that follows adds only this file.
**Entry point:** `bash scripts/gauntlet-install-daemon-hygiene.sh c6ed438` reruns every layer below and exits nonzero on any failure. Pass the base explicitly: after the merge, `main` itself contains the change.
**Tools:** cargo 1.98.0, rustfmt 1.9.0-stable, node v26.8.2, git 2.54.0, macOS.
**New dependencies:** none. `tempfile` became a dev-dependency of `ax-installer`; it was already in the workspace lockfile.

## Outcome

- **Git hooks in a checkout without `.ax/`.** `ax sync --quiet` and `ax ship --evaluate --quiet` are a silent no-op there. A fresh worktree no longer reports a failed hook on every checkout. Without `--quiet` both still report `project not initialized`.
- **`ax init` writes `.ax/.gitignore`.** Only `.ax/.gitignore` and `.ax/policy/` stay committable. `ax.db`, the daemon files, logs, backups and turn snapshots are ignored. Existing user lines are kept.
- **The installer writes the `ax` on `PATH`.** The path is written as found, so a shim stays a shim. The report names both paths when they differ. A stale Claude hook path is repaired in place.
- **The MCP proxy survives a daemon restart.**
  - A request left unanswered gets `-32000 ax daemon restarted; retry the call`.
  - Requests sent during the reconnect arrive in order.
  - After 15 s without a daemon, the proxy exits 1 with the reason on stderr.
  - A daemon the proxy started is reaped, so it leaves no zombie behind.
- **Daemons follow their binary.**
  - `daemon.json` and the hello line carry the binary's path, size and mtime.
  - A newer proxy restarts an older daemon; an older proxy attaches to a newer one.
  - A daemon whose binary was replaced stops within 30 s (`AX_DAEMON_EXE_CHECK_MS`).
- **Gauntlet checkers.** Layer 7 of every gauntlet now also fails when `~/.cursor/mcp.json`, the Codex or Gemini config, or the `ax` entry in `~/.claude.json` changed. Layer 7 of `gauntlet-git-hooks.sh` now fails closed.

## Behavior to test mapping

| # | Test / check |
|---|---|
| A1 | `hooks_without_ax::a1_quiet_sync_without_ax_is_a_silent_no_op`; mutants A1, A4 |
| A2 | `a2_quiet_ship_evaluate_without_ax_is_a_silent_no_op`; mutant A2 |
| A3 | `a3_sync_and_ship_without_quiet_still_report_the_missing_project`; mutant A3 |
| A4 | Layer 7 of `gauntlet-git-hooks.sh` (a commit in an initialized repo prints only git's summary, and the quiet hook still records the git memory). It passed unmodified in `scripts/control-gauntlet-layer7.sh` |
| A5 | Layer 7: `git worktree add` and a `git checkout` in the new worktree exit 0 with no ax output, and no `.ax/` appears |
| B1, B5 | `init_gitignore::b1_b5_init_leaves_only_shareable_files_untracked`; mutant B1; layer 7 (B1 in a temp repo) |
| B2, B3 | `agents_share::tests::b2_local_data_is_ignored_and_policy_stays_committable`; mutant B2 |
| B4 | `b4_user_lines_survive_and_still_apply`, `b4_second_run_changes_nothing`; mutants B3, B4 |
| C1 | `ax_command` tests `c1_first_ax_on_path_wins_as_found`, `c1_a_symlinked_shim_is_not_resolved`, `c1_skips_a_file_that_is_not_executable_and_relative_entries`; mutants C1, C2; layer 7 (`mcp.json` gets the shim path) |
| C2 | `c2_without_ax_on_path_the_running_binary_is_used_and_said`; mutant C4 |
| C3 | `c3_the_note_names_both_paths_when_they_differ`, `c3_no_note_when_the_path_ax_is_the_running_binary`; mutant C3; layer 7 (the report names both paths) |
| C4 | `targets::mcp_path_tests::c4_a_stale_claude_hook_path_is_replaced_in_place`; mutant C5; layer 7 (a stale Claude `Stop` hook repaired in place) |
| C5 | `c5_a_correct_claude_hook_is_left_byte_for_byte`, `c5_a_correct_hook_in_a_hand_formatted_file_keeps_its_formatting`; mutant C6 |
| C6 | Layer 7 (`~/.cursor/mcp.json` in the temp `HOME` points to the shim); existing replace-in-place tests pass |
| C7 | Structural. `is_ax_configured` and `config_has_ax` call the same `ax_bin()` the installer writes, so no separate test. See Known limits |
| D1 | `proxy_pump::tests::d1_d7_a_call_after_the_daemon_restarted_is_served_by_the_new_one`; mutant D11 |
| D2 | `d2_an_unanswered_request_gets_an_error_with_its_own_id`, `d2_an_answered_request_gets_no_second_reply`; mutants D1, D2, D5 |
| D3 | `d3_requests_sent_during_the_reconnect_arrive_in_order`, `d3_a_request_the_dead_daemon_refused_goes_to_the_new_one_without_an_error`; mutant D3 |
| D4 | `d4_a_notification_in_flight_gets_no_error`; mutant D4 |
| D5 | `d5_no_daemon_within_the_deadline_answers_pending_and_fails`, `d5_requests_sent_while_reconnecting_are_answered_when_giving_up`, `d5_a_request_the_dead_daemon_refused_is_answered_when_giving_up`; mutants D6, D12, D13 |
| D6 | `d6_the_client_closing_its_input_ends_the_pump_without_reconnecting`, `d6_a_client_that_stopped_reading_has_left_cleanly`; mutants D7, D8 |
| D7 | `daemon_lifecycle::d7_a_proxy_keeps_serving_after_its_daemon_is_killed`, which runs real processes: the next call is answered by a new pid, the killed daemon is reaped, and closing stdin ends the proxy with exit 0. Mutants D10, D14 |
| E1 | `exe_identity` tests `e1_a_daemon_from_before_identities_attaches_on_the_same_version`, `hello_without_identity_still_parses`; mutants E6, E7 |
| E2 | `e2_the_same_binary_attaches`; mutant E2 |
| E3 | `e3_a_newer_proxy_restarts_an_older_daemon`, `e3_an_upgrade_in_place_restarts_the_daemon_still_on_the_old_file`, `daemon_lifecycle::e3_a_newer_proxy_restarts_an_older_daemon_on_its_own_binary` (real processes); mutants E1, E3, E8 |
| E4 | `e4_an_older_proxy_never_restarts_a_newer_daemon`, `e4_equal_mtimes_on_different_files_do_not_restart`, `daemon_lifecycle::e4_an_older_proxy_attaches_to_a_newer_daemon_without_restarting_it` (real processes) |
| E5 | `e5_an_untouched_binary_is_not_replaced`, `e5_a_rewritten_or_deleted_binary_is_replaced`, `e5_the_check_runs_every_30_seconds_unless_configured`, `daemon_lifecycle::e5_the_daemon_stops_when_its_binary_is_replaced` (exit 0, `daemon.json` removed); mutants E4, E5, E10; layer 7 with the release binary |
| E5 (Linux ` (deleted)`) | `linux_deleted_suffix_is_stripped`; mutant E9 |
| E6 | A version mismatch leads to a restart, not the embedded server: mutant E3 removes that rule and is killed. `attach_or_spawn` falls back only when no daemon can be reached at all. See Known limits |
| F1, F2 | `scripts/control-user-agent-files.sh` (below); layer 7 of all four gauntlets sources `scripts/lib/user-agent-files.sh` |
| F3 | `scripts/control-gauntlet-layer7.sh` (below) |
| N1 | A4 row; the existing `ax-sync` and `ax-cli` hook tests pass unchanged |
| N2 | `the_hello_line_never_reaches_the_client`; mutant D9; the existing `ax-mcp` protocol tests pass unchanged |
| N3 | E4 rows (an older proxy never restarts a daemon; equal mtimes never restart); `e5_an_untouched_binary_is_not_replaced` |
| N4 | B1/B5 test (the project `.gitignore` is untouched); C4 test (the user's other Claude hook group is unchanged); F1 check in layer 7 |
| N5 | Layer 2: exactly the 4 baseline failures |
| N6 | Not verified: no Windows machine. The `ax.exe` lookup shares code with the Unix path, and a rename-away changes the file's identity, as E5 needs |

## Gauntlet (one run on `7cb2b5c`)

| Layer | Result |
|---|---|
| 1. Targeted tests (`ax-mcp`, `ax-installer`, `ax-policy`, `ax-cli`) | 160 passed, 0 failed; pump and `daemon_lifecycle` tests green 5/5 repeats |
| 2. Workspace suite | 852 passed, 4 failed. All 4 are in `scripts/lib/baseline-install-daemon-hygiene.txt`: `bootstrap::tests::legacy_prefix_from_workspace_folder`, `bootstrap::tests::resolves_placeholder_to_folder_name`, `savings::tests::cursor_transcript_path_filter` (crates with no diff and no dependency on a changed crate), and `cycles_api_path_handlers_work` (also fails on the base, checked by stashing) |
| 3. Clippy | 0 findings on changed lines (16 files) |
| 4. rustfmt | new files 0 hunks; every changed file at or below the base count (`proxy.rs` went from 2 to 1) |
| 5. Release build + CLI docs | all 131 commands documented |
| 6. Mutants | `scripts/mutants-install-daemon-hygiene.sh` 38/38 killed (A1–A4, B1–B4, C1–C6, D1–D14, E1–E10) |
| 7. Real execution (temp `HOME`, a shim on `PATH`, the release binary) | A5 B1 C1 C3 C4 C6 E5 verified. D7 E3 E4 E5 also run as real processes in layer 1. The F1 check against the real agent configs passed |

**Negative controls:**

- **F1, `scripts/control-user-agent-files.sh`** (temp `HOME`):
  - A created `mcp.json`, an edited `mcp.json`, and a changed `ax` entry in `~/.claude.json` each count as a change.
  - Another key in `~/.claude.json` is ignored.
  - An unreadable `~/.claude.json` fails closed.

  The controls are non-vacuous. With a helper that skipped `mcp.json` (a copy in `/tmp`), exactly the two `mcp.json` controls failed.
- **F3, `scripts/control-gauntlet-layer7.sh`.**
  - Unmodified layer 7 passes.
  - With one `false` injected after the first commit, it fails with `GAUNTLET FAILED: real execution`.
  - The old `( … ) || fail` shape runs past the same failing step and never fails.
- **Layer 6.**
  - A pattern that matches twice is rejected.
  - Four mutants survived the first full run (see below).
  - The script refuses to run on unstaged changes.
- **Layer 4.** It failed a full run on real drift (see below).

## What failed along the way

- **Four mutants survived the first full mutation run (31/35).**
  - **C2:** the relative-`PATH` test used an entry that was not a relative path to an executable `ax`, so it was vacuous. It now uses a real one.
  - **C6:** equivalent; the unchanged-hook branch was redundant with `write_json_action`. I removed the branch and aimed the mutant at `json_equal`.
  - **E2 and E10:** nothing tested an in-place upgrade or the default interval. I added `e3_an_upgrade_in_place…` and `e5_the_check_runs_every_30_seconds…`.

  All four were confirmed killed on rerun.
- **Proxy bugs found by tests while building D:**
  - A line written to a dead daemon was lost; it is now kept and resent.
  - The hello line leaked when the daemon's JSON keys were sorted; it is now filtered by parsing.
  - A client that stopped reading made the proxy exit 1; it now counts as the client leaving.
- **E3 first failed in the real-process test.** `fs::copy` keeps the mtime on macOS, so the "newer" binary was not newer. The test now sets the mtime.
- **The first full gauntlet on `fd53a0c` failed at layer 4.** Four new assertions in `targets.rs` and two in `agents_share.rs` were over rustfmt's width. They were reformatted without changing an assertion (`7cb2b5c`), and the next run passed.

## Review rounds

| Round | Skills (status) | Findings | Fixed |
|---|---|---|---|
| 1 | `old-coder` usable, `rust-review` usable; bash scripts and docs by the generic checklist (no skill for them) | 1 major, 5 minor. The major: requests received during a reconnect, or refused by the dead daemon, went unanswered when the proxy gave up. Minor: the daemon's binary check and the proxy's identity lookup ran blocking I/O on the async runtime; restart and spawn failures were swallowed; the new files were not rustfmt-clean | `fb6d71e`, with RED tests `d5_requests_sent_while_reconnecting…` and `d5_a_request_the_dead_daemon_refused…` |
| 2 | same | 1 major, 1 minor. The major: daemons spawned by a proxy stayed zombies after they exited (RED: `kill -0` in D7). The minor: the client-left shutdown lacked its reason | `b18254a` |
| 3 | same | 1 minor: a failed reaper-thread start threw away the pid of a daemon that did start | `fd53a0c` |
| 4 | same | 0 | — |
| 5 | same, on `7cb2b5c` (formatting and the two control scripts) | 0 | — |

## Known limits

- **Concurrent restarts.** Two new proxies that both find an older daemon can each restart it. One of their spawns can be killed by the other, and that proxy then reconnects to the survivor. This is self-healing but noisy.
- **Restart window.** During an E3 restart, an older proxy at a different path could spawn its own daemon. Spawning starts only from reconnect attempt 3 (`FIRST_SPAWNING_ATTEMPT`), which narrows but does not close that window. E4 then keeps it from ping-ponging.
- **Checks not tested separately:**
  - **E6.** Not run with two different version strings as real processes; that would need two builds. The mutant and the attach rules cover it.
  - **C7.** Holds by construction (shared `ax_bin()`), with no separate test.
  - **N6 (Windows).** Not run.
- **`process::exit` in the exe watch.** It exits after a clean shutdown, like the existing idle watcher.
- **Parsing every line.** The proxy parses each line as JSON to track request ids. This costs a parse per message.
- **`ship.toml`.** The new `.ax/.gitignore` ignores `.ax/ship.toml` along with the rest of the local data. A team that wants to share the quality-gate config adds `!ship.toml` itself; user lines take precedence (B4). Before this change, `ship.toml` was untracked but not ignored, so it could be committed by accident.
- **Other Cursor windows.** Two stray proxies from `/tmp/ax-gauntlet*` belong to other Cursor windows started from an old `mcp.json`. They were left alone and go away after `ax install cursor` and an MCP restart.
- **Spec approval** was obtained for the whole spec before code.
