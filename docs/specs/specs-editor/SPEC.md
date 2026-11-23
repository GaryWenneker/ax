# SPEC — Specs editor, version diff, and preflight plan

Tier 2. The specs page becomes the same kind of editor as rules and skills, version history gets a git-style diff, and a plan bound to the live session shows up in preflight.

## Goal

A person can open a spec or a plan, read it as Markdown, WYSIWYG, or a split of source and preview, edit it, and save the edit as the next draft. They can compare two versions as a unified diff. When a plan is bound to the live Cursor session, `ax_preflight` includes that plan in the spec section even if the ax chat id is different.

## Layout

The page is two columns. The list scrolls on the left. The open document scrolls on the right. Filters stay above the list.

Each row shows the title, the file path (or "No file"), and two outline badges: Plan or Spec, and Draft or Approved. Session ids are not written into the row. Logging and Memory links stay on the open document, one pair per bound session.

## Editor

The open document has four modes: Markdown, WYSIWYG, Split, and Diff.

- Markdown shows the source and is editable.
- WYSIWYG uses the existing rich-text editor and is editable.
- Split shows the source and the rendered preview side by side, with the same resize handle as the rule editor. The source side is editable.
- Diff is read-only.

Save is enabled when the editor text differs from the open revision. Save posts the existing revision endpoint with `basedOn` set to the open version and the editor text as `body`. That appends the next draft. It does not rewrite the open revision. Discard puts the open revision's body back. Approve stays available only for a clean draft.

An approved body can be edited immediately. There is no separate Revise step.

## Diff

Diff compares the open version with a chosen older version. The default base is the previous version. The control lists every older version.

The text is a unified diff:

- A hunk header `@@ -oldStart,oldCount +newStart,newCount @@`
- Up to 3 unchanged context lines around each change
- Removed lines prefixed with `-`, added lines with `+`, context with a leading space
- Identical bodies render the status "No changes from vN."
- A first version, with no older version, renders "No previous version."

Line order is preserved. A line that moves is a removal at the old place and an addition at the new place. Blank lines are part of the diff.

Example: before is `a\nb\nc\n` and after is `a\nB\nc\n`. The diff contains the hunk `@@ -1,3 +1,3 @@`, the context line ` a`, the removed line `-b`, the added line `+B`, and the context line ` c`.

## Kind stays with the source

A document that has any `plan_mode` revision keeps `kind: "plan"` after it is approved. A document created from a finished turn, with no `plan_mode` revision, stays `kind: "spec"`. The Plans filter still shows an approved plan. The Specs filter does not show it.

## Preflight

`spec_context_block(pool, root, session)` keeps the current result when `session` is bound.

When `session` is not bound, it tries the live session ids, in order: `.ax/turns/current.json` `session_id`, then `~/.ax/active-cursor-session` via `ax_utils::paths::home_dir()`. The first live id that is bound supplies the block.

The block for an unapproved plan is:

```text
spec: <id>
approved: none
pointer: ax spec show <id>
```

It does not include the plan body.

A session named `missing`, with no live session file and no binding, still returns `none`.

`ax_preflight` appends `<ax_section name="spec">` with that text on every successful preflight, including when the index fingerprint is unavailable and when the context cache is off. A failure inside the spec lookup still yields `none` and does not fail preflight.

## Must not change

- `POST /api/specs/{id}/revisions` and `POST /api/specs/{id}/approve` request and response fields stay as they are.
- Saving does not change an approved body in place and does not write a Markdown copy for an imported plan.
- Approving a draft still requires a clean editor.
- Plan import roots, the one-line fragment skip, and live binding only on the newest finished file under `~/.cursor/plans` stay as they are.
- Rules and skills keep Markdown, WYSIWYG, and Preview. Split is added beside those three; the stored default remains Preview.
- Logging and Memory links, and the specs list scrolling inside `.specs-page .column-list`, stay.
- No new package. No version bump. No commit unless asked. No Netlify deploy. Command Center is started only to verify the page, then left as the user last asked: stopped, unless they ask to keep it running.

## Tests

