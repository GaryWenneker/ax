# SPEC: Obsidian vault served by ax over WebDAV

Status: draft, awaiting approval
Tier: 3 (writes and deletes policy rules, skills, and memories)

## Goal

Open the ax database in Obsidian as a normal vault. You get the standard editor,
file list, backlinks, Live Preview, and hotkeys. No note files exist on local
disk. `ax web` serves the pages over WebDAV, macOS mounts that share as a
folder, and Obsidian opens the folder.

## How you use it

1. `ax web` is running (port 7070).
2. Finder → Go → Connect to Server → `http://127.0.0.1:7070/dav/` → Connect.
   macOS mounts it at `/Volumes/dav`.
3. Obsidian → Open folder as vault → `/Volumes/dav`.

## Vault layout

| Path | Backed by | One file per |
|---|---|---|
| `rules/<id>.md` | `policy_rules` via `PolicyStore::save_rule` / `delete_rule` | rule |
| `skills/<name>.md` | `policy_skills` via `PolicyStore::save_skill` / `delete_skill` | skill |
| `memories/<title>.md` | `memories` via `ax_memory` update / remember / delete | memory |
| `DRAFTS.md` | generated, read-only | list of pages that did not save, with the reason |
| `.obsidian/**` and any other path | new table `dav_files` in `ax.db` | Obsidian settings, plugins, workspace |

Page content:

- Rule page = exactly what `serialize_rule(frontmatter, body)` produces today (the
  same text as a `.agents/rules/*.mdc` file).
- Skill page = `serialize_skill(frontmatter, body)`.
- Memory page = frontmatter `id`, `kind`, `tags`, `files`, then the body. The
  file name is the memory title (characters Obsidian forbids in names become
  `-`; a clash gets ` (2)`).

## Behaviors (each becomes a test)

Reading

- B1 `PROPFIND /dav/ Depth:1` lists `rules/`, `skills/`, `memories/`,
  `DRAFTS.md`, and `.obsidian/` once it exists.
- B2 `PROPFIND /dav/rules/ Depth:1` lists one `<id>.md` per rule in the
  database, with size and last-modified from the row.
- B3 `GET /dav/rules/english-only.md` returns `serialize_rule` of that row,
  byte for byte.
- B4 `GET /dav/memories/<title>.md` returns the memory with its `id` in the
  frontmatter.

Saving an existing page

- B5 `PUT rules/english-only.md` with valid frontmatter whose `id` is
  `english-only` saves through `save_rule`. A following `GET /api/policy/rules/english-only`
  returns the new body. A policy revision is recorded, as with Command Center saves.
- B6 `PUT rules/english-only.md` with no frontmatter keeps the stored
  frontmatter and replaces only the body.
- B7 `PUT rules/english-only.md` with frontmatter that fails to parse or
  validate does not change the rule. The text is kept as a draft at that path:
  `GET` returns the text you wrote, `DRAFTS.md` lists the path and the parse
  error, and `GET /api/policy/rules/english-only` still returns the old rule.
- B8 `PUT rules/english-only.md` with frontmatter `id: other-rule` does not
  touch either rule. It becomes a draft with reason "id does not match file name".
- B9 A draft that later saves successfully is removed from `dav_files` and from
  `DRAFTS.md`.
- B10 `PUT memories/<title>.md` updates body, kind, tags, and files of the
  memory named by the frontmatter `id`.

Creating

- B11 `PUT rules/new-rule.md` with only body text creates rule `new-rule`,
  level `NORMAL`, with that body.
- B12 `PUT rules/Untitled.md` with an empty body creates nothing; it is a
  draft. Obsidian's "new note" flow (empty file, then rename, then type) ends in
  a saved rule once the name is a valid id and the body is not empty.
- B13 `PUT memories/Some idea.md` with no `id` creates a memory titled
  `Some idea`. A second `PUT` to the same path updates that memory instead of
  creating a second one.

Renaming and deleting

