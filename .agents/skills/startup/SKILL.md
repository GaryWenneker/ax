---
name: startup
description: Runs the mandatory ax session-start sequence (ax_preflight with matched rules and skills). Use at the start of every new chat or user message when .ax/policy/ is indexed.
triggers: ["session start", "new message", "preflight", "startup", "turn start"]
tags: ["workflow", "preflight"]
priority: 100
seedVersion: 6
---
# ax Startup Protocol

> **ABSOLUTE — NO EXCEPTIONS.** Every user message starts with preflight when policy is enabled. Skipping preflight is a protocol violation.

## SS-00 — Preflight (unconditional, always first)

> **Run preflight exactly once per turn.** If you already called `ax_preflight` / `ax preflight` this turn, skip this step. Do not call preflight again after reading this skill.

**MCP connected:** run as the **very first MCP tool call** on every new user message, before reading files, searching code, or other work:

```json
ax_preflight({ "prompt": "<full user intent in English>", "files": ["<open or changed paths relative to project root>"] })
```

**CLI-only / MCP unavailable:** use the same engine via CLI (or the turn loop):

```bash
ax session start --json   # or resume --session <id>
printf '%s' "$PROMPT" | ax preflight --prompt-stdin --session <id> --json
# preferred loop: ax turn begin … → work → ax turn finish
```

- Apply all **CRITICAL** rules from `inject` before editing files.
- If a skill matched (including this one), follow its workflow from `inject` — do not re-read `.ax/policy/` on disk.
- **Fallback:** If `inject` lacks `<ax_policy>` (some MCP clients filter it), this skill delivers the startup workflow — follow it.
- `ax_rules` / `ax_skill` are for on-demand loads — preflight already returns full bodies in `inject` on turn start.

## Memory

Working notes for this chat stay in `ax_session` / `ax session`. They do not go into the vault. A durable decision, fix, or convention is `ax_remember` / `ax remember` with a trigger name (`triggers` / `--trigger`). Before re-deriving, fetch that name with `ax_recall` `cue` / `ax recall --cue`. If the user names a trigger, that fetch comes before explore. Free-text recall is the fallback. Questions about old graph changes, logs, or past turns use `depth=archive` on `ax_recall` / `ax recall` and on `ax_history` / `ax history`. Preflight will not contain those hits. A candidate long-term memory is confirmed with `ax_remember` `action=confirm` / `ax remember --confirm`. The old engram stays.

## SS-01b — Capture user directives (interview → confirm → save)

When the user gives a **durable** instruction (`je moet`, `always`, `never`, `@rule`, `#rule`):

```json
ax_policy_capture({ "action": "propose", "prompt": "<verbatim user message>", "files": ["<open paths>"] })
```

Show the preview. Ask the user **each question** from `questions[]` (level, alwaysApply, triggers, globs, priority, tags, body). Apply their answers to the rule.

Only after explicit user confirmation (`ja`, `yes`, `save`):

```json
ax_policy_capture({ "action": "save", "rule": { "frontmatter": { ... }, "body": "..." } })
```

Save writes to **ax.db** in database mode — not a disk-only file. Never auto-save without confirmation.

## SS-01a — Reuse what this conversation already knows

From the second turn on, read `<ax_session_context>` in the preflight inject before exploring: it lists graph calls already answered in this conversation whose cited files are unchanged. A repeated graph call in this conversation returns a short `[ax cache hit]` reference; the answer is already in your context, or `ax_expand` with its id returns it. Pass `fresh: true` only when you need a new query.

Record a durable fact, file, symbol, decision, or open question with `ax_session` (actions `add`, `update`, `compact`, `clear`, `fork`, `handoff`). Preflight shows `<ax_working_context>`; pass its hash as `known_context` and an unchanged snapshot comes back as one line. Pass the `session` id from `<ax_chat>` to every ax call in this chat; a preflight without it starts a new chat. A changed index marks it stale; `compact` confirms the notes against the current index. fork copies these notes to a new session. handoff starts a new session from the note you send. The old session stays readable, and the graph cache is not copied. `ax_durable` stores this chat's transcript, documents, and checkpointed tasks.

## SS-01 — Code context (after preflight)

> **ABSOLUTE**: For structural questions — how code works, call paths, blast radius, architecture — call **`ax_explore`** (or `ax_search` / `ax_node` / graph tools) **before** broad `Grep` / `Read`. Do not open with repo-wide Grep/Read while ax MCP is available. `GetMcpTools` / schema lookup is **not** explore. Skipping explore burns tokens and fails MCP quality (`ExploreBeforeGrep`).

