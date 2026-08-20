---
name: startup
description: Runs the mandatory ax session-start sequence (ax_preflight with matched rules and skills). Use at the start of every new chat or user message when .ax/policy/ is indexed.
triggers: ["session start", "new message", "preflight", "startup", "turn start"]
tags: ["workflow", "preflight"]
priority: 100
---
# ax Startup Protocol

> **ABSOLUTE — NO EXCEPTIONS.** Every user message starts with preflight when policy is enabled. Skipping preflight is a protocol violation.

## SS-00 — Preflight (unconditional, always first)

> **Run preflight exactly once per turn.** If you already called `ax_preflight` this turn, skip this step. Do not call preflight again after reading this skill.

Run as the **very first MCP tool call** on every new user message, before reading files, searching code, or other work:

```json
ax_preflight({ "prompt": "<full user intent in English>", "files": ["<open or changed paths relative to project root>"] })
```

- Apply all **CRITICAL** rules from `inject` before editing files.
- If a skill matched (including this one), follow its workflow from `inject` — do not re-read `.ax/policy/` on disk.
- **Fallback:** If `inject` lacks `<ax_policy>` (some MCP clients filter it), this skill delivers the startup workflow — follow it.
- `ax_rules` / `ax_skill` are for on-demand loads — preflight already returns full bodies in `inject` on turn start.

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

Shell `ax …` is only for **Mode: DEGRADED** or ops with no MCP tool (`install`, `upgrade`, `web`, `share`, `ship --watch`).

## SS-03 — Language rule (CRITICAL)

- **All agent responses to the user MUST be in English.**
- Translate non-English user input for your own reasoning; respond in English.

## SS-04 — MCP failure

If preflight fails:

1. Report: `ax MCP unreachable: [error]`
2. State: `Mode: DEGRADED — no policy loaded.`
3. Do not proceed as if policy is active; best-effort only.

## SS-05 — Status line

After successful preflight (one line):

```
ax policy active — N rules, N skills matched
```

Summarise matched counts from the preflight response.