- B14 `MOVE rules/a.md` → `rules/b.md` renames the rule through `rename_rule`.
- B15 `MOVE memories/Old.md` → `memories/New.md` changes the memory title.
- B16 `DELETE rules/a.md` deletes the rule through the same store call as the
  Command Center delete button.
- B17 `DELETE` or `MOVE` of `/dav/rules/`, `/dav/skills/`, `/dav/memories/`, or
  `/dav/` returns 403 and deletes nothing.

Obsidian and macOS housekeeping

- B18 `PUT /dav/.obsidian/app.json` stores the bytes in `dav_files`; `GET`
  returns them; they survive an `ax web` restart. `MKCOL` works under `.obsidian/`.
- B19 `PUT` of `._*` or `.DS_Store` anywhere returns success and stores nothing.
- B20 `OPTIONS /dav/` advertises `DAV: 1, 2`, and `LOCK` / `UNLOCK` succeed, so
  macOS mounts the share writable instead of read-only.

## Must not happen (invariants)

- N1 No note file is written to local disk by this feature. Rule and skill
  saves still write `.agents/` files exactly when they do today, because they go
  through the same `PolicyStore` calls. Nothing new is written there.
- N2 A malformed page never overwrites a stored rule, skill, or memory (B7, B8).
- N3 No path escapes its folder: `..`, encoded `%2e%2e`, and absolute paths return 400.
- N4 `/dav` sends no CORS headers, so a web page in your browser cannot read or
  write it. Test: a preflight with an `Origin` header gets no
  `Access-Control-Allow-Origin`.
- N5 When `ax web` is readonly (`AX_WEB_READONLY=1` or share mode), every
  write method on `/dav` returns 403.
- N6 A single `PUT` larger than 5 MB returns 413.
- N7 Existing `/api/*` routes, the Command Center, and the Getax plugin behave
  as before: the existing `ax-web` test suite passes unchanged.

## Failure model (Tier 3)

| Way this can hurt | Caught by |
|---|---|
| Bad frontmatter wipes a rule | B7, N2, mutation on the validation branch |
| Page named `a.md` claiming `id: b` clobbers rule `b` | B8 |
| Deleting a folder in Finder or Obsidian deletes every rule | B17 |
| Path traversal writes outside the vault | N3 |
| A website on your machine edits rules through the browser | N4 |
| Share mode exposes writes to the network | N5 |
| `workspace.json` rewritten constantly grows the database | `dav_files` is keyed by path (upsert, never append); N6 caps size |
| Obsidian's new-note flow loses typed text | B12 drafts keep every write |

Known limits, not solved here:

- Changes that ax makes itself (an MCP save, the Command Center, an index run)
  appear in Obsidian only after you reopen the note or the vault. A WebDAV mount
  does not send file-change events.
- Two editors saving the same rule: the last save wins. Each save is a
  revision, so the earlier text can be restored.
- A draft (B7, B12) is not visible as an error inside the editor; it is listed in `DRAFTS.md`.
- Code symbols are not pages (scope decision).

## Setup plan

- Isolation: git worktree `/Users/gary/io/ax-dav` on new branch `obsidian-webdav`
  from `HEAD`. Your working tree has uncommitted changes in
  `crates/ax-web/src/workspace_state.rs` and `lib.rs`; the worktree does not
  contain them. The final diff to those two files is a few lines (one router
  nest), applied to your tree after you review it.
- New dependency: `dav-server = "0.11"` in `ax-web`, default features off, no
  local-filesystem backend. Why: macOS only mounts a share writable if the
  server implements class-2 locking and exact PROPFIND responses; this crate
  implements those and lets the storage be our own SQLite code. Rejected: a
  hand-written WebDAV handler (more code to get macOS quirks right).
- New dev-dependency: `tower = { version = "0.5", features = ["util"] }` in
  `ax-web`, for request tests without opening a port. It is already in
  `Cargo.lock` through axum.
