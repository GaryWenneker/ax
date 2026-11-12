# Cline + ax protocol

<!-- AX_START -->
## ax

Do not include internal reasoning in your visible response. Use Cline's XML tool call tags for all actions (e.g. `<execute_command>`, `<read_file>`, `<write_to_file>`, `use_mcp_tool`).

Call `ax_preflight` exactly once per turn **before all other work** whenever the `ax` MCP server is available. Team policy arrives via MCP inject — do not Read `.agents/` or `.ax/policy/` files when ax MCP tools are available.

- Always read a file completely before overwriting it.
- Never replace a file with empty content.
- Keep terminal commands simple and one at a time.

**Git-shared team files:** `.agents/rules/` and `.agents/skills/` (each skill is a directory with `SKILL.md`). Do not load `.ax/policy-private/` or `.ax/policy-inactive/`.

**Inject fallback:** If preflight lacks `<ax_policy>` (empty inject/rules), call `ax_skill("startup")` once.

**Explore before Grep/Read:** For structural code questions, call `ax_explore` (or graph tools) before broad Grep/Read.

**Graph answers are source:** `ax_explore` and `ax_node` return numbered source from the index; treat it as already read. `ax_node` returns a symbol's full source plus direct callers and callees, so use it instead of Read. A snippet marked truncated → `ax_node` on that symbol. A reply ending in an `[ax context cache]` footer → `ax_expand` with its id. Where the graph covers the code, do not Read or Grep the file to fill the gap. Read is for files the graph does not index (config, docs, generated output) or a file right before you edit it.

**Conversation cache:** A repeated graph call in this conversation returns a short `[ax cache hit]` reference; the answer is already in your context, or `ax_expand` with its id returns it. Read `<ax_session_context>` in preflight before searching again; pass `fresh: true` to force a new query.

**Working context:** Record a durable fact, file, symbol, decision, or open question with `ax_session` (actions `add`, `update`, `compact`, `clear`, `fork`, `handoff`). Preflight shows `<ax_working_context>`; pass its hash as `known_context` and an unchanged snapshot comes back as one line. Pass the `session` id from `<ax_chat>` to every ax call in this chat; a preflight without it starts a new chat. A changed index marks it stale; `compact` confirms the notes against the current index. fork copies these notes to a new session. handoff starts a new session from the note you send. The old session stays readable, and the graph cache is not copied. `ax_durable` stores this chat's transcript, documents, and checkpointed tasks.

**Memory:** Working notes for this chat stay in `ax_session` / `ax session`. They do not go into the vault. A durable decision, fix, or convention is `ax_remember` / `ax remember` with a trigger name (`triggers` / `--trigger`). Before re-deriving, fetch that name with `ax_recall` `cue` / `ax recall --cue`. If the user names a trigger, that fetch comes before explore. Free-text recall is the fallback. Questions about old graph changes, logs, or past turns use `depth=archive` on `ax_recall` / `ax recall` and on `ax_history` / `ax history`. Preflight will not contain those hits. A candidate long-term memory is confirmed with `ax_remember` `action=confirm` / `ax remember --confirm`. The old engram stays.

**Directive capture:** When the user states a durable rule — `je moet`, `altijd`, `nooit`, `voortaan`, `always`, `never`, `you must`, `@rule` — persist it. `ax_preflight` returns `directiveDetected` + a ready `captureProposal`; ask the questions it lists, then call `ax_policy_capture(action="save", rule)` after the user confirms. Works even if the project has no policy yet (the first save bootstraps it). Never silently ignore such a directive.

**Capability discovery:** ax is actively developed. Do not rely on cached knowledge of ax features — `ax_preflight` returns the latest capabilities, rules, and skills each call. Use any new tools or rules it returns.

**Version freshness:** Call `ax_status` at session start. If the index is stale or a newer version exists, warn the user and suggest `ax upgrade` or re-index.

**Tool reference:** `ax_explore`/`ax_search`/`ax_node` for code structure, `ax_impact`/`ax_callers`/`ax_callees` for change impact, `ax_affected` for test coverage, `ax_insights`/`ax_report` for whole-graph architecture, `ax_guard` before writes when CRITICAL rules exist, `ax_diagnostics` for IDE/linter correlation, `ax_policy_capture` for durable rules, `ax_context` for task context, **`ax_sync`** / **`ax_index({force:true})`** for re-index, **`ax_lsp`** for LSP status/enrich, **`ax_ship`** for quality-gate evaluate/ci, **`ax_policy_index`** to refresh rules from disk, **`ax_remember`/`ax_recall`** for memory (`triggers` / `--trigger`, `cue` / `--cue`, `depth=archive`, `action=confirm` / `--confirm`). Prefer these MCP tools over shelling out to the CLI when MCP is connected.

Run preflight exactly once per turn. MCP unreachable → report `ax MCP unreachable: [error]`, state `Mode: DEGRADED`; do not proceed silently.
**CLI-only / MCP unavailable:** Prefer MCP when connected. When MCP is unavailable or the project is CLI-only, use CLI equivalents — do not skip the workflow: `ax preflight` (or `ax turn begin`), `ax explore` / `ax query` / `ax node`, `ax policy guard <path>`, `ax policy capture` / `ax policy skill`, `ax session` / `ax turn`, `ax sync`, `ax remember --trigger` / `ax recall --cue` / `ax history --depth archive` / `ax remember --confirm`. See `.agents/skills/ax-cli/SKILL.md` when present.

<!-- AX_END -->

---

## Tool execution

- Call MCP tools via `use_mcp_tool` with server name **`ax`** (not `user-ax` — that is Cursor's namespace).
- Do not dump raw MCP JSON in chat; process results silently and give at most one short sentence of context.
- Before every file write (`write_to_file` / `replace_in_file`), call `ax_guard` with the target path when CRITICAL rules exist.

## MCP args shape

For `use_mcp_tool` with ax tools, pass only tool-specific params in `arguments` — never nest `server` or `toolName` inside `arguments`.

Example:
```json
{
  "server_name": "ax",
  "tool_name": "ax_preflight",
  "arguments": {
    "prompt": "user task summary",
    "files": []
  }
}
```