```json
ax_explore({ "query": "<question or symbol names>" })
```

Use `ax_search`, `ax_node`, `ax_callers`, `ax_callees`, `ax_impact` for focused graph queries. Treat numbered explore source as already read — then `Read` / `Grep` only the files the graph already pointed to.

For a whole-graph overview — subsystems (Leiden communities), god nodes, and surprising cross-community links — use `ax_insights`, or `ax_report` for a full Markdown architecture report. Edges carry a confidence tag (extracted / inferred / ambiguous), and Markdown docs are indexed as `Doc` nodes linked to the code they reference.

**Policy vs code:** `ax_preflight` = rules/skills. `ax_explore` / `ax_context` = code graph — different tools.

## SS-02 — Pre-write guard

Before Write/StrReplace/Delete on project files when CRITICAL policy rules exist:

```json
ax_guard({ "path": "<relative file path>", "operation": "write" })
```

Preferred shape: **`path`** (string) + **`operation`** (`write` | `delete`).

Also accepted (avoids `path required` retries):

- `paths`: string array — guards each path
- `file` / `filepath`: aliases for `path`
- `action`: alias for `operation` (`edit` / `write` / `create` -> write; `delete` -> delete)

Do **not** call `ax_guard({ "action": "edit", "paths": [...] })` without also satisfying `path`/`paths` — prefer one call per file with `path` + `operation`.

Any CRITICAL rule can opt into a static check without code changes by adding a directive line to its body: `guard: forbid-path: "<glob>"`, `guard: forbid-content: "<substring or /regex/>"`, or `guard: require-content: "<substring or /regex/>"`. Both content directives honour the rule's `globs`; `forbid-content` falls back to project-wide when the rule declares none.

## SS-02b — Diagnostics correlation

ax cannot read editor/LSP state itself. If the IDE surfaces linter/compiler diagnostics (e.g. a Problems panel or lint tool output) for files you touched, feed them in to get graph-correlated context — which files intersect CRITICAL-guarded paths, and which tests the graph says are impacted:

```json
ax_diagnostics({ "diagnostics": [{ "path": "<relative path>", "line": 42, "severity": "error", "message": "<text>", "source": "<tool name>" }] })
```

## SS-02c — Prefer MCP ops (not shell CLI)

When ax MCP is connected, call MCP tools for session ops — do **not** shell out to the CLI:

| Need | MCP tool |
|------|----------|
| Incremental re-index | `ax_sync` |
| Full rebuild | `ax_index({ "force": true })` |
| LSP status / enrich | `ax_lsp({ "action": "status"\|"enrich" })` |
| Quality gate | `ax_ship({ "mode": "evaluate"\|"ci" })` |
| Refresh policy from disk | `ax_policy_index` |
| Memories | `ax_remember` / `ax_recall` |

When MCP is **unavailable** or the project is **CLI-only**, use the matching CLI — do not skip the workflow:

| Need | CLI |
|------|-----|
| Preflight / turn | `ax preflight` / `ax turn begin` |
| Code graph | `ax explore` / `ax query` / `ax node` |
| Guard | `ax policy guard <path>` |
| Capture directive | `ax policy capture` |
| Load skill body | `ax policy skill <name>` |
| Re-index / memory | `ax sync`, `ax remember` / `ax recall` |
| Cache expand | `ax cache expand` |

Shell `ax …` is also allowed for ops with no MCP tool (`install`, `upgrade`, `web`, `share`, `ship --watch`).

## SS-03 — Language rule (CRITICAL)

- **All agent responses to the user MUST be in English.**
- Translate non-English user input for your own reasoning; respond in English.

## SS-04 — MCP failure / CLI-only

If the MCP **server is unreachable** (connection error, no tools listed) **or** the project is CLI-only:

1. Report once: `ax MCP unreachable: [error]` and `Mode: DEGRADED` (skip the unreachable line for intentional CLI-only installs).
2. Continue with the CLI table in SS-02c — **not** editor Grep/Read as the first choice for structural work.
3. Prefer `ax explore` / `ax turn begin` over skipping graph/policy.

If `ax_preflight` **returns a payload** (including empty inject or `policyError`): continue the turn. Graph and memory tools remain available. Do **not** invent `Mode: PARTIAL` or skip `ax_explore` / `ax explore` because policy files failed to parse.

Session-start clients may call `ax_preflight` with no `prompt`; that is valid (`prompt` defaults to empty).

## SS-05 — Status line

After successful preflight (one line):

```
ax policy active — N rules, N skills matched
```

Summarise matched counts from the preflight response.