1. `unifiedDiff("a\nb\nc\n", "a\nB\nc\n")` contains `@@ -1,3 +1,3 @@`, ` a`, `-b`, `+B`, and ` c`.
2. `unifiedDiff("same\n", "same\n")` is an empty hunk list. The page copy for that case is "No changes from v1."
3. The specs page source includes the modes Markdown, WYSIWYG, Split, and Diff, uses `MarkdownEditor`, and still links Logging and Memory.
4. Approving a `plan_mode` document leaves `kind == "plan"`. The existing billing row created by `capture_finished_plan` stays `kind == "spec"`.
5. A finished plan under the temp home `.cursor/plans`, with `.ax/turns/current.json` session `cursor-live`, makes `spec_context_block` for session `axs-other` return the plan id, `approved: none`, and `pointer: ax spec show <id>`.
6. `spec_context_block` for session `missing`, with no live session file, stays `none`.
7. An `ax_preflight` call whose chat session is not bound, while the live session is bound to that plan, has `<ax_section name="spec">` containing the plan id. The section is present when the index fingerprint cannot be built.

## Setup

Work in `/Users/gary/io/ax`. This checkout is the one `scripts/reinstall-cli.sh` builds, so a separate worktree would not update the Command Center binary.

No new dependency. No git commit unless you ask for one.

Files the work may change:

- `crates/ax-web/web-ui/src/pages/Specs.tsx`
- `crates/ax-web/web-ui/src/pages/specsPage.test.ts`
- `crates/ax-web/web-ui/src/index.css`
- `crates/ax-web/web-ui/src/components/MarkdownEditor.tsx`
- `crates/ax-web/web-ui/src/components/PolicyBodyCard.tsx`
- `crates/ax-web/web-ui/src/lib/policyBodyView.ts`
- `crates/ax-web/web-ui/src/lib/policyBodyView.test.ts`
- `crates/ax-web/web-ui/src/lib/unifiedDiff.ts` (new)
- `crates/ax-web/web-ui/src/lib/unifiedDiff.test.ts` (new)
- `crates/ax-web/web-ui/package.json` test script, only to run the new test file
- `crates/ax-usage/src/spec_registry.rs`
- `crates/ax-mcp/src/tools.rs`

After the UI change, rebuild with `bash scripts/reinstall-cli.sh`, open the specs page, edit a plan in each mode, save, and open the diff. Then stop the listener on port 7070.

## Revision — sliding blade (requested after seeing the page)

The list rows fill the width: title, path, date, badges. The title and the path stay left aligned. The date is the latest revision time, or the time the document was stored when it has no revision. Newest is the default order. Oldest and Title are the other sort controls. Below 720px of list width the path moves under the title, the date sits at the end of that path line, and the badges stay on the right. The heading has no extra top margin.

The editor is a blade. It is closed until a row is opened. It slides in from the right with the same slide as the rule blade (`policy-blade-slide-in`), and slides back out on close. Close is the close button, Escape, a click outside the blade and the list, or a click on the open row. Opening another row keeps the blade open and changes the document. The four editor modes, save, discard, approve, and the diff stay inside the blade.

## Revision — session, version, and overlay

Each row has a version column (`v3 · draft · edit · based on v2`) and a session column. Every bound session in that column has Logging and Memory links. The blade overlays the list and leaves the start of the title visible. It does not squeeze the columns. The blade bar is one short row: the open version, previous and next, open and compare, the session links, the view, and save.

## Revision — columns, badges, and version chips

Logging and Memory are their own columns, like Version. The session id is not shown between the version and those buttons. Each bound session is a button in that column. The button text is Logging or Memory. The full id is the button title and accessible name. An empty column is a dim em dash. Clicking a button opens Logging or Memory for that id and does not open the blade.

Version, status, source, and based-on are outline badges in the Version column and in the blade bar. Kind is an outline badge in its own column.

The list has the shared column header. Title, Path, Version, Updated, and Kind sort and filter the loaded rows. Logging and Memory do not sort or filter. The default order, before that header is used, stays newest. The page filter bar keeps kind, status, and search, in separate groups with space between them. It does not repeat Newest, Oldest, and Title.

The list pane scrolls. The column header stays at the top of that pane. Rows load in windows of 20. A sentinel at the end of the loaded rows asks for the next window when it nears the pane.

The version badges stay on one line. The version column is wide enough for the version, status, source, and based-on badges side by side.

Specs sits between Skills and Memory in the sidebar. A plan's Logging and Memory buttons use the real session when one is bound, and the plan slug only when that is the only session. Memory lists the memories stored for that session. Logging, Memory, and the plan link to each other.

The blade no longer uses Previous, Next, or dropdowns. Open version is a row of square chips, one per revision, with the open revision pressed. Compare is a row of chips for the older revisions. The four modes, Save, Discard, and Approve stay. The blade still overlays from `left: 8rem` and does not squeeze the list.
