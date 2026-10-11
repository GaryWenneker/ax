# SPEC: Restore package — per-change (hunk) Accept

**Tier:** 2 (writes team policy files; merge of selected hunks)  
**Spec approval:** approved 2026-09-03 (user: "approve")  
**Isolation:** current branch. **No new cargo/npm dependencies.**

Absolute path: `/Users/gary/io/ax/docs/specs/policy-zip-hunk-restore.md`

## Problem

Restore **Accept** / **Reject** applies to the **entire** rule or skill file. The inspect pane shows one unified diff. There is no way to take some package edits and leave other local lines.

The screenshot (Kaleidoscope / Fork-style) is the **interaction model**: jump between changes, take some, leave others. It is **not** a pixel clone (no file tree, no git-branch chrome, no bezier connectors).

## Today (must survive)

- Per-item **Reject** = skip write; **Accept** = write the full package file.
- New items: still whole-file Accept (no local hunks).
- Identical / invalid: unchanged.
- Skill extra files (not `SKILL.md`): still whole-file only.
- Existing `decisions` JSON `"rule:<id>" | "skill:<name>"` → `"overwrite" | "skip"` still works.
- Cursor-native skip-on-index, compact empty restore modal, drag-and-drop zip: unchanged.

## Behaviors

### H1 — Split the existing LCS diff into hunks

Given local `a\nb\nc\n` and package `a\nX\nc\n`  
When hunks are built from the same LCS used by `unified_diff`  
Then there is **one** hunk whose package side is `X` and local side is `b`. Unchanged lines are context, not selectable hunks.

Consecutive `+`/`-` lines form one hunk. Isolated changes far apart are separate hunks.

The current single `@@ -1,n +1,m @@` blob is **not** one selectable unit when there are two separate edits.

### H2 — Diff pane: select hunks

After clicking a **changed** row, the inspect pane lists hunks with:

- English **Accept** / **Reject** on **that hunk** (Accept = take package lines for that hunk).
- Footer: `Change N of M` with previous/next (same idea as the screenshot).
- Selecting a hunk scrolls it into view.

Default for a changed file: every hunk **Rejected** (matches file-level Reject / local-newer default). File-level **Accept** selects every hunk. File-level **Reject** clears every hunk. Mixed selection shows **Partial** in the Action column (not a third combo; file buttons stay Accept/Reject plus a Partial badge).

### H3 — Restore writes the merge

Given a conflict with two hunks, user Accepts hunk 0 and Rejects hunk 1  
When they click **Restore**  
Then the written file is local text with **only** hunk 0 replaced by the package side. Hunk 1 stays local.

Empty Accept set on a conflict = skip (same as Reject).

All hunks Accepted = same bytes as today’s overwrite.

New items with Accept = full package file (no merge).

### H4 — API (backward compatible)

`POST /api/policy/package/restore` `decisions` JSON:

- Unchanged: `"rule:id": "overwrite" | "skip"`
- New: `"rule:id": { "action": "merge", "acceptHunks": [0, 2] }` (0-based hunk indexes)

Unknown indexes → 400. CLI `--decisions` accepts the same object.

No new HTTP path. Zip format unchanged.

### H5 — Visual (v1, in the existing inspect pane)

Keep the restore table. In the inspect pane, render each hunk as a **compact two-column** strip: left = local, right = package, with add/del highlighting (English labels). Unified dump remains available as fallback when there are zero hunks but bytes still differ (line endings).

Out of scope for v1: full-window side-by-side, file list sidebar, overlay mode, connecting ribbons.

### H6 — Line numbers (follow-on, 2026-09-03)

Each hunk column shows 1-based source line numbers (`localStart` / `packageStart` on diff JSON). Insert-only or delete-only sides use `0` (blank gutter).

### H7 — Local / Package checkboxes, including both (follow-on, 2026-09-03)

Per hunk: **Local** and **Package** checkboxes (English). Default: Local on, Package off. Both on writes local lines then package lines. Both off omits the hunk. File Accept = all Package; file Reject = all Local.

API additive: `{ "action": "merge", "hunks": [{ "index": 0, "take": "local"|"package"|"both"|"none" }] }`. Legacy `acceptHunks` still means package for those indexes.

**Follow-on request:** 2026-09-03 (user: line numbers and checkboxes for old/new/both).

### H8 — Larger inspect modal; Old / New above the code (follow-on, 2026-09-03)

Restore with a loaded preview uses ModalShell size `full` (`calc(100vw - 32px)`, max 1760px, height `calc(100vh - 24px)`). Empty restore stays `md`. Compose stays `xl`.

When a row is selected, the file list is a fixed ~22–26rem column; the inspect pane takes the rest. Each hunk column heading above the code is **Old** + `local file` (left) and **New** + `package` (right). Checkboxes remain for take-local / take-package.

Compare in the table is hidden while inspecting (status stays in the inspect title) so Kind/Id/Action stay one line. Id uses ellipsis, never `overflow-wrap: anywhere`.

Must not: Dutch UI; shrink compose; change merge semantics; letter-by-letter wrapping of ids.

## Must not

- Dutch UI.
- New npm/cargo deps.
- Change pack format / `formatVersion`.
- Partial merge of binary skill extras.
- Auto-Accept hunks on local-newer files.

## Setup

| Item | Path |
|------|------|
| Spec | `/Users/gary/io/ax/docs/specs/policy-zip-hunk-restore.md` |
| Engine | `crates/ax-policy/src/zip_package.rs` |
| UI | `crates/ax-web/web-ui/src/policyPackage.ts`, `PolicyZipPackageModals.tsx`, `index.css` |
| HTTP | existing restore multipart `decisions` |
| Gauntlet | `tools/gauntlet-policy-zip-package.sh` |
| Docs | `site/.../policy-engine.md`, `reference/cli.md` |

## Mapping (tests after approval)

| Behavior | Test |
|----------|------|
| H1 | `split_lcs_into_hunks_separates_distant_edits` |
| H3 | `merge_accepts_first_hunk_keeps_second_local`, `restore_merge_accepts_first_hunk_keeps_second_local` |
| H4 | `restore_merge_string_overwrite_still_full_file`, `restore_merge_json_roundtrip`, `restore_merge_invalid_hunk_index_errors` |
| H2 | `restoreFileActionLabel is partial when some hunks are accepted`, `changeNavLabel is 1-based Change N of M` |
| H6 | `split_lcs_into_hunks_separates_distant_edits` (`local_start` / `package_start`) |
| H7 | `merge_accepts_first_hunk_keeps_second_local` (both), `hunkTakeFromChecks` |
| H8 | `tools/gauntlet-policy-zip-package.sh` greps `full`, `.ax-modal--full`, `>Old<` / `>New<`, inspect list `minmax(22rem, 26rem)`, Compare hidden, Id not `overflow-wrap: anywhere` |

## Calibration

Tier 2. Failure modes: wrong hunk index, merge that drops YAML frontmatter — tests use full `.mdc` files, not snippets.