- New files: `crates/ax-web/src/dav/mod.rs`, `crates/ax-web/src/dav/fs.rs`
  (storage), `crates/ax-web/src/dav/pages.rs` (page text ↔ rows),
  `crates/ax-web/tests/dav_vault.rs`, a migration for table `dav_files`,
  `scripts/dav-mutants.sh` (manual mutation runner), and
  `docs/specs/obsidian-webdav-vault-EVIDENCE.md`.
- Docs: a section in `site/src/content/docs/guides/` explaining the three steps
  above (team rule: docs ship with features).
- Git: checkpoint commits on branch `obsidian-webdav` in the worktree only.

## Gauntlet

- `cargo test -p ax-web` (all existing tests plus `tests/dav_vault.rs`), run in random order via `--test-threads` shuffle.
- `cargo clippy -p ax-web -- -D warnings`, `cargo fmt --check`.
- Changed-line coverage with `cargo llvm-cov` if installed; otherwise recorded as skipped.
- Manual mutation (no Rust mutation tool in the repo): at least five mutants on
  the validation, id-match, folder-delete guard, path guard, and readonly
  guard. Each must be killed.
- `cargo audit` for the new dependency.
- Real execution: build the binary, run it on port 7071 against a copy of a
  project, mount with `mount_webdav`, edit a rule file with a shell editor,
  confirm through `/api/policy/rules`. Then open the mount in Obsidian and save a
  rule from the editor.

## Revisions (made during implementation, after approval)

Recorded here so the approved text above stays unchanged and every drift is visible.

1. **B11 default level.** A new rule created from a body-only page gets
   `level: INFO` and `triggers: ["<id>"]`. The spec said "normal"; the policy
   store only accepts `CRITICAL`, `WARNING`, `INFO`, and a rule needs
   `alwaysApply`, a glob, or a trigger to be valid.
2. **New skill from a body-only page** gets `description` = its name (the store
   requires a description). Not in the original scenario list; covered by
   `skill_page_round_trip_and_body_only_save`.
3. **N3 is strict:** an unparseable or traversal path (`/dav/../api/version`)
   returns **400** before reaching the WebDAV handler. It first returned 502.
4. **N5 lives in the request guard.** In readonly mode every method other than
   GET, HEAD, OPTIONS, PROPFIND returns **403** (dav-server's method filter
   would give 405). The filesystem-level check stays as a second line but is
   shadowed and not independently tested.
5. **Share-token gate added.** `/dav` sits outside the CORS layer, so it gets
   its own `share_token_middleware` layer. New test file
   `crates/ax-web/tests/dav_share.rs`.
6. **Protected folders are guarded at request level.** dav-server deletes a
   folder's children before the folder itself, so a filesystem-level refusal
   would come after data loss. DELETE / MOVE / COPY on the root or an area
   folder (source or `Destination`) returns 403 before any child is touched.
7. **Extra dependency edges:** `serde_yaml` and `bytes` in `ax-web` (both
   already in the workspace lockfile); `ax-policy` now exports
   `split_frontmatter`.
8. **Schema tripwires:** `crates/ax-db/tests/migration_v17.rs` to `v20.rs` pin
   `CURRENT_SCHEMA_VERSION`; updated from 20 to 21 for the new migration. Their
   assertions about their own migrations are unchanged.
9. **Gauntlet scope:** `cargo clippy -p ax-web -- -D warnings` cannot pass on
   this repo (pre-existing warnings outside the new code). The layer is held as
   "zero warnings in `crates/ax-web/src/dav/`", checked by
   `scripts/dav-gauntlet.sh`. `cargo fmt --check` is held on the new files only,
   for the same reason.
10. **Real execution** used `curl` against the release binary instead of
    `mount_webdav` from the agent shell: the macOS 27 beta WebDAV client hangs
    on `ls` there, and Apache `mod_dav` reproduces the same hang, so the
    Finder + Obsidian step is left to the user.
