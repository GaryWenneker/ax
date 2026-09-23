# EVIDENCE — policy dedup (one copy per rule and skill)

Spec: `docs/specs/policy-dedup.md`, including the append-only section "Revisions during the build" (items 1–5).

- **Tier:** 3 (it deletes data rows and changes which policy body agents load).
- **Spec approval:** obtained before implementation ("Approve, build it"). The decisions behind it were "db-only" (disk files stay; the import skips global-level names), "longest" (the longest trimmed body wins; on a tie the most recently changed copy), and "both" (skills and rules). Revisions 1–5 were made during the build and have not been separately approved; they are listed below so you can review them.
- **Isolation:** none. The work was done in the working tree, which already had many uncommitted changes from earlier tasks. Nothing was committed.
- **Source state:** HEAD `76da9c6` plus uncommitted changes. SHA-256 of the change set (the diff of the changed files plus the new files) is `9a40369cbc420c11…`.
- **Toolchain:** cargo 1.98.0, Python 3.14.7.

## What changed

- **`global.db`:**
  - `level` column (`global` or `mirror`); existing rows became `global`.
  - `global_policy_revisions`: monotonic `version` per item, 20 kept; an unchanged write records nothing.
  - A row stored before versioning keeps its old body as version 1 (`baseline`).
- **`ax global sync`:** writes `mirror` rows, skips names that exist at the global level, then runs the cleanup.
- **Import (`ax-policy`):** skips a disk rule or skill whose global copy wins. It only reads `global.db`.
- **Engine (`ax_core::policy_dedup::run`):**
  - collapses duplicate global rows to the winner;
  - removes mirrors of global names;
  - removes project rows that lose;
  - promotes a longer project copy into the global row first.
  - Every removed body is kept as a revision. Line endings and surrounding whitespace do not count.
- **Triggers:**
  - after `ax sync`, `ax index`, and the watch sync;
  - after every policy index (`ax init`, `ax policy index`, MCP `ax_policy_index`);
  - after `ax global sync`;
  - at the end of `ax install`;
  - in `ax web` when a workspace opens, then every 10 minutes (not when read-only);
  - by hand: `ax policy dedup [--dry-run] [--json]`.
- **`ax init`:** no longer seeds the `global.db` skills (`review-loop`, `pr-review-comments`) into `<project>/.agents/skills/`.

## Spec behaviours mapped to tests

| Spec | Test | Result |
|---|---|---|
| D1 | `ax-global-db` `policy_level_tests::existing_global_rows_become_level_global` | pass |
| D2 | `policy_level_tests::global_sync_writes_mirrors_and_skips_global_names` | pass |
| D3 | `ax-core` `policy_dedup::tests::identical_project_copy_is_removed` (skill + rule) | pass |
| D4 | `policy_dedup::tests::shorter_project_copy_is_removed_global_kept` (skill + rule) | pass |
| D5 | `policy_dedup::tests::longer_project_copy_is_promoted` | pass |
| D6 | `policy_level_tests::global_revisions_count_up_and_cap_at_20` | pass |
| D7 | `policy_dedup::tests::duplicate_global_rows_collapse_to_the_winner` | pass |
| D8 | `ax-policy` `index::tests::import_skips_names_at_the_global_level` (skills + rules) | pass |
| D9 | `policy_dedup::tests::dedup_never_touches_disk_files` | pass |
| D10 | `policy_dedup::tests::missing_global_db_is_a_noop`; import side `index::tests::import_without_global_db_keeps_everything` | pass |
| D11 | `policy_dedup::tests::second_run_changes_nothing` | pass |
| D12 | `policy_dedup::tests::tie_goes_to_the_newest` | pass |
| D13 | `ax-web` `workspace_state::policy_dedup_tests::readonly_web_does_not_dedup` (+ `writable_web_dedups_on_open`) | pass |
| D14 | `policy_dedup::tests::dry_run_changes_nothing` (now includes a mirror row) | pass |
| Hooks in sync / policy index | `ax-core` `tests/policy_dedup_hooks.rs::sync_and_policy_index_promote_and_remove_duplicates` | pass |
| Revision 1 (line endings) | `global_level::tests::windows_line_endings_are_not_more_extensive`, `policy_dedup::tests::crlf_copy_counts_as_identical` | pass |
| Revision 2 (baseline) | `policy_level_tests::a_row_without_history_keeps_its_old_body_as_version_1` | pass |
| Revision 3 (init seed) | `seed::tests::project_seed_leaves_global_db_skills_out` | pass |
| Revision 4 (old schema dry run) | `policy_dedup::tests::dry_run_on_a_pre_versioning_global_db_says_so_and_the_real_run_upgrades` | pass |
| Revision 5 (errors) | `policy_dedup::tests::an_unreadable_row_stops_the_run_with_an_error`, `a_corrupt_global_db_is_an_error_not_a_skip`, `report_lines_cover_skip_actions_and_error` | pass |

