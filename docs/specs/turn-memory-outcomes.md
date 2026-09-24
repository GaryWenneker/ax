# DESIGN SPEC: turn memories with outcomes, recall while working, and "when did I change X"

**Status:** Revision 1, approved ("Approve Revision 1, build it on branch feat/turn-memory-outcomes").

## Revision 1 (your answers folded in)

- **Outcome:** the full final reply is stored, not the first 600 characters, with secrets redacted. A 20,000-character safety cap still applies so one huge reply cannot bloat the database. Behavior O3 changes to match.
- **Recall:** at most 3 related turns in preflight, matched on open files or the prompt. This is unchanged from the draft.
- **Retention:** 90 days instead of 30.
- **New: backup before pruning.** Before turns older than 90 days are deleted, they are appended to `.ax/backups/turn-memories-YYYY-MM-DD.jsonl` in the project. Nothing is deleted unless that write succeeded. New behaviors P1 to P3 cover this.
**Tier:** 2. It changes what ax stores per turn and what preflight gives back to the agent.

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
