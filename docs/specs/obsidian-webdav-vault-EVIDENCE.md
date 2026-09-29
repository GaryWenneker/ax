# EVIDENCE: Obsidian vault over WebDAV

- Spec: `docs/specs/obsidian-webdav-vault.md` (approved at commit `16b6b99`; revisions 1–10 appended afterwards and listed in the spec).
- Spec approval: obtained. The user answered the approval question with "approve" ("Approved, start building") before any implementation was written.
- Tier: 3 (data loss on rules/skills/memories, a new network surface, a schema migration).
- Source state: `01c80deefe7cca466220e249bc40cee12d44adca` on branch `obsidian-webdav`, worktree `/Users/gary/io/ax-dav`, based on `60d9369`.
- Toolchain: `rustc 1.98.0 (88d9e12ae 2026-08-18)`, `cargo 1.98.0 (797e8a9bc 2026-08-05)`.
- Entry point: `scripts/dav-gauntlet.sh` (tests, 10x suite health, clippy, rustfmt, mutation). One final fresh run, after the last code edit, exited 0.
- Independent verification: not performed (declared downgrade). The findings below are the builder's own.

## Behaviour to test mapping

All tests live in `crates/ax-web/tests/dav_vault.rs` unless another file is named. "pages" means a unit test in `crates/ax-web/src/dav/pages.rs`.

| Spec item | Test(s) |
|---|---|
| B1 root lists areas and `DRAFTS.md` | `b01_root_lists_areas_and_drafts` |
| B2 rules folder with real sizes | `b02_rules_folder_lists_rules_with_real_size` |
| B3 rule page is the serialized rule | `b03_get_rule_is_serialized_rule`, pages `rule_page_round_trips` |
| B4 memory page carries its id | `b04_get_memory_has_id_in_frontmatter`, pages `memory_page_round_trips` |
| B5 valid PUT saves and records a revision | `b05_put_valid_rule_saves_and_records_revision` |
| B6 body-only PUT keeps frontmatter | `b06_put_body_only_keeps_frontmatter`, pages `rule_body_only_keeps_existing_frontmatter` |
| B7 invalid frontmatter becomes a draft, rule unchanged | `b07_invalid_frontmatter_becomes_draft_and_keeps_rule`, pages `rule_invalid_frontmatter_is_rejected` |
| B8 id / file-name mismatch becomes a draft | `b08_id_mismatch_becomes_draft_and_touches_neither_rule`, pages `rule_id_mismatch_is_rejected` |
| B9 a valid save clears the draft | `b09_valid_save_clears_draft` |
| B10 memory PUT updates fields | `b10_put_memory_updates_fields`, pages `memory_without_frontmatter_keeps_stored_fields` |
| B11 body-only page creates a rule (revision 1) | `b11_body_only_page_creates_rule`, pages `new_rule_from_body_only` |
| B12 Obsidian "new note" flow ends in a rule | `b12_obsidian_new_note_flow_ends_in_rule` |
| B13 new memory page, then update the same memory | `b13_new_memory_page_then_update_same_memory`, pages `memory_stems_are_safe_and_unique` |
| B14 MOVE renames a rule | `b14_move_renames_rule` |
| B15 MOVE renames a memory | `b15_move_renames_memory` |
| B16 DELETE removes a rule | `b16_delete_rule` |
| B17 top-level folders cannot be deleted or moved (revision 6) | `b17_top_level_folders_cannot_be_deleted_or_moved` |
| B18 `.obsidian/` persists across restart | `b18_obsidian_config_persists_across_restart` |
| B19 macOS junk files accepted and dropped | `b19_macos_junk_is_accepted_and_dropped` |
| B20 OPTIONS advertises class 2, LOCK works | `b20_options_advertises_locking_and_lock_works` |
| Skills (revision 2) | `skill_page_round_trip_and_body_only_save`, pages `skill_rules_match_rule_rules` |
| Path classification, bad names | pages `classify_paths`, `empty_or_bad_name_is_rejected` |
| N1 no note file on disk | `n1_vault_writes_leave_no_files_on_disk`; real execution check 16 |
| N2 malformed page never overwrites | B7 and B8 tests; mutants `draft-kept`, `id-match` |
| N3 traversal returns 400 (revision 3) | `n3_path_traversal_is_rejected` (`..`, `%2e%2e`, `..%2f`) |
| N4 no CORS on `/dav` | `n4_dav_sends_no_cors_headers` (with a control proving `/api` does send CORS) |
| N5 readonly / share blocks writes (revisions 4, 5) | `n5_readonly_blocks_every_write`; `crates/ax-web/tests/dav_share.rs::dav_requires_share_token_when_sharing` |
| N6 PUT over 5 MB is rejected | `n6_put_over_5mb_is_rejected` |
| N7 existing routes unchanged | full `ax-web`, `ax-db`, `ax-policy`, `ax-memory` suites (below) |

## Gauntlet (final run at `01c80de`)

