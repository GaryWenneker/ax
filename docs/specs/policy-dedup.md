# SPEC: One copy per skill and rule across global.db and project ax.db

**Tier:** 3. The change deletes database rows, so data loss is the main risk.
**Spec approval:** pending.
**Isolation:** the current working tree, as in the previous tasks, because it holds the uncommitted work this change builds on. No commits unless you ask.

## Problem

The same skill is stored at several levels:

| Skill | `~/.ax/global.db` | `ax` ax.db | `io` ax.db | client project ax.db |
|---|---|---|---|---|
| auti, noti, old-coder-api, systematic-debugging | yes | yes | yes | yes |
| pr-review-comments, review-loop | yes | yes | – | – |

Agents already let the global copy win (`merge_skills`), so the project copies are dead weight. They also come back on every import, because the import reads `.agents/skills/` and `~/.ax/global_policy/skills/` from disk.

A second problem blocks a safe cleanup. `ax global sync` copies every project skill and rule into `global.db` under that project's id. Agents then treat those copies as global too. So a skill mirrored from project A overrides the same-named skill in project B, and a cleanup that trusts "exists in global.db" would strip every project skill after one `ax global sync`.

## Decisions (your answers)

- The cleanup changes the database only. Files on disk stay; the import skips a name that already exists at the global level.
- The most extensive copy wins: the longest body, trimmed, counted in characters. On a tie, the most recently changed copy wins.
- Skills and rules both.

## Design

1. **Global level vs mirror.** `global_policy_skills` and `global_policy_rules` get a `level` column, either `'global'` or `'mirror'`. Existing rows become `'global'`, which is how agents treat them today. `ax global sync` writes `'mirror'`, and it does not mirror a name that exists at the global level. Only `'global'` rows are loaded by agents (`list_skill_payloads`) and count for the cleanup. Command Center keeps showing mirrors as other projects' rows.
2. **Versioning in global.db.** A new table, `global_policy_revisions (kind, item_id, version, content_hash, payload, source, created_at)`, is filled on every change to a global-level row: a write through `upsert_policy_item` (Command Center save, move to global, machine seeding), a promotion, or a removal. `version` counts up per item and never goes back. The 20 newest revisions per item are kept, the same cap as project revisions. An unchanged write (same content hash) records nothing.
3. **Versioning in the project.** Before a project row is removed, its body is written to the existing `policy_revisions` with source `dedup`, so Command Center can restore it.
4. **The cleanup** (`ax_core::policy_dedup::run`), for one project and for global.db:
   1. Among `'global'` rows with the same name (under different project ids), the winner stays; the others are recorded as revisions and deleted.
   2. For every project row whose name exists at the global level, the bodies are compared.
      - If the global copy wins, or the bodies are identical, the project row is recorded and deleted.
      - If the project copy wins, it is written into the global row first, as a new version, and only then is the project row deleted.
   3. The report lists every action: `removed`, `promoted (vN)`, or `kept`, with the reason.
5. **Import skip** (`ax-policy`). A disk skill or rule whose name exists at the global level is not imported, unless its body is longer than the global copy. A longer copy is imported, and the next cleanup promotes it. This stops the import from bringing the duplicates back. The import only reads global.db.
6. **When it runs:**
   - Automatically after:
     - `ax sync`, `ax index`, and the watch sync (through `ax-core`), which also covers MCP `ax_sync`;
     - `Ax::index_policy`, which covers `ax init`, `ax policy index`, and MCP `ax_policy_index`;
     - `ax global sync`;
     - the installer, at the end of `ax install`.
   - In the web server: when a workspace opens (after the reindex), and then every 10 minutes while `ax web` runs. Read-only mode is skipped.
   - By hand: `ax policy dedup [--dry-run] [--json]`.
7. **Safe failure.** If global.db is missing, unreadable, or has an old schema, the cleanup does nothing and says why. The import then behaves as it does today. The cleanup never deletes a row before the winner is safely stored.

## Failure model

| Way this can hurt | Guard | Checked by |
|---|---|---|
| The only copy of the better version is deleted | Promote before delete; revision before delete | D4, D5, mutants |
| Mirrored project rows count as global and strip projects | `level` column; sync writes `'mirror'` | D1, D2 |
| The import re-adds what the cleanup removed (flapping) | Import skip | D8 |
| Disk files in the team repo are deleted | The cleanup never touches files | D9 |
| A missing or old global.db breaks index or sync | No-op with a reason | D10 |
| A web job and a CLI run at the same time | One transaction per database; a second run finds nothing to do | D11 (idempotence) |
| Revisions grow without limit | Cap of 20 per item; unchanged writes record nothing | D6 |
| The web server writes while in read-only mode | Skipped when read-only | D13 |

## Behaviors (acceptance tests)

