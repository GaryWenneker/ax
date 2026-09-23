# Read guard hook: steer Read/Grep on indexed source back to the graph

Status: APPROVED rev 2 ("Approve, build it", 2026-09-23); rev 3 below is a post-approval correction found in real execution and still needs your review. Decisions (2026-09-23): deny the first attempt and allow an identical retry; inspect shell commands (first simple command only); install for every IDE that supports blocking tool hooks (Cursor, Claude Code + VS Code Copilot, Gemini CLI, Windsurf, Codex). Tier 3: it gates tool calls of every agent in every IDE it is installed in.
Isolation: the existing checkout, as with the previous graph-over-grep change (the change is the product). Veto if you want a branch.
New dependencies: none (serde_json, sqlx, and dirs are already in the workspace).

## Problem

The rules tell agents to use `ax_explore` / `ax_node` before Read/Grep, but nothing enforces it. Agents still open whole source files and grep for symbol names the graph already indexes.

## Command

Hidden subcommand `ax read-guard --ide <cursor|claude|gemini|windsurf|codex>`. It reads the hook JSON on stdin and writes the IDE's decision format.

## Behaviors

1. **Unknown tools pass fast.** A tool that is not a read, search, or shell tool is allowed before any file or database access. This matters for VS Code, which ignores matchers and runs the hook on every tool.
2. **Outside ax passes.** If no `.ax/ax.db` is found from the target path (or the payload cwd) upward, the call is allowed.
3. **`AX_READ_GUARD=off` passes everything.**
4. **Whole-file read of indexed source, first time → deny.**
   - The file is in the index and has at least one symbol node.
   - The Read has no offset or limit.
   - This `(conversation, file)` pair has not been denied before.
   - The deny message lists up to 8 symbols in that file (`name kind start-end`). It says: `ax_node <name>` returns the full source, `ax_explore` answers how-questions, and "retry the same Read if you need the exact text to edit".
5. **Same read again in the same conversation → allow.** No deadlock before edits.
6. **Partial reads pass.** A Read with offset or limit, `sed -n a,bp`, or `head/tail -n N` is allowed: the agent already targets lines.
7. **Non-source files pass.** Files not in the index, or with no symbols (README, Cargo.toml, lockfiles, logs, generated output), are allowed.
8. **Symbol search, first time → deny.**
   - A Grep or shell `rg`/`grep` whose pattern is a bare identifier (after stripping `\b`, `-w`) and matches at least one graph node name is denied the first time per `(conversation, pattern)`.
   - The deny message lists up to 8 graph hits (`qualified name — file:line`) and suggests `ax_callers` / `ax_node`.
   - A retry of the same pattern is allowed.
9. **Other searches pass.** Regex, string-literal, or no-graph-hit searches are allowed.
10. **Shell parsing is minimal.** Only the first simple command of a shell line is inspected:
    - `rg`/`grep`/`ag`/`ack` count as a search.
    - `cat`/`bat`/`less`/`more` on a file count as a whole-file read.
    - Anything else (pipelines after the first segment, `&&`, subshells) is allowed.
11. **Conversation key.** The key is the first non-empty of `conversation_id`, `session_id`, `trajectory_id`, or `turn_id`, else `anon`. Denials are remembered for 4 hours in `~/.ax/read-guard.json` (capped at 2000 entries, oldest dropped).
12. **Fail open.** Any error allows the call (exit 0, no deny output): bad stdin, unreadable database, or state file I/O. A hook must never break the agent.
13. **Output per dialect:**
    - **cursor:** deny = `{"permission":"deny","user_message":<short>,"agent_message":<full>}`; allow = `{"permission":"allow"}`.
    - **claude** (also used by VS Code Copilot, which reads `~/.claude/settings.json`) and **codex:** deny = `{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":<full>}}`; allow = no output.
    - **gemini:** deny = `{"decision":"deny","reason":<full>}`; allow = no output.
    - **windsurf:** deny = message on stderr, exit 2; allow = exit 0.
14. **Tool names recognised.**
    - **Read:** `Read`, `read_file`, `readFile`, `view`, Windsurf `pre_read_code`.
    - **Search:** `Grep`, `grep`, `grep_search`, `search_file_content`.
    - **Shell:** `Shell`, `Bash`, `run_shell_command`, `run_in_terminal`, Windsurf `pre_run_command`.
    - **Path fields:** `path`, `file_path`, `target_file`, `absolute_path`, `filePath`, `tool_info.file_path`.

