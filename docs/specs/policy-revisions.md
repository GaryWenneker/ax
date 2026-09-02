# SPEC: Policy hash-on-change revisions

**Status:** approved (user: “accept”, 2026-09-02)  
**Tier:** 2  
**Isolation:** current working tree (same as in-progress policy zip work). No new crates.

## Product

Command Center and `ax policy restore` keep a **capped local revision log** of rule/skill content. A row is written only when the blake3 hash of the serialized document **changes**. Team history remains git. Zip `contentHash` is unchanged.

## Not in scope

- Logging every Save click when bytes are identical
- Auto-git-commit
- Filesystem watchers for editor/disk edits that bypass `save_rule` / `save_skill` / zip restore
- Moving revision rows on rename (new id starts its own log)
- HMAC/PGP, 3-way merge, Review-queue staging
- New CLI subcommand
- Zip `formatVersion` bump

## Schema (v20)

Table `policy_revisions`:

| Column | Type | Meaning |
|---|---|---|
| `id` | INTEGER PK AUTOINCREMENT | Revision id |
| `kind` | TEXT | `rule` or `skill` |
| `item_id` | TEXT | Rule id or skill name |
| `content_hash` | TEXT | blake3 hex of `body` |
| `body` | TEXT | Full serialized document (restore payload) |
| `source` | TEXT | `save` or `restore` |
| `created_at` | INTEGER | Unix ms |

Index `(kind, item_id, created_at DESC)`. Cap **20** rows per `(kind, item_id)`; after insert, delete older rows (`ORDER BY created_at DESC, id DESC` then `OFFSET 20`).

No backfill of items that were never saved after upgrade.

`CURRENT_SCHEMA_VERSION` becomes **20**. Existing v17–v19 migration tests pin `== 20`.

## Behaviors

### B1 — First write is a revision

Given no rows for `rule` / `rev-a`, when `record_if_changed(..., body_a, "save")` runs, then one row exists with that body, hash `blake3(body_a)`, source `save`.

### B2 — Identical body is a no-op

Given B1, a second call with the same body does not insert (count stays 1).

### B3 — Changed body inserts

Given B1, a call with `body_b` ≠ `body_a` inserts a second row; newest hash is `blake3(body_b)`.

### B4 — Cap 20

21 distinct bodies for the same item keep **20** rows; the oldest body is gone; the newest remains.

### B5 — `save_rule` / `save_skill` hook

Successful Command Center / store save uses source `save` and hashes `doc.raw` (same serialization as index upsert).

### B6 — Zip restore hook

After a successful zip write **and** `index_policy`, each `written` key (`rule:<id>` / `skill:<name>`) records source `restore` from the indexed `doc.raw`. Skip/reject items are not written and must not record.

### B7 — HTTP list (additive)

`GET /api/policy/rules/{id}/revisions` and `GET /api/policy/skills/{name}/revisions`

- 404 if the item does not exist
- 200 `{ "revisions": [ { id, kind, itemId, contentHash, body, source, createdAt } ] }` newest first
- Same auth/readonly as other policy GETs (readable when `AX_WEB_READONLY=1`)

### B8 — HTTP restore

`POST /api/policy/rules/{id}/revisions/{revId}/restore` (skills analog)

- 403 when readonly
- 404 if item or revision missing, or revision `kind`/`item_id` mismatch
- 200 applies `body` via `save_rule` / `save_skill` (records a new `save` revision if content differs from current)

### B9 — Command Center

Existing rule/skill editors (inline workspace + full editor) show **History**. Modal lists revisions (time, source label, hash prefix) and **Restore** per row. English only. `ModalShell`.

## HTTP gates (old-coder-api)

| Gate | Result |
|---|---|
| Scope | Internal Command Center API; additive routes |
| Boring | Nested under existing `/rules/{id}` and `/skills/{name}` |
| Don’t break userspace | No existing fields removed |
| AuthN | Same as other `/api/policy` (local web hub) |
| AuthZ | Restore mutations honor `AX_WEB_READONLY` |
| Idempotency | POST restore optional; duplicate restore of current bytes is a no-op save (B2) |
| Pagination | N/A — hard cap 20 |
| Errors | 404 not found, 403 readonly, 400 bad revision ownership |

## Setup plan

- Files: `crates/ax-db/src/migrations.rs`, `crates/ax-db/tests/migration_v20.rs`, pin v17–v19 `CURRENT_SCHEMA_VERSION == 20`
- `crates/ax-policy/src/revisions.rs` + `lib.rs` / `store.rs` / zip restore callers
- `crates/ax-web/src/policy.rs`, web-ui History UI + `policyApi.ts`
- `tools/gauntlet-policy-revisions.sh`, `docs/specs/policy-revisions-EVIDENCE.md`
- Site: `policy-engine.md`, `command-center.md`; CLI restore note in `cli.md`
- No new Cargo/npm dependencies (reuse blake3, sqlx)
- After CLI/web-ui: `bash scripts/reinstall-cli.sh`

## Gauntlet

Entry: `bash tools/gauntlet-policy-revisions.sh`

Layers: ax-db v20 test, ax-policy `revisions` tests, web-ui helper test + tsc, grep wiring, 3/3 manual mutants, docs greps. Coverage: changed lines exercised by unit tests (no `--cov-fail-under` in this crate; recorded as such).
