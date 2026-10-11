# EVIDENCE: read guard hook

Spec: `docs/specs/read-guard-hook.md`. Rev 2 was approved ("Approve, build it", 2026-09-23). Rev 3 is a post-approval correction found in real execution and has **not** been approved yet.
Tier: 3. The hook gates tool calls of every agent in every IDE it is installed in.
Source state: HEAD `76da9c6e16a9b88bb050b16dda0409656cf2ec04` plus uncommitted changes. sha256 of the changed files:

| File | sha256 |
|---|---|
| `crates/ax-cli/src/commands/read_guard.rs` | `ac857b12a0beeba42233199615f12b8b4487a6b2572d78c2cda47c2f0b28c1d6` |
| `crates/ax-installer/src/hooks.rs` | `951900fbe5d3ce2e762c7bf2938cf83d228ab8816d67d7fbc25dfcc48b9f4e15` |
| `crates/ax-installer/src/targets.rs` | `b2b9c1624dc95b44353a0b1f2ffa4fe9d5009c8fa09d867a02280885dc37c9b6` |
| `crates/ax-cli/src/main.rs` | `3c4ae2092ebf6cb9849f46662c5a9b8d7f83c18065606a5501c4419ab5c1258d` |
| `crates/ax-cli/Cargo.toml` | `a44d3297e543b685271c2aa9a0cba2da1b4723e823424efa113e7c890069a1f3` |
| `scripts/read-guard-mutants.py` | `53500efded2ef2fdd8dbd7ce22524a046fe925c83439969d8d7785216e247a4c` |

Tools: rustc 1.98.0, cargo 1.98.0, Python 3.14.7, macOS. Independent verification was not performed (a declared downgrade).

## Behavior → check

| Spec | Verified by |
|---|---|
| 1 Unknown tools pass fast | `unknown_tools_pass`. `classify` returns `Pass` before any path or database work (see the code in `evaluate`). Probe `zed {}` gives exit 0 with no output |
| 2 Outside ax passes | `outside_an_ax_project_everything_passes` |
| 3 `AX_READ_GUARD=off` | `guard_can_be_switched_off`. The live probe with `AX_READ_GUARD=off` returned `{"permission":"allow"}` for a read that is otherwise denied |
| 4 First whole-file read of indexed source → deny with symbols | `first_whole_read_of_indexed_source_is_denied_then_allowed`, `relative_read_resolves_against_cwd`, `cursor_whole_read_is_a_read_probe`, `null_offset_is_still_a_whole_read`. Live in Cursor, see below |
| 5 Same read again → allow | Same test: the identical retry returns `None`, and a new conversation is guarded again. Live in Cursor |
| 6 Partial reads pass | `claude_read_with_offset_or_limit_passes`, `non_source_and_partial_reads_pass`, and the shell cases (`head`, `sed -n`, piped `cat`) in `shell_parser_only_inspects_the_first_simple_command` |
| 7 Non-source files pass | `non_source_and_partial_reads_pass`: README not in the index, `logs/run.log`, and an indexed `docs/guide.md` holding only a doc node. The file, doc, and table node exclusions are asserted in the read-deny test. `Cargo.toml` was checked by a real-execution probe (Gemini `read_file` on `Cargo.toml` passes), not by a unit test |
| 8 First symbol search → deny, retry allowed | `symbol_search_is_denied_once_with_graph_hits`, `grep_tools_are_search_probes`. Live Grep in Cursor |
| 9 Other searches pass | `non_symbol_or_unknown_or_out_of_index_searches_pass` (free text, unknown symbol, and a scope outside the index), `symbol_name_rejects_text_and_regex` |
| 10 Shell: first simple command only | `shell_parser_only_inspects_the_first_simple_command`, `shell_tools_route_through_the_shell_parser` |
| 11 Conversation key, TTL 4h, cap 2000 | `conversation_key_prefers_conversation_then_session_then_anon`, `state_remembers_until_ttl_and_roundtrips`, `state_is_capped_and_drops_oldest` |
| 12 Fail open | `unwritable_state_fails_open`, `broken_database_fails_open`, `corrupt_state_loads_empty`. Probe: `not json` on stdin gives allow with exit 0 |
| 13 Output per dialect | `render_cursor`, `render_claude_and_codex`, `render_gemini`, `render_windsurf_blocks_with_exit_2`, plus one real probe per dialect below |
| 14 Tool names | `other_ide_read_shapes_are_recognised` (Windsurf, Gemini, Copilot, `readFile`, `view`), `grep_tools_are_search_probes`, `shell_tools_route_through_the_shell_parser` |
| Rev 3.1 Cursor carries the full text in `user_message` | `render_cursor`. Live in Cursor: the second denial showed the full symbol list to the agent |
| Rev 3.2 `readFile`, `view` | `other_ide_read_shapes_are_recognised` |
| Install: shapes per IDE | `cursor_entry_shape`, `nested_entry_shapes` (Claude, Codex, Gemini), `windsurf_guards_reads_and_commands`, `command_quotes_a_bin_path_with_spaces` |
| Install: idempotent, replaces a moved binary | `upsert_is_idempotent_and_replaces_a_moved_binary_in_place`, `install_and_uninstall_files` |
| Must not: rewrite entries it did not create | `upsert_preserves_unrelated_settings_and_groups`, `remove_takes_only_read_guard_entries`, `remove_keeps_pre_existing_empty_events`. The real install into `~/.cursor/hooks.json` kept both existing `ax-session-model.sh` hooks |
| Must not: clobber an invalid file | `invalid_json_is_never_rewritten`, `upsert_rejects_hooks_of_the_wrong_type` |
| Must not: block on an error path | Behavior 12 tests. One exception is listed under known limits |
| Must not: deny a second identical attempt | Behavior 5 test, plus the `retry never allowed` mutant |
| Must not: add network access | Capability diff: the hook reads stdin, `.ax/ax.db` (read-only SQLite), and `~/.ax/read-guard.json`. It has no network code, and hook commands are excluded from the update check and telemetry (`should_notify_update`, `cli_command_name`) |