### Must not

| Clause | Evidence |
|---|---|
| Delete or edit any file on disk | D9 test; mutant runs restore sources by SHA-256. Real run: `/Users/gary/io/.agents/skills/` still holds `auti`, `noti`, `old-coder-api`, `systematic-debugging` |
| Change which body agents load, except by promotion | Real run: `global.db` still has 8 global skills and 1 rule, with 0 revisions and 0 promotions. `list_skill_payloads` returns only `level = 'global'`, and all pre-existing rows are `global` |
| The import writes `global.db` | Code review only: `global_level.rs` issues only `SELECT` and `pragma` statements. No executable check |
| Add a dependency | `git diff` of the `Cargo.toml` files of `ax-core`, `ax-web`, `ax-global-db`, `ax-policy`, and `ax-utils` is empty |

## Gauntlet (one final fresh run after the last source edit)

| Layer | Command | Result |
|---|---|---|
| Full suite | `env -u CARGO_TARGET_DIR cargo test -p ax-global-db -p ax-policy -p ax-core -p ax-web -p ax-cli -p ax-mcp` | exit 0; **377 passed, 0 failed**, across 11 test binaries |
| Test isolation | snapshot of the real `~/.ax/global.db` policy rows and revision count, before and after the full suite | unchanged (10 lines identical) |
| Suite health | dedup, level, import, and seed tests run 3 times | 70/70, 70/70, 70/70 |
| Types + lint | `cargo clippy … --all-targets -- -A clippy::invalid_regex` | exit 0; 0 warnings in new files; the only hit in modified files is an existing warning at `commands/policy.rs:378` (`run_guard`) |
| Format | `rustfmt --check --edition 2021` on the 4 new files | clean |
| Mutation | `python3 scripts/policy-dedup-mutants.py` | **30/30 killed**, sources restored (SHA-256 verified) |
| Supply chain | no dependency changes; secret scan of the diff | no secrets (the hits are guidance text in stack templates) |
| Real execution | see below | pass |

`-A clippy::invalid_regex` allows one existing `deny` lint in `crates/ax-context/src/directory.rs:155`, which is not part of this change. Without it, clippy stops before reaching `ax-core`, `ax-web`, and `ax-cli`.

### Mutants (30)

Engine and level logic:

- shorter copy wins
- tie goes to the older copy
- identical bodies compared by time
- line endings count as content
- mirror rows counted as global
- dry run on an old schema runs anyway
- stop reason reported as a skip
- error line dropped from the report
- missing `global.db` gets created
- longer copy removed without promotion
- project row removed without a revision
- dry run deletes project rows
- dry run deletes mirrors
- global collapse removes the winner

Import and seed:

- import skip removed for skills
- import skip removed for rules
- import skips everything without `global.db`
- init seeds global-level skills into the project

Hooks in `ax-core`:

- no dedup after sync
- no dedup after policy index

`global.db` layer:

- version not incremented
- cap keeps one version too many
- unchanged write records a revision
- old body of a row without history is lost
- dry run predicts the wrong version for a row without history
- delete records no revision
- agents load mirror skills
- global sync mirrors global names

Web:

- read-only web still dedups
- web never dedups

**Negative control for the runner:** an earlier run reported 3 survivors (`SURVIVED: …`) and exited non-zero. A later run reported `DID NOT COMPILE` and `NOT APPLIED` for two mutants whose anchors had changed. So the runner fails closed; each of those cases led to a stronger test or a corrected anchor.

**Tests that passed as soon as they were written** were checked with a mutant:

- `import_without_global_db_keeps_everything`, by the mutant "import skips everything without global.db";
- `readonly_web_does_not_dedup`, by "read-only web still dedups";
- `report_lines_cover_skip_actions_and_error`, by "error line dropped";
- `a_corrupt_global_db_is_an_error_not_a_skip`, by "stop reason reported as a skip";
- the hook test, by "no dedup after sync" and "no dedup after policy index".

## Real execution on this machine

Backup before the first real run: `~/.ax/backup-dedup-20260923/`, containing `global.db`, `ax.db`, `io.db`, and `mijnvf.db` (SQLite `.backup`).

| Database | Before | After | Revisions |
|---|---|---|---|
| `~/.ax/global.db` | 8 global skills, 1 rule, 0 mirrors | same | 0 (nothing promoted) |
| `/Users/gary/io/ax/.ax/ax.db` | 22 skills, 0 duplicates | same | — |
| `/Users/gary/io/.ax/ax.db` | 24 skills, 4 duplicates | 20 skills, 0 duplicates | 4 × `dedup` |
| `/Users/gary/io/MijnVF/.ax/ax.db` | 18 skills, 4 duplicates | 14 skills, 0 duplicates | 4 × `dedup` |

