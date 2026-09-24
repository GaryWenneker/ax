# DESIGN SPEC: turn memories with outcomes, recall while working, and "when did I change X"

**Status:** Revision 1, approved ("Approve Revision 1, build it on branch feat/turn-memory-outcomes").

## Revision 1 (your answers folded in)

- **Outcome:** the full final reply is stored, not the first 600 characters, with secrets redacted. A 20,000-character safety cap still applies so one huge reply cannot bloat the database. Behavior O3 changes to match.
- **Recall:** at most 3 related turns in preflight, matched on open files or the prompt. This is unchanged from the draft.
- **Retention:** 90 days instead of 30.
- **New: backup before pruning.** Before turns older than 90 days are deleted, they are appended to `.ax/backups/turn-memories-YYYY-MM-DD.jsonl` in the project. Nothing is deleted unless that write succeeded. New behaviors P1 to P3 cover this.
**Tier:** 2. It changes what ax stores per turn and what preflight gives back to the agent.

## Revision 1a (found while checking the hook docs and code, before any implementation)

1. **Claude Code outcome source.** The `Stop` payload has a `last_assistant_message` field. The docs say `transcript_path` "may not yet include the current turn's most recent messages" and point to that field instead. ax uses `last_assistant_message` and does not read the transcript. Behavior O2 is unchanged.
2. **Cursor.** `afterAgentResponse` gives `{ "text": … }` and fires after every assistant message in a turn. ax keeps the latest one, redacted, in the turn snapshot. The `stop` hook uses it. Known limit: if Cursor ran `stop` before the last `afterAgentResponse` finished, the outcome would be the previous message.
3. **New behavior O6, a v5.1.0 bug.** Claude Code's `SubagentStop` runs the same `ax stop-hook`. So a subagent finishing mid-turn already writes the turn memory. The real `Stop` is then ignored, because it has the same id. With outcomes, the subagent's report would become the turn's outcome. Fix: `SubagentStop` no longer ends the turn. The policy guard on `SubagentStop` stays as it is.
4. **"Matches the prompt above a score threshold" (R1), made concrete.** The recall scores are rank-based and differ between the hash and ONNX embedders, so a numeric threshold would be arbitrary. A turn matches the prompt when it is in the recall results **and** shares at least 2 distinct words of 4 or more letters with the prompt (case-insensitive, common stop words ignored).
5. **Dates** are shown in local time as `YYYY-MM-DD HH:MM`, using `chrono`. `chrono` is already in `Cargo.lock` (used by ax-usage, ax-web, ax-ship), so it adds no new package.
6. **`ax_history` output.** Each entry shows the outcome's first 600 characters, followed by the memory id. `ax_history` with `id` (CLI: `ax history --id <id>`) returns that one turn with the full outcome. The full text, up to 20,000 characters, stays in the database. New behavior H4 covers this.
7. **Existing Cursor installs** get the new `afterAgentResponse` hook when `ax install` runs again. The memory guide says so.

| # | Given | When | Then |
|---|---|---|---|
| O6 | Claude Code `SubagentStop` with changed files | stop-hook | no turn memory is written; the next `Stop` writes it, with the main agent's `last_assistant_message` as the outcome |
| H4 | a turn with a 5,000-character outcome | `ax_history` lists it, then `ax_history id=<id>` | the list shows 600 characters and the id; the id call shows all 5,000 |

## Revision 1b (found during implementation; no behavior row changes)

1. **Free-text `ax_history`** (H1 with a topic instead of a file or symbol) keeps a recalled turn when it shares at least 1 word of 4 or more letters with the query. Preflight (R1) still needs 2. A short question like "when did I change the language dropdown" should find the turn about that dropdown, and it only runs when asked.
2. **Open files in preflight** are made project-relative before matching: an absolute path under the project root is stripped of the root, a path that only matches after resolving symlinks (for example `/var` and `/private/var` on macOS) is canonicalized first, and backslashes become `/`. Without this, R1 on file match would never fire for the absolute paths editors send.
3. **The CLI shows the full outcome only with `--id`**, like the MCP tool. An earlier draft showed it for any list with one entry.
4. **Mutant scripts** run only the ax-mcp library tests and `catalog_payload_size`. `tests/new_tools_smoke.rs` opens the repo's own `.ax/` and fails in a fresh worktree before and after this change. They also restore files with git hooks disabled, because ax's `post-checkout` hook exits nonzero in a tree without `.ax/`.
5. **Backups and git.** `ax init` does not add `.ax/` to `.gitignore`, so `.ax/ax.db` and the new `.ax/backups/` are both untracked (not ignored) in a fresh project. That was already true for the database and is out of scope here. The gauntlet checks that the backup is never more exposed than `ax.db`, and the finding is reported separately.

## Current state (v5.1.0)