## Install (user-level config, merged; unrelated hooks preserved; idempotent; uninstall removes only ax entries)

| IDE | File | Event / matcher |
|---|---|---|
| Cursor | `~/.cursor/hooks.json` | `preToolUse`, matcher `Read\|Grep\|Shell`, timeout 5 |
| Claude Code (+ VS Code Copilot) | `~/.claude/settings.json` | `PreToolUse`, matcher `Read\|Grep\|Bash` |
| Gemini CLI | `~/.gemini/settings.json` | `BeforeTool`, matcher `read_file\|search_file_content\|grep\|run_shell_command` |
| Windsurf | `~/.codeium/windsurf/hooks.json` | `pre_read_code`, `pre_run_command` |
| Codex | `~/.codex/hooks.json` | `PreToolUse`, matcher `^Bash$`. Codex requires a one-time trust via `/hooks`; the installer says so. |

Installed by the existing agent installer (`ax init` → installer, `ax install`) for the targets above. Removed by uninstall. Zed, Continue, Kiro, OpenCode, Antigravity, Hermes, and Takumi have no blocking tool hook: bootstrap instructions only.

## Failure model (Tier 3) → check

- **Blocks a legitimate edit** → behaviors 5–7; tests for retry, partial read, non-source.
- **Hook crash or timeout blocks the agent** → behavior 12; tests with garbage stdin, missing DB, unwritable state.
- **Latency on every tool call** → read-only SQLite, no full `Ax::open`; benchmark: p95 < 150 ms over 50 runs on this repo.
- **False positive on docs/config** → behavior 7; tests on `README.md`, `Cargo.toml`.
- **Clobbering the user's own hooks** → merge tests with pre-existing entries; uninstall leaves them intact.
- **Deny loop without a conversation id** → `anon` key + retry allow; test.
- **VS Code runs it on every tool** → behavior 1; test that unknown tools never touch the DB.

## Must not

- Block any call on an error path.
- Remove or rewrite hook entries it did not create.
- Deny a second identical attempt in the same conversation.
- Add network access.

## Setup plan

- **New files:**
  - `crates/ax-cli/src/commands/read_guard.rs` (command + tests)
  - `crates/ax-installer/src/hooks.rs` (per-IDE merge/remove + tests), if `targets.rs` gets too large
  - this spec, plus its EVIDENCE file
- **Edited:**
  - `crates/ax-cli/src/main.rs` (hidden subcommand)
  - `crates/ax-installer/src/targets.rs` (install/uninstall wiring)
  - `site/src/content/docs/reference/cli.md` and `guides/policy-engine.md` (docs-with-features)
- **Git:** no commits unless you ask (standing instruction).

## Revision 3 (post-approval, found in real execution)

1. **Cursor shows the agent `user_message`, not `agent_message`.** In a live Cursor session the denied Read surfaced only the short `user_message`, followed by Cursor's own note "Do not suggest workarounds to the blocked tool". The symbol list and the "repeat this same read" escape hatch never reached the agent. Behavior 13 for **cursor** becomes: deny = `{"permission":"deny","user_message":<full>,"agent_message":<full>}`. The short text is no longer used by any dialect.
2. **Tool names.** The implementation missed `readFile` and `view` from behavior 14. Both are added as read tools. `copilot_readFile` (VS Code Copilot's internal name) is also recognised.

## Revision 4 (found during the v5.0.0 release, approved: "Fix it in 5.0.0")

1. **Cursor does not send the read range.** A captured Cursor 3.21.16 `preToolUse` payload for a Read with offset and limit was `"tool_input":{"file_path":"…/savings.rs"}`, without `offset` or `limit`. So behavior 6 cannot hold in Cursor: a partial Read of an indexed file is denied once, like a whole read, and the identical retry passes (behavior 5). No field name fixes this, because the range is not in the payload.
2. **The deny message no longer promises it.** It said "A partial Read (offset/limit) is not guarded." It now says partial reads pass where the IDE sends the range, that Cursor does not, and to repeat the read. Test: `first_whole_read_of_indexed_source_is_denied_then_allowed` (RED observed on the old text, GREEN after). `reference/cli.md` says the same.
