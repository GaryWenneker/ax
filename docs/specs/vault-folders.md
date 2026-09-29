# SPEC — Vault folders in Settings

Approval: the user approved the plan "Vault folders in Settings" with "Implement the plan as specified". This SPEC restates that plan as testable behaviors. Tier 3, because the change reads and writes real files outside ax.

## Behaviors

### Config (`.ax/vault-folders.json`, `crates/ax-web/src/vault_folders.rs`)
- C1: a list of `{ name, path, index }` round-trips through save and load. A missing file loads as an empty list.
- C2: a path that is relative, missing, or not a directory is rejected with a reason. The path is saved canonicalized.
- C3: a name that fails `valid_name`, or a name that appears twice, is rejected.
- C4: the API has `GET /api/vault/folders`, `PUT /api/vault/folders` (400 with a reason when invalid), `POST /api/vault/folders/{name}/sync` (404 for an unknown name) and `DELETE /api/vault/folders/{name}` (also removes that folder's `doc` memories). In read-only mode (`ax web --readonly`), every write route returns 403.

### DAV (`/folders/<name>/...`)
- D1: `/folders/` lists the configured folder names as directories.
- D2: reading `/folders/<name>/a.md` returns the bytes on disk, and listing a subdirectory shows its files and subdirectories.
- D3: writing a file creates or overwrites it on disk. Making a directory, deleting and renaming inside one folder work on disk.
- D4: `..` in a path, or a symlink inside the folder that points outside it, is refused (Forbidden). Nothing outside the root is read or written.
- D5: `/folders` and `/folders/<name>` themselves cannot be deleted or moved.
- D6: an unknown folder name is NotFound.
- D7: a write in a folder with indexing on triggers a re-sync of that folder.

### Memory sync (`crates/ax-memory/src/folder_sync.rs`)
- S1: `.md`, `.markdown` and `.txt` files become memories with kind `doc`, source `folder`, and the tag `folder:<name>`. The title is the first `# ` heading, or the file name when there is none. The body is the file text followed by `Source: folders/<name>/<rel>`. The id is `doc:<name>:` plus a hash of the relative path.
- S2: changing a file updates its memory in place (same id). Running a second sync with no changes reports 0 added, 0 updated and 0 removed.
- S3: a deleted file loses its memory. Memories from other folders are untouched.
- S4: hidden entries (a name starting with `.`), `node_modules` and `target*` are skipped. Files larger than 256 KB are counted as skipped. At most 2,000 files are imported per folder, and the rest are counted as skipped.
- S5: `remove_folder_memories(name)` deletes only that folder's `doc` memories.
- S6: `doc` memories are recall-only: they are left out of preflight injection, the preflight title list and memory export, the same way `turn` memories are.

### UI
- U1: Settings → Vault connection has a **Folders** list with, per folder, the name, the path, an **Index into memory** switch, the last sync result, **Sync now** and **Remove**, plus an **Add folder** button that opens a `ModalShell` form.
- U2: the Memory page shows a `doc` kind badge.

## Must not
- N1: write anything outside a configured folder root.
- N2: change existing vault areas (rules, skills, memories, global) or their tests.
- N3: add new dependencies (blake3 and serde are already present).

## Setup plan
- Isolation: none, work in the current tree (it already has uncommitted work that this builds on; a worktree would lack it). No commits.
- New files: `crates/ax-web/src/vault_folders.rs`, `crates/ax-web/src/dav/folders.rs`, `crates/ax-memory/src/folder_sync.rs`, `crates/ax-memory/tests/folder_sync.rs`, `crates/ax-web/tests/vault_folders.rs`, `crates/ax-web/web-ui/src/components/VaultFoldersCard.tsx`, `crates/ax-web/web-ui/e2e/vault-folders.spec.ts`, `scripts/vault-folders-mutants.sh`, plus this SPEC and its EVIDENCE file.

## Revision 1 (from review round 1; added, not approved separately)
- C5: every folder API route, including GET, returns 403 unless the request comes from the local browser (loopback `Host`, and an `Origin` that is loopback or absent). The folder API sits behind the permissive CORS layer, so without this a foreign web page could add `$HOME`, index it, and read it back through recall.
- D8: `/dav/folders/...` gets the same local-only check for reads and writes, including a MOVE or COPY whose `Destination` is inside `folders/`. Share sessions and other devices never see folders.
- C6: saving the folder list re-syncs only folders that are newly indexed or whose path changed. **Sync now** still syncs on demand.
- U3: the Add folder modal links its hints and error to the inputs (`aria-describedby`), and errors use `role="alert"`.
- Deviation from U2: the Memory page has a `doc` category color and label ("Folder doc") and rows show the `folder:<name>` tag, but no separate filter chip.