| Layer | Command | Result |
|---|---|---|
| Full test suite | `cargo test -q -p ax-web -p ax-db -p ax-policy -p ax-memory --no-fail-fast` | 18 test binaries, **291 passed, 0 failed**; `dav_vault` 26/26, `dav_share` 1/1, pages 11/11 |
| Suite health | the two dav test binaries, 10 consecutive runs | 10/10 green |
| Static types | covered by `cargo test` / `cargo clippy` compiling | 0 errors |
| Lint | `cargo clippy -p ax-web --all-targets --message-format=short -- -A clippy::invalid_regex`, filtered to `crates/ax-web/src/dav/` and `tests/dav_*` | **0 warnings** in dav code |
| Format | `rustfmt --check --edition 2021` on the five new Rust files | clean |
| Mutation (manual, fail-closed runner) | `scripts/dav-mutants.sh` | **12/12 killed**: id-match, body-only-keep, memory-tags, stem-unique, folder-guard, readonly-guard, traversal-400, size-cap, draft-cleared, draft-kept, cors-outside, share-gate |
| Real execution | release binary built at `01c80de`, `ax web --port 7071` on a fresh `ax init` temp project with `HOME` isolated, driven by `curl` (script kept at `/tmp/axdav/flow2.sh`, outside the repo) | 16/16 checks OK: edit saved (204) and visible in the API and in the `.agents` file; invalid level becomes a draft listed in `DRAFTS.md`, API unchanged; the fix clears it ("No drafts."); `DELETE /dav/rules/` 403; `.obsidian` MKCOL/PUT 201; new memory page 201 and found by recall; 0 vault files on disk; 0 CORS headers on `/dav` |
| Supply chain | manual RustSec lookup of the 9 new crates (`cargo-audit` not installed) | one advisory, RUSTSEC-2022-0048 on `xml-rs`: withdrawn, informational (unmaintained). All licenses MIT or Apache-2.0 |
| Secrets / capabilities | diff review | no secrets. New capability: an HTTP surface at `/dav`, behind the share token, outside CORS, writes only to `ax.db` |

Checker controls:

- **Mutation runner:** a mutant on a comment reported `SURVIVED`, and a missing literal made the run exit 1.
- **Clippy gate:** a planted `v.len() == 0` in `dav/pages.rs` turned the gate red, and the file was restored and verified with `git diff`. This proves one known-bad case reaches the failure path. The gate only watches file paths under the dav code; it does not judge warnings elsewhere.

## Skipped layers

- **Changed-line coverage:** `cargo-llvm-cov` is not installed. The mutation score and the per-behaviour tests stand in for it, which is weaker.
- **Property-based tests:** not added. The page round-trips are covered by example tests only.
- **`cargo audit`:** not installed. I checked the advisory database by hand instead.
- **Finder mount + Obsidian GUI:** not verified. On this machine (macOS 27 beta), the system WebDAV client hangs on `ls` when mounted from the agent shell, and Apache `mod_dav` shows the same hang. It is an environment limit, not this server. **This step is left to the user.**

## Failures and fixes along the way

- `/dav/../api/version` returned 502. The test was tightened to 400 (RED), then fixed in the request guard.
- dav-server deletes a folder's children before the folder, which meant data loss. Fixed with the request-level folder guard (B17, mutant `folder-guard`).
- The first mutation run overlapped an earlier run and misattributed kills. A lock was added and the run repeated cleanly.
- The earlier mid-task claim "0 clippy warnings in dav code" was wrong. Clippy never reached `ax-web`, because a deny-level `invalid_regex` error in `crates/ax-context/src/directory.rs:155` stops the build first. The gauntlet now allows only that lint. Two real `type_complexity` warnings in `dav/fs.rs` surfaced and were fixed by a refactor under green (a row type alias). No test changed.
- The schema tripwires in `crates/ax-db/tests/migration_v17.rs` to `v20.rs` were updated from 20 to 21 (revision 8).
- The first real-execution run on `01c80de` targeted a rule (`go-errors`) that a fresh `ax init` does not seed. The first PUT created it from a body-only page (201, B11), and the "restore" PUT sent back an empty 404 body, which the server rightly kept as an "empty page" draft. The run was repeated against a seeded rule (`codegraph-parity`); those are the numbers above.
- The real-execution script lives in `/tmp`, not the repo, so that layer cannot be reproduced from the repo alone.

## Known limits

- A WebDAV mount sends no change events, so edits made through MCP or the Command Center appear in Obsidian only after the note is reopened.
- Last write wins. Earlier versions remain in the revision history.
- Drafts show up only in `DRAFTS.md`, not as an error inside the editor.
- The filesystem-level readonly check is shadowed by the request guard and is not tested on its own.
- Store validation errors (as opposed to parse errors) record only the top-level message in the draft reason, without field details.
- N3 absolute paths: HTTP paths always start under `/dav`, so only relative and encoded traversal was tested.

## Found outside scope (not fixed)

- `crates/ax-context/src/directory.rs:155` `escape_regexp` builds `Regex::new(r"[.*+?^${}()|[\]\\]").unwrap()`, which the regex crate rejects (unclosed character class). It will panic whenever it is called.