- D1 `existing_global_rows_become_level_global`: after migrating an old global.db, existing rows have `level = 'global'`.
- D2 `global_sync_writes_mirrors_and_skips_global_names`: `sync_project` writes project rows as `'mirror'` and skips a name that exists at the global level. `list_skill_payloads` does not return mirrors.
- D3 `identical_project_copy_is_removed`: the project `noti` body equals the global body, so the project row is gone, a `dedup` revision exists in the project, and global.db is unchanged.
- D4 `shorter_project_copy_is_removed_global_kept`: the global body has 200 characters and the project body 100. The project row is removed, and the global row is unchanged.
- D5 `longer_project_copy_is_promoted`: the project body has 300 characters and the global body 200. The global row now holds the project body, with a new revision at `version = previous + 1`, and the project row is removed.
- D6 `global_revisions_count_up_and_cap_at_20`: 25 different writes give versions 1 to 25, with 20 kept (6 to 25). A write with an unchanged hash records nothing.
- D7 `duplicate_global_rows_collapse_to_the_winner`: two `'global'` rows `auti` under different project ids leave one row, the longer one. The loser is recorded as a revision.
- D8 `import_skips_names_at_the_global_level`: a disk skill with the same name and a body no longer than the global copy is not imported, while a longer one is imported. Rules behave the same.
- D9 `dedup_never_touches_disk_files`: `.agents/skills/noti/SKILL.md` still exists, byte-identical, after the cleanup.
- D10 `missing_global_db_is_a_noop`: with no global.db, the cleanup reports `skipped: no global.db` and project rows stay. The import still imports everything.
- D11 `second_run_changes_nothing`: running the cleanup twice gives an empty second report.
- D12 `tie_goes_to_the_newest`: with equal lengths and different content, the copy with the newest timestamp wins.
- D13 `readonly_web_does_not_dedup`: a web hub opened read-only runs no cleanup.
- D14 `dry_run_changes_nothing`: `--dry-run` gives the same report as a real run, and both databases are unchanged.
- Rules: D3, D4, D5, and D7 also run for a rule.
- Existing tests stay green, and no existing assertion is weakened. The existing `merge_skills_global_name_wins` still holds.

## Must not

- Delete or edit any file on disk.
- Change which skill body agents load, except where a longer copy is promoted (your chosen rule).
- Write global.db from the import; the import only reads it.
- Add a dependency.

## Setup plan

- Files:
  - `crates/ax-global-db`: schema migration, `policy.rs` (level filter, revisions), and `sync/mod.rs` (mirror level).
  - `crates/ax-policy/src/index.rs`: import skip.
  - `crates/ax-core/src/policy_dedup.rs` (new), plus hooks in `ax-core`.
  - `crates/ax-cli`: the `ax policy dedup` command, and hooks in `global sync` and the installer.
  - `crates/ax-web/src/workspace_state.rs`: on open, plus a 10-minute interval.
  - Docs: `site/src/content/docs/guides/policy-engine.md` and `reference/cli.md`.
  - `scripts/policy-dedup-mutants.py` (new).
  - `docs/specs/policy-dedup-EVIDENCE.md`.
- The global.db path resolution moves to `ax-utils`, so that `ax-policy` can find global.db without depending on `ax-global-db`.
- No new dependencies.

## Gauntlet

- `cargo test` for `ax-global-db`, `ax-policy`, `ax-core`, `ax-web`, and `ax-cli`.
- Clippy scoped to the changed files.
- Mutation with `scripts/policy-dedup-mutants.py`:
  - delete before promote;
  - the shorter copy wins;
  - mirrors counted as global;
  - the import skip removed;
  - the revision not recorded;
  - the version not incremented;
  - read-only not checked;
  - dry-run writing.
- Real execution:
  - reinstall;
  - run `ax policy dedup --dry-run`, then a real run on this machine; the before and after counts go in EVIDENCE;
  - `ax sync` and `ax web` both keep it clean, and the duplicates do not come back after a reindex.
- A backup of `~/.ax/global.db` and the three project `ax.db` files goes to `~/.ax/backup-dedup-<date>/` before the first real run.
- Review loop until a round has zero findings, then EVIDENCE.

## Revisions during the build (append-only)

Both came out of the first dry run on this machine. Neither changes a decision the user made; they close gaps in how the decisions were written down.

1. **Line endings do not count.** The four `/Users/gary/io` copies looked "more extensive" only because they use Windows line endings (`\r\n`): the same text, one extra character per line. Length and identity are now measured on the trimmed body with `\r\n` read as `\n`. Tests: `windows_line_endings_are_not_more_extensive`, `crlf_copy_counts_as_identical`.
2. **A global row without history keeps its old body.** Rows written before `global_policy_revisions` existed have no versions, so the first change would have overwritten the old body unrecorded. The first change to such a row now records the old body as version 1 (source `baseline`) and the new body as version 2. Test: `a_row_without_history_keeps_its_old_body_as_version_1`.
3. **`ax init` no longer seeds global-level skills into a project.** Real execution showed that `seed_project_cursor_skills` wrote every bundled skill into `<project>/.agents/skills/`, including the two stored in `global.db` (`review-loop`, `pr-review-comments`, both `scope: company`). That created the exact duplicate this spec removes, and the `company` scope made the policy index of a fresh project fail as a whole. Bundles with `global_db: true` are now left out of project seeding; `~/.cursor/skills` and `~/.ax/global_policy` still get them. Test: `project_seed_leaves_global_db_skills_out`. This regression came from the earlier review-loop task in the same session.
4. **A dry run on a `global.db` from before this change reports an error instead of guessing.** It cannot upgrade the schema without writing, so it says "global.db has an older schema; a real run upgrades it first". Any real run (sync, index, `ax web`) upgrades it. Test: `dry_run_on_a_pre_versioning_global_db_says_so_and_the_real_run_upgrades`.
5. **Errors are reported separately from skips.** `DedupReport.error` holds why a run stopped; the CLI exits non-zero on it, `ax web` and the sync hooks log it at warn, `ax global sync` and `ax install` print it. `skipped` is only for "no global.db". Test: `an_unreadable_row_stops_the_run_with_an_error`.