- **First dry run:** it would have promoted the four `/Users/gary/io` copies. They differ from the global copies only by Windows line endings, which led to revision 1. After the fix, all 8 duplicates were "identical to the global copy" and were removed.
- **Second real run:** "nothing to clean" in all three projects.
- **After the final reinstall,** the dry run reports "nothing to clean" in all three projects.
- **No return after a reindex:**
  - `ax policy index --force` in `/Users/gary/io` and `ax sync` in MijnVF: still 0 duplicates.
  - `ax sync` in `/Users/gary/io` panicked once (see Known limits). The row count stayed at 20.
- **`ax web`,** in a throwaway project with a copy of the backed-up `global.db` via `AX_GLOBAL_DB`: an identical duplicate was removed 4 s after opening, with one `dedup` revision and an info log line.
- **Error path:** a row stored as a BLOB makes `ax policy dedup` print the reason and exit 1, and nothing is removed.
- **`ax global sync`,** in a throwaway project:
  - `auti` (identical to the global copy) is not imported and not mirrored;
  - `probe-local` becomes a `mirror` row;
  - `review-loop` stays only at the global level;
  - `ax init` no longer writes `review-loop` or `pr-review-comments` into the project.
- **Reinstall:** `scripts/reinstall-cli.sh`. The PATH shim `~/.local/bin/ax` points to `target-dev/release/ax`, with one hash (`f8d9ab97…`).

## Review rounds

| Round | Skills (status) | Findings | Fixed |
|---|---|---|---|
| 1 | `old-coder` usable, `rust-review` usable (`old-coder-api` not applicable: no HTTP change) | 6 (2 major, 4 minor): R1-1 dry-run version fallback hid errors and gave wrong versions on an old schema; R1-2 `ax global sync` dropped the dedup error; R1-3 `lines()` untested; R1-4 `use` in the middle of the file; R1-5 clumsy installer match; R1-6 `ax global sync` hook never executed | all (R1-1 via RED test, revision 4; R1-6 by real execution, which also found revision 3) |
| 2 | same | 2 minor: R2-1 empty-body fallback undocumented; R2-2 missing trailing newline in `paths.rs` | both |
| 3 | same | 2 minor: R3-1 new files not rustfmt-clean; R3-2 no test for a corrupt `global.db` (surviving mutant) | both |
| 4 | same | 0 | — |

## Deviations from the setup plan

- **Extra new files:**
  - `crates/ax-policy/src/global_level.rs`: the read-only view shared by the import and the engine;
  - `crates/ax-core/tests/policy_dedup_hooks.rs`;
  - `crates/ax-global-db/src/policy_level_tests.rs`.
- **Extra changed files:**
  - `crates/ax-policy/src/seed.rs` (revision 3);
  - `crates/ax-policy/src/revisions.rs` (`SOURCE_DEDUP`).
- **Revisions table:** it has no `content_hash` column; unchanged writes are detected by comparing the payload text.
- **Path resolution:** the `global.db` path resolver lives in `ax-utils` and takes `home` as a parameter.

## Known limits

- **Import prune leaves no revision.** When the import skips a global-level name, replace-mode pruning deletes the existing project row without a `dedup` revision. This happened in `/Users/gary/io/ax` before the first real run. The body is still on disk, so nothing is lost.
- **Removed mirrors get no revision.** They are copies of project rows, and `ax global sync` recreates them.
- **Tests in other crates can open the real `~/.ax/global.db`.** The full suite applied the additive schema migration (`level` column, revisions table) to it. Every policy row stayed unchanged, as the snapshot shows. `ax-policy` tests are isolated through a test-only path; the engine takes explicit paths.
- **"Longest wins" can undo an upgrade.** It can promote an older project copy over a newly seeded global version, if the old copy is longer. That follows the chosen rule, and every overwrite keeps the previous body as a revision.
- **`ax install` hook not executed for real,** because a real install writes `~/.cursor` and agent configs. It is covered by the build and by `dedup_global_blocking` sharing `run_default` with the tested paths.
- **Existing panic, not fixed:**
  - `ax sync` in `/Users/gary/io` panicked once with "end byte index 400 is not a char boundary".
  - The candidates are byte slices in untouched extraction code: `crates/ax-resolution/src/frameworks/react.rs:60` and `crates/ax-extraction/src/contracts.rs:239`, last changed in `7a31b7d`.
  - It did not reproduce on the next run.
- **Running `ax` processes need a restart.** MCP servers and `ax web` instances started before the reinstall still run the old binary until restarted.
