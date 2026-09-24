# DESIGN SPEC: automatic per-turn memory

**Status:** Revision 1 APPROVED ("Approve Revision 1 (build it on a branch after v5.0.3, ships in v5.1.0)", 2026-09-24).

## Current state (checked 2026-09-23)

Memories are written by only three paths:

1. `ax_remember` / `ax remember`, when an agent or the user calls it explicitly. This project has 8 such memories in the last month.
2. The git post-commit / post-merge hooks (`ax capture-git`), one memory per commit. Broken on macOS since 2026-08-26, fixed by `git-hook-exec.md`.
3. `ax memory import`.

Nothing runs at the end of a turn in Cursor. `ax stop-hook` is only installed for Claude Code (`Stop` / `SubagentStop`), and it does not write memories either: it stores the transcript tail in the context cache (`usage.db`) and runs the CRITICAL policy guard on changed files.

## Goal

After each agent turn in which something durable happened, a memory exists without anyone having to remember to call `ax_remember`.

## Options

| | What is saved | How | Pros | Cons |
|---|---|---|---|---|
| A | A deterministic turn record: user prompt (first 300 chars), files changed this turn, commits made this turn | Cursor `stop` hook + Claude `Stop` hook call `ax stop-hook`, which writes a `turn` memory | No LLM, cheap, always runs | Noisy: most turns are not durable; the prompt alone rarely says *why* |
| B | Nothing directly; the hook asks the agent to save | Stop hook returns a follow-up message ("call ax_remember if this turn produced a decision, fix, or convention") | The agent writes the *why* | Costs an extra agent step per turn; the agent may still skip it; risk of a follow-up loop |
| C | A + pruning | As A, but only when files changed or a commit was made, deduplicated per session, with `turn` memories excluded from preflight injection unless recalled | Low noise in preflight, full history in recall | More moving parts |

Recommendation: **C**. It always records, keeps preflight clean, and needs no LLM.

## Open questions for you

1. Which option (A, B, C, or another)?
2. Should every turn be recorded, or only turns that changed files or made a commit?
3. Should `turn` memories be injected into preflight, or only found through `ax_recall`?
4. How long are they kept (forever, 30 days, the last N per project)?
5. Should the Cursor `stop` hook be installed by `ax install` for everyone, or be opt-in via `ax.json` (`memory.perTurn: true`)?

## Revision 1 (2026-09-24): your answers folded in

**Answers:** option C; only turns that changed files or made a commit; recall-only (never injected by preflight); kept 30 days; installed by default, switched off with `ax.json` `memory.perTurn: false`.

**Tier:** 2. It adds a hook that runs after every agent turn, and it writes the user's prompt text into a database.

### How a turn is measured

Cursor's `stop` hook does not receive the prompt, and "uncommitted files" would count the same dirty files on every turn. So the turn is bracketed by two hooks:

- **Turn start** (`beforeSubmitPrompt`, Cursor; `UserPromptSubmit`, Claude Code): `ax turn-hook start` saves a small snapshot per conversation in `.ax/turns/<conversation>.json`: the prompt (first 300 characters), `HEAD`, and a content hash of every dirty file.
- **Turn end** (`stop`, Cursor; `Stop`, Claude Code, inside the existing `ax stop-hook`): compares against the snapshot. Files changed this turn are files whose hash differs from the snapshot, plus files that became dirty. Commits this turn are `snapshot HEAD..HEAD`.

### Behaviors (each becomes a test)

