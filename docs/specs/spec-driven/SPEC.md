# Spec-driven plans

Status: approved by the user on 2026-11-20. The acceptance rows below are the contract that was re-checked after implementation.

The spec is the agreement between the user, the agent, and Ax. Code that drifts from an approved spec raises a review. An approved spec that changes names the code and tests that must move. Neither side rewrites the other on its own.

## Storage decision

Two copies, one authority.

| Copy | Role |
| --- | --- |
| Project database `ax.db`, tables `specs` and `spec_revisions` | Authority for identity, revision number, status, approval, author, session, turn, content hash, and history |
| Markdown under the existing `docs/specs/<topic>/` tree, path stored on the spec row | Readable Git copy of the **approved** body |

Drafts, including every plan captured from plan mode, are rows in `spec_revisions`. They do not create a new documentation file. Approving a revision writes that body to the recorded Markdown path. A later edit of that file is a new draft whose hash is compared with the approved hash.

`docs/plans/` stays the place for the flat plans that already live there. A captured plan may point at one of those files. New product specs use `docs/specs/<topic>/`.

These are not the store:

- `.ax/plans/` is not created. A second directory would be a second current copy.
- `policy_revisions` is not reused. That table is for rules and skills and `list_revisions` returns twenty rows.
- Working-context notes are not the spec. Those notes are objective, facts, files, symbols, decisions, and open questions, and they are compacted.
- The memory vault is not the spec history. A spec revision is not a memory.

### Revision row

Each revision stores: spec id, monotonic version, status (`draft`, `in_review`, `approved`, `change_requested`, `rejected`, `superseded`), body, content hash, source (`plan_mode`, `file`, `turn`, `edit`), session id, turn id, created at, and the id of the revision it was based on.

History is not capped. The approved version stays approved until a newer revision is approved. Older approved bodies stay readable. A draft never becomes the version injected as current.

### Markdown names

Existing folders keep their names. `docs/specs/ax-neural/` already has `SPEC.md`, `SPEC-FULL.md`, `PLAN-AUDIT.md`, and `ARCHITECTURE.md`. `docs/specs/context-continuity/` uses descriptive filenames. The registry stores the path it was given. A new topic may start as `docs/specs/<topic>/SPEC.md`. `PLAN.md`, `TASKS.md`, and `DECISIONS.md` are optional attachments, not a required set.

### Conflict

If the file hash and the approved revision hash differ, the spec status for delivery is `conflict`. Preflight says so and names both hashes. Ax does not pick the file or the database as the new approved body. The user resolves it by approving a revision.

## What exists today

- Readable specs and plans already live in `docs/specs/` and `docs/plans/`. Nothing writes an agent plan into those trees automatically.
- Agent context for a turn is the policy inject (`format_inject_block`) plus `working_context_block`. Neither carries a spec id or a spec hash.
- Cursor hook state that Ax parses today is model and session identity. It is not a plan document.
- `policy_revisions` shows the shape of a versioned body in `ax.db`, and also the limit this spec must not copy: twenty rows, no approval status.

## Lifecycle

1. A finished plan is stored as `draft` v1 with its session and turn. A partial stream is not a revision.
2. Each later edit of that plan is a new version. The previous body stays.
3. Review sets `in_review`, `change_requested`, `rejected`, or `approved`. Approval is a person. Capture does not approve.
4. Implementation turns receive the approved version. The open draft is named beside it and is not treated as current.
5. A code change linked to the spec is checked against the approved body. A mismatch opens `change_requested` and a new draft. The approved body is left as it was.
6. A spec change lists the linked files and tests. It does not edit them.
7. A new chat on the same session receives the same approved version. An old session, opened later, can request the version that turn used.

## Context contract

Every agent turn bound to a spec receives a preflight section, separate from working context:

- spec id
- approved version and its content hash
- one-line objective
- the acceptance criteria that fit the budget
- constraints
- current phase
- open review points
- drift flag, when the code or the file disagrees with the approved hash
- a pointer that returns the full body of that version

The section names the version that was delivered. The agent does not treat an older body as current. When no spec is bound, the section says none.

Binding: the session has one active spec. Creating a plan binds that spec. A turn that names another spec id switches the binding and the next preflight shows the switch.

The context-export agent-ready document, when one is built for that session, includes the same pointer: spec id, approved version, hash. It does not paste the full spec by default. That export still does not claim a new chat is equivalent.

## Detection

Three routes, and a plan is stored only when one of them completes:

| Route | When it counts |
| --- | --- |
| Native event | The agent integration emits a finished plan document. Cursor hooks still do not emit that payload. A finished Markdown file under `.cursor/plans/` is stored as a draft instead. A file with fewer than two non-empty lines is not stored. |
| File | A Markdown file under `docs/specs/` or `docs/plans/` is written or changed, and the write is finished. |
| Turn | The user asked for a plan, and the turn has ended with a plan body. The word "plan" in a sentence is not a request. |