## Gauntlet (final fresh run after the last code edit)

| Layer | Command | Result |
|---|---|---|
| Full suites (changed crates) | `env -u CARGO_TARGET_DIR cargo test -p ax-cli -p ax-installer`, run twice | ax-cli 40/40 and ax-installer 32/32 on both runs. The runs cover 28 read-guard tests and 12 hook-installer tests |
| Lint | `cargo clippy -p ax-installer -p ax-cli --all-targets -- -A clippy::invalid_regex` | exit 0, 0 findings in the changed files. The one finding during development (`question_mark`) was fixed. `-A clippy::invalid_regex` is needed because of a **pre-existing** deny in `crates/ax-context/src/directory.rs:155`, a file this change does not touch (see below) |
| Types | covered by `cargo build` / `cargo test` | compiles with no new warnings in the changed files |
| Mutation (manual, persisted) | `python3 scripts/read-guard-mutants.py` | **15/15 killed**, sources restored with sha256 verified. The script fails closed on a mutant that does not apply, does not compile, or survives |
| Latency | 50 runs per case of the installed release binary against this repo's real `ax.db` | p95: read deny 17.3 ms, symbol grep 17.1 ms, unknown tool 14.9 ms, shell passthrough 16.3 ms. Budget: 150 ms. The worst single run in the final pass was 160.9 ms. The first run after a rebuild once took 1.4 s (cold start of a new binary) |
| Real execution, per dialect | stdin probes against the installed binary | cursor: deny, then allow on retry. claude: Grep denied; `read_file` with a line range passes. codex: `cat` denied. gemini: search denied; `Cargo.toml` passes. windsurf: read gives exit 2 with stderr; `cargo build` passes. `AX_READ_GUARD=off` allows. Garbage stdin allows. Unknown `--ide` prints nothing, exit 0 |
| Real execution, live IDE | `ax install --yes --target cursor`, then Read and Grep tool calls in this Cursor session | A whole-file Read of `cli_install.rs` was denied with the symbol list. The identical retry on `report.rs` passed. Grep for `install_read_guard` was denied with the graph hit |
| Supply chain | `crates/ax-cli/Cargo.toml` | `sqlx` is now a direct dependency of ax-cli. It was already a workspace dependency, and `Cargo.lock` gains no new packages. No secrets in the diff |
| Suite health | Rust test harness (parallel, nondeterministic order), two runs | identical results. Tests use per-process temp directories |

## Failures found and how they were resolved

- **Vacuous assertion (caught by mutation).** The "doc nodes counted as symbols" mutant survived: the fixture's doc node lived in a file that was not in `files`, so the "no README" check could never fail. Added an indexed `docs/guide.md` with only a doc node, and a table node inside `src/lib.rs`, with assertions for both. Also added a "table nodes counted" mutant. Both are killed now.
- **Installer bug found in review.** `remove` used a running flag across events, so an untouched, pre-existing empty event could be deleted after an earlier event changed. Fixed with a per-event flag and a regression case, and the mutant for it is killed. **Process deviation:** the installer tests in `hooks.rs` were written together with the implementation instead of RED-first. Mutation (5 installer mutants, all killed) partly makes up for this.
- **Spec defect found in the live test (rev 3).** Cursor hands the agent `user_message`; the `agent_message` never reached it. The spec was revised openly, the test changed first (RED observed: 2 failing), then the implementation (GREEN), then the unused short message was removed in a separate refactor.

## Known limits

- **Old binary + installed hook blocks tool calls.** A binary without `read-guard` (after a downgrade) exits 2 from argument parsing. Cursor, Claude Code, and Windsurf treat exit 2 as a block, so every Read/Grep/Shell would be denied until `ax install` or `ax uninstall` runs again. Seen with a stale debug binary during development.
- **Cursor appends "Do not suggest workarounds to the blocked tool"** to every deny. This slightly contradicts our "repeat this same read" hint. The agent still receives the hint.
- **Concurrent state writes.** Two hooks firing at the same moment can overwrite each other's state entry, so a later retry might be denied once more. The write is atomic (temp file + rename), so the file is never corrupt.
- **Shell parsing is shallow by design.** `cd x && cat y`, subshells, `xargs`, and editors are not inspected. That under-enforces; it never over-blocks.
- **Codex** only runs the hook after it is trusted once with `/hooks`. The installer says so. **VS Code Copilot** runs the hook on every tool (it ignores matchers); unknown tools return before any I/O.
- The hook command points to the binary path at install time (here `target-dev/release/ax`). Moving or cleaning that binary makes the hook command fail. Cursor and Claude treat a non-2 failure as allow; re-running `ax install` fixes the path.
- Not installed in Claude, Gemini, Codex, or Windsurf on this machine yet. Those configs get the entry the next time `ax install` runs for them. Backups of the existing files are in `/tmp/ax-hook-backup/`.

## Pre-existing issues seen, not changed

- `crates/ax-context/src/directory.rs:155`: `regex::Regex::new(r"[.*+?^${}()|[\]\\]")` fails clippy's `invalid_regex` ("unclosed character class"). The pattern likely panics at runtime when it is built.
- `crates/ax-installer/src/targets.rs` `read_json` turns an invalid JSON file into `{}`, so the existing MCP and prompt-hook writers can overwrite a broken settings file. The new hooks code uses a strict reader instead.