| # | Given | When | Then |
|---|---|---|---|
| T1 | a snapshot, and the turn edited `src/a.rs` | turn end | one memory: kind `turn`, source `turn-hook`, title = first 80 characters of the prompt, body = prompt (300 characters), files changed, commit subjects |
| T2 | a snapshot, and nothing changed, no commit | turn end | no memory written |
| T3 | `src/a.rs` was already dirty before the turn and the turn did not touch it | turn end | `src/a.rs` is not listed; no memory if nothing else changed |
| T4 | the turn made a commit and left the tree clean | turn end | memory lists the commit subject(s) and the files of those commits |
| T5 | the same turn end is delivered twice (retry) | turn end twice | one memory (id derived from conversation id + turn number) |
| T6 | no snapshot (hook installed mid-turn, or start hook failed) | turn end | no memory, exit 0, nothing printed |
| T7 | `ax.json` has `memory.perTurn: false`, or `AX_NO_STOP_HOOK=1` | either hook | nothing written, exit 0 |
| T8 | `turn` memories older than 30 days exist | any turn end that writes | they are deleted; other kinds are never touched |
| T9 | a `turn` memory matches the prompt best | `ax_preflight` | it is not in `memories`, not in the inject block, and not in the memory titles list |
| T10 | the same memory | `ax_recall` / `ax recall` | it is found |
| T11 | `turn` memories exist | `ax memory export` | they are not exported (local only; the export file is shared through git) |
| T12 | the prompt contains a secret (`sk-…`, `ghp_…`, `AKIA…`, `password=…`, a 40+ character hex or base64 run) | turn end | it is stored as `[redacted]` |
| T13 | `ax install` for Cursor and Claude Code | install | the turn-start and turn-end hook entries are added once; reinstall changes nothing; uninstall removes only ours; user hook entries are kept (same rules as the read-guard entries) |
| T14 | any failure inside the hooks (no git, no ax project, locked db) | either hook | exit 0, nothing on stdout; a turn is never blocked by memory capture |
| T15 | Claude Code `Stop` with a CRITICAL policy violation | turn end | the existing policy block still happens exactly as today; the memory is written as well |

### Must not change

- Existing Claude `Stop` behavior (policy guard, transcript tail) and its tests.
- Preflight output for projects without `turn` memories.
- Other memory kinds: recall, injection, export, decay.

### Known limits

- The memory records *what* changed and the prompt, not *why*. The why still needs `ax_remember`.
- Edits made outside the agent during a turn (you typing in the editor) are counted as part of that turn.
- Cursor hook input fields (conversation id, turn id) are checked against Cursor's hook docs during RED. If a field is missing, the spec is revised visibly before coding around it.

### Revision 1a (2026-09-24, during RED): hook input fields checked

Checked against https://cursor.com/docs/agent/hooks. Both Cursor events (`beforeSubmitPrompt`, `stop`) receive `conversation_id`, `generation_id` (changes with every user message) and `workspace_roots`; only `beforeSubmitPrompt` has `prompt`. Claude Code sends `session_id`, `cwd` and (`UserPromptSubmit` only) `prompt`, with no turn id. So the "turn number" in T5 is: Cursor's `generation_id`, or for Claude Code a counter the start hook increments per conversation. The start hook stores it in the snapshot and the end hook takes it from there, so both deliveries of one turn end produce the same memory id. The project root is `workspace_roots[0]` (Cursor) or `cwd` (Claude Code). No behavior changes.

### Revision 1b (2026-09-24, found by the gauntlet): ax's own files are not turn changes

In a project that does not gitignore `.ax/`, ax's database (`.ax/ax.db`, `-wal`, `-shm`), its logs and the turn snapshots show up as dirty files, and SQLite rewrites them during most turns. Counting them would record almost every turn, which breaks T2. So files under `.ax/` are not counted as turn changes, except the shareable `.ax/policy/` and `.ax/memory/` folders, which an agent can really edit. Test: `ax_runtime_files_are_not_changes_but_shared_ax_files_are`. No other behavior changes.

### Setup plan

- **Isolation:** branch `feat/per-turn-memory` from `main` after v5.0.3 is merged.
- **Files:** `crates/ax-cli/src/commands/turn_hook.rs` (new), `crates/ax-cli/src/commands/stop_hook.rs`, `crates/ax-cli/src/main.rs` (hidden `turn-hook` command), `crates/ax-memory/src/store.rs` (kind filter, 30-day prune), `crates/ax-memory/src/sync.rs` (export filter), `crates/ax-mcp/src/tools.rs` (preflight filter), `crates/ax-installer/src/hooks.rs` and `targets.rs` (hook entries), docs `site/src/content/docs/guides/memory.md` and `reference/cli.md`, `scripts/gauntlet-per-turn-memory.sh`.
- **Dependencies:** none new (`blake3` for the file hashes and `serde_json` are already workspace dependencies).
- **Git:** commit this revision at approval, then fix commits on the branch; ships in the next minor release (v5.1.0), not in v5.0.3.