A streaming fragment is not stored. The stored body is the body present when the turn or the file write finishes. If that body is empty, no revision is written.

## Must not

- Mark a draft as the approved spec.
- Change an approved body because code, or an agent, diverged from it.
- Drop a revision to keep the table small.
- Publish two current bodies, one in the database and one in Git, without a `conflict` flag.
- Inline the full spec on every turn.
- Store a partial stream as a version.
- Commit the Markdown copy unless the user asks for a commit.

## Phases

1. Tables, hash, and conflict flag. No capture yet.
2. Capture from a finished turn and from a finished file write. Plan-mode native events when a payload exists.
3. Preflight section with the approved version and the pointer.
4. Drift record: code or file hash differs, new draft proposed, approved body unchanged.
5. Command Center page to list specs and open a revision. Search by project, session, status, and path.

## Acceptance

| Scenario | Expected | Check |
| --- | --- | --- |
| A finished plan request is stored | Draft v1 exists with session and turn. No approval. No new Markdown file. | `a_finished_plan_is_a_draft_and_an_edit_keeps_the_first_version` |
| The same plan is edited | v2 exists. v1 is still readable. | same test |
| The user approves v2 | v2 is the approved version. Its body is the file at the recorded path. v1 stays readable. | `approval_writes_the_markdown_and_keeps_the_previous_body` |
| A sealed file changes after approval | Approved body is unchanged. A `change_requested` draft records the hash change. Context sets `drift: yes`. | `a_linked_file_change_does_not_rewrite_the_approved_body` |
| The user edits the Markdown file | Context reports `delivery: conflict` and both hashes. The approved revision stays. | `an_edited_markdown_file_is_a_conflict` |
| A new turn is bound to the spec | Context includes id, approved version, hash, and pointer. | `a_bound_turn_can_show_the_version_it_received` and the preflight call in `tools.rs` |
| A turn mentions the word plan and does not ask for one | No revision is written. | `a_mention_of_plan_is_not_a_request` |
| A stream stops halfway | No revision is written. | `a_partial_stream_writes_no_revision` |
| History is opened on an old turn | The version recorded for that turn is the one returned. | `a_bound_turn_can_show_the_version_it_received`; `on_turn_finished` records the approved version for the turn |
| A finished plan file is on disk | The file is a draft. A one-line fragment is not stored. Importing the same file again does not add a revision. No Markdown copy is written before approval. | `a_finished_plan_file_is_a_draft_and_a_fragment_is_not` |
| An approved spec names a code file and that file changes | The named file is sealed at approval. The hash change is `change_requested`. The approved body stays. | `a_named_code_file_change_is_drift_and_the_approved_body_stays` |
| The Specs page is opened | The list shows path, project, session, and status, and a row opens the revision history. Approving a draft is a separate action. | `specsPage.test.ts` and `GET /api/specs` |
| An edit is saved from an open version | The save appends a draft. The open version, every approved row, and the Markdown file stay as they were. `based_on` is the version that was open. An empty body and a second identical body add no row. | `an_edit_appends_a_draft_and_leaves_the_approved_body` and `POST /api/specs/{id}/revisions` |
| The list is filtered | Kind is `plan` only when a revision came from `plan_mode` and nothing is approved. Search matches title, id, path, project, and session. An empty match can be cleared. | `list_specs` kind field and `specsPage.test.ts` |
| History is walked | Previous and Next open the stored body of that version. The page lists lines added and removed against the previous version. | `specsPage.test.ts` |

## Re-check after implementation

Command: `cargo test -p ax-usage --lib spec_registry` — 11 passed, including `an_edit_appends_a_draft_and_leaves_the_approved_body`.
Command: `cargo test -p ax-policy --lib ide_seed` — 24 passed, including `an_update_refreshes_every_ide_file_without_force`.
Command: `node --test src/pages/specsPage.test.ts` — 2 passed.

These parts of the spec are not in this build, so they are not counted as done:

- Cursor hooks still do not emit a plan-document payload. The stored substitute is a finished file under `.cursor/plans/`.
- Drift is a hash of a sealed file, including a code path named in the approved body when that file exists. Ax does not read the code and decide that it contradicts the prose.
- `ax upgrade` on macOS and Linux already runs `ax init --all`. Every later ax command in a project with a `.ax` directory refreshes managed IDE files when their Ax section differs, including after a Windows upgrade on the next command. A file the user removed stays removed.

## Open point

The truncated capture "silently rewrite an approved spec when code drifts" is the opposite of this draft and is not a rule to save. The rule, if saved later, is: never rewrite an approved spec because code drifted; record a draft and wait for review.