- A turn memory holds the prompt (first 300 characters, redacted), the files changed, and the commits made.
- It does not hold what the agent answered or concluded.
- Turn memories are recall-only: preflight never gives them back, so an agent working on the same feature later does not see them.
- There is no way to ask "when did I change X". `ax_recall` ranks by text similarity, not by file or date, and its results show no dates.

## Goal

1. Each turn memory also stores the **outcome**: what the agent reported at the end of the turn.
2. When you work on something you touched before, the agent **gets the related past turns back** in preflight.
3. A question like "wanneer heb ik de review-taal aangepast?" gets an answer with **dates and actions** (prompt, outcome, files, commits).

## How it works

### 1. Outcome

- **Cursor:** a new `afterAgentResponse` hook (`ax turn-hook response`) stores the agent's reply text in the turn snapshot. The turn end copies it into the memory.
- **Claude Code:** the `Stop` hook reads the last assistant message from the `transcript_path` it receives.
- The memory body gets an `Outcome:` section. It holds the first 600 characters of the final reply, with secrets redacted the same way as the prompt.
- The hook payload fields are checked against the Cursor and Claude Code hook docs during RED. If a field is missing, this spec is revised visibly first.

### 2. Related past turns in preflight

Preflight adds a small `<ax_turn_history>` block with at most 3 past turns. A turn is related when either holds:

- it changed a file that is open or passed in `files`;
- its prompt or outcome matches the current prompt above a score threshold.

Each entry is one line: date, prompt title, outcome (first 120 characters), and up to 3 files. The block is at most 1,200 characters. It only lists turns from this project.

This reverses the earlier "recall-only" choice from the per-turn memory spec. Export stays off. Retention goes from 30 to 90 days, with a backup before pruning.

### 3. "When did I change X"

New MCP tool `ax_history` and CLI `ax history`:

- **Input:** a file path, symbol name, or free text, plus an optional `since` date.
- **Output:** turn memories and git commits that touched it, newest first. Each entry shows date and time, the prompt, the outcome, the files, and the commit subjects.
- **Sources:** a path or symbol matches the files stored on turn memories, and `git log` for that path. Free text uses the existing recall.
- Preflight recognises "wanneer heb ik", "when did I", "wat heb ik aangepast aan" and tells the agent to call `ax_history`.

## Behaviors

| # | Given | When | Then |
|---|---|---|---|
| O1 | Cursor turn whose reply is "Fixed the dropdown, 11 languages now load." | turn end | the memory body has `Outcome: Fixed the dropdown, 11 languages now load.` |
| O2 | Claude Code turn with a transcript | `Stop` | the outcome is the last assistant message |
| O3 | a 5,000-character reply | turn end | the full reply is stored; a reply over 20,000 characters is cut at 20,000 with `[truncated]` |
| O4 | a reply containing `ghp_…` | turn end | it is stored as `[redacted]` |
| O5 | no reply was captured | turn end | the memory is written without an Outcome section, exactly as today |
| R1 | a past turn changed `Settings.tsx`, and `Settings.tsx` is open now | preflight | `<ax_turn_history>` lists that turn with its date and outcome |
| R2 | no related turn | preflight | there is no `<ax_turn_history>` block |
| R3 | 10 related turns | preflight | 3 are listed, newest first, and the block is at most 1,200 characters |
| H1 | a turn and a commit changed `review_language.rs` | `ax_history review_language.rs` | both are listed, newest first, with date and time |
| H2 | the prompt "wanneer heb ik de review-taal aangepast?" | preflight | the inject tells the agent to call `ax_history` |
| H3 | `since=2026-09-24` | `ax_history` | older entries are not listed |
| P1 | a turn memory 91 days old | prune | it is appended to `.ax/backups/turn-memories-<today>.jsonl` as one JSON line, then deleted |
| P2 | a turn memory 89 days old | prune | it is kept and not backed up |
| P3 | the backup file cannot be written | prune | nothing is deleted, and the error is logged |

## Must not change

- Turns that change nothing still write no memory.
- Turn memories are still never exported (the backup file is local, not an export).
- The hooks still never block a turn or print anything.
- Other memory kinds and the existing preflight blocks.

## Setup

- **Isolation:** branch `feat/turn-memory-outcomes` from `main`.
- **Files:** `crates/ax-cli/src/commands/turn_hook.rs`, `crates/ax-cli/src/commands/stop_hook.rs`, `crates/ax-memory/src/turns.rs` plus a history query, `crates/ax-mcp/src/tools.rs` (preflight block, `ax_history`), `crates/ax-installer/src/hooks.rs` (Cursor `afterAgentResponse` entry), CLI `ax history`, and the docs `guides/memory.md` and `reference/cli.md`.
- **Dependencies:** none new.
- **Git:** commit this spec at approval, then the implementation. It ships in the next minor release.
