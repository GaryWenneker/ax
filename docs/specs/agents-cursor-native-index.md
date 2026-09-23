# SPEC — Index and zip restore skip Cursor-native `.agents` files

**Tier:** 2  
**Spec approval:** approved 2026-09-02 (user: "approve")  
**Isolation:** current branch (no worktree) — crate tests use `target-dev` / existing deps.  
**No new dependencies.**

## Problem

Zip restore writes into `.agents/`, then Command Center / CLI re-index (`import_policy_from_files`). The leak gate treats **parse failures** as `.agents contains private or inactive files`. Cursor-native `.mdc` / `SKILL.md` (no ax `id`/`level`, or missing skill `description`) abort restore after bytes are already on disk.

## Behaviors

### C1 — Leak gate is only private/inactive

Given `P/.agents/rules/cursor.mdc` with Cursor frontmatter only (`description`, `alwaysApply`, no `id`/`level`)  
And `P/.agents/skills/client-pr/SKILL.md` with `name` and no `description`  
When `agents_share_violations(P)` runs  
Then the list is **empty**.

### C2 — Parsed leaks still fail closed

Given an ax-schema rule under `.agents/rules` with `enabled: false` or `scope: private_project`  
When `agents_share_violations(P)` runs  
Then the list is **non-empty** and mentions disabled or non-packable (unchanged).

### C3 — Index skips unparseable files

Given C1 files plus a valid ax rule `ok.mdc` (`id`, `level`, `alwaysApply: true`)  
When `import_policy_from_files` runs  
Then it returns `Ok` and indexes the valid ax rule `ok`. Cursor-native files are not upserted (no id `no-ab-prefix`). Extra rules from other policy layers (company/user home) may also appear; the test does not require `rules_indexed == 1`.  
Unparseable files are not an `AxError`.

### C4 — Zip restore then index

Given dest `.agents` already has a Cursor-native rule  
When `restore_policy_zip` writes a packed enabled project rule and `import_policy_from_files` follows  
Then both succeed.

### C5 — MCP session policy refresh must not fail on Cursor-native files (2026-09-02)

Every `ax_preflight` (and other policy tools) calls `ensure_policy_ready` before the tool body. In **database** storage that runs `policy_dir_disk_stale`, which used to `parse_rule_file` / `parse_skill_file` and **return `AxError` with `validation_failed`** on Cursor-native neighbors.

Given database mode, a valid ax rule already indexed, and C1 Cursor-native files under `.agents`  
When `ensure_policy_ready` runs  
Then it returns `Ok` (not `validation_failed`). Unparseable files are skipped the same way as import.

That error is what Windows Cursor shows as **set policy/session failed (ax_preflight validation_failed)** and **Mode: PARTIAL**.

### C6 — Index `.scripts` (hidden dir whitelist)

`WalkBuilder.hidden(true)` never descends into `.scripts/`, so a client project's `.scripts/wcag` is missing from the code graph and agents fall back to Grep.

Given `P/.scripts/wcag/Triage.cs`  
When `scan_files` runs  
Then the path is in the result. `.git` / `.ax` stay skipped.

## Must not

- Auto-upgrade Cursor files to ax schema.
- Allow parsed private/disabled files under `.agents`.
- New cargo/npm dependencies.
- Dutch UI strings.

## Setup

| Item | Path |
|------|------|
| Spec | `/Users/gary/io/ax/docs/specs/agents-cursor-native-index.md` |
| Evidence | `/Users/gary/io/ax/docs/specs/agents-cursor-native-index-EVIDENCE.md` |
| Tests | `crates/ax-policy/src/agents_share.rs`, `crates/ax-policy/src/index.rs`, `crates/ax-policy/src/zip_package.rs` |
| Gauntlet | `/Users/gary/io/ax/tools/gauntlet-agents-git-share.sh` (extend) |
| Docs | `site/src/content/docs/guides/policy-engine.md` |

## Mapping

| Behavior | Test |
|----------|------|
| C1 | `leak_gate_ignores_cursor_native_files` |
| C2 | existing `leak_gate_detects_disabled_and_private` |
| C3 | `import_skips_unparseable_cursor_files` |
| C4 | `restore_then_index_with_cursor_native_neighbor` |
| C5 | `ensure_policy_ready_skips_cursor_files_when_checking_stale` |
| C6 | `scan_files_includes_scripts_dot_dir` |
