---
name: Link plans to sessions
overview: Import plans from every active workspace folder and from Cursor's user plan folder, bind each plan to a session, and carry that plan as a pointer when a logging session is copied.
todos:
  - id: scan-roots
    content: Import Markdown plans from workspace member folders and ~/.cursor/plans, without scanning the disk root.
    status: completed
  - id: bind-session
    content: Give every plan a session binding, including plan-<slug> and the live session for the newest user plan.
    status: completed
  - id: carry-pointer
    content: Add the spec pointer to the continuity prompt, the context-export JSON, and ax spec show.
    status: completed
  - id: click-through
    content: Link each plan session to Logging and Memory, and filter those pages from the query.
    status: completed
  - id: verify-plans
    content: Cover the import, binding, prompt, and export rules, then rebuild and confirm the open plan is listed.
    status: completed
isProject: false
---

# Link every plan to a session

The Specs page is empty because it only imports [crates/ax-usage/src/spec_registry.rs](crates/ax-usage/src/spec_registry.rs) from this repo's `.cursor/plans`, and that folder is empty. The open plan is `~/.cursor/plans/edit_spec_history_6f184f74.plan.md`. A copied continuity prompt names the session and does not name a plan.

The truncated capture `vanuit-logging-als-we-zo-session` is not saved. This plan is the requirement.

## Where plans are read

Scan these roots, and no others. `default_browse_roots` includes `/` and must not be used.

- Every workspace member from `member_roots` in [crates/ax-core/src/workspace.rs](crates/ax-core/src/workspace.rs). For this repo that is the project root. Recent workspace projects are included. Home and filesystem roots are not.
- Inside each of those folders: `.cursor/plans` and `docs/plans`, Markdown only, including `*.plan.md`.
- `~/.cursor/plans` as well. Cursor writes the live plan there, outside the workspace, which is why the open plan is missing today.

Skip `node_modules`, `target`, `target-dev`, and `.git`. Raise the 50-file stop so a full plans folder is stored. A one-line fragment is still ignored. Import stays a draft with source `plan_mode`. Nothing is approved, and no Markdown copy is written.

`file_stem` of `name.plan.md` is `name.plan`. Use the frontmatter `name:` as the title when it is present.

## A session on every plan

`spec_bindings` stays one spec per session. A spec may have several sessions. Every imported plan gets at least one row.

- A session id written in the file (`session:` or an `axs_…` id) is bound.
- Every file also gets a stable session `plan-<slug>`, so the plan view always has a link even when logging has no lines for it.
- The newest file in `~/.cursor/plans` is also bound to the current ax session in `.ax/turns/current.json` and to `~/.ax/active-cursor-session`, when those files exist. Older plans are not attached to the live chat.
- `save_revision` does not remove bindings. A list read backfills a `plan-<slug>` binding when a row has none.

## Copying a session takes the plan with it

The Logging copy button keeps the short continuity prompt. It does not switch to the seven-part export. Both the TypeScript prompt in [crates/ax-web/web-ui/src/lib/continuity.ts](crates/ax-web/web-ui/src/lib/continuity.ts) and `continuity_prompt` in [crates/ax-usage/src/continuity.rs](crates/ax-usage/src/continuity.rs) gain two lines when that session has a spec:

- `Spec: <id>@<version> <hash>`
- `Pointer: ax spec show <id>@<version>`

No approved or draft body is pasted in. A session with no spec says `Spec: none`. `equivalence_claimed` stays false.

The same pointer is added to the full-evidence and agent-ready JSON in [crates/ax-usage/src/context_export.rs](crates/ax-usage/src/context_export.rs), as a reference, not a copied body. A missing spec is not a completeness blocker. The published fidelity fixture is updated in the same change so the locked comparison still matches a fresh run.

`ax spec show <id>@<version>` prints that stored body. No new MCP tool. Preflight already has the spec section.

## From the plan to logging and memory

On [crates/ax-web/web-ui/src/pages/Specs.tsx](crates/ax-web/web-ui/src/pages/Specs.tsx) each bound session is a link to `/logging?session=<id>` and a link to `/memory?q=<id>`.

Logging reads `session` and shows only lines whose `session=` matches. An unknown id shows an empty log, not an error. Memory prefills its search with that id when `q` is present.

## Checks

- A temp workspace plans dir and a temp user plans dir are both imported. A one-line file is not. The open-plan shape (`name:` frontmatter, two or more lines) is stored and keeps a session.
- A second import of the same file does not add a revision.
- A plan with no session id in the file still has a `plan-<slug>` binding. The newest user plan is also bound to a supplied current session.
- The continuity prompt for that session contains the pointer and does not contain the plan body. An unbound session says `Spec: none`.
- The export JSON carries the same pointer. The fidelity file matches a fresh run.
- The Specs page source contains the logging and memory links.

Rebuild the embedded UI with `scripts/reinstall-cli.sh` and confirm the open plan appears on the Specs page, linked to its session.

Verification line.
