---
title: MCP Server
description: The tools ax exposes to AI agents over MCP.
---

ax runs as a [Model Context Protocol](https://modelcontextprotocol.io/) server. Agents configured by the installer launch it automatically — you don't start it by hand:

```bash
ax serve --mcp
```

When a `.ax/` index exists, the agent gets the tools below. In a workspace with **no** index, the server announces itself inactive and lists **no** graph tools — the agent works normally with its built-in tools, and indexing stays your decision.

When `.ax/policy/` is indexed (**ax v2.0.0+**), policy tools are listed automatically. See [Policy Engine](/guides/policy-engine/).

## One tool by default: `ax_explore`

By default the server exposes a **single tool**, `ax_explore`. It's Read-equivalent: give it a natural-language question or a bag of symbol and file names, and it returns the **verbatim, line-numbered source** of the relevant symbols grouped by file — the same shape the `Read` tool gives you — plus the call paths between them (including dynamic-dispatch hops like callbacks, React re-render, and JSX children that grep can't follow) and a blast-radius summary of what depends on them. One call usually answers the whole question.

Exposing a single strong tool is deliberate. Measured agent behavior showed that one well-aimed tool steers agents to a direct answer better than a menu of narrower ones — fewer mis-picks — and agents reach for it both when answering questions and while editing code.

## The other tools

Seven more tools exist and stay fully functional, but are **unlisted by default** — everything they return already arrives inline on a `ax_explore` response (its blast-radius section, the relationship map, a symbol's body and its callee list):

| Tool | Purpose |
|---|---|
| `ax_node` | One symbol's source + caller/callee trail, or a whole file read with line numbers (Read-parity). Returns every overload's body for an ambiguous name. |
| `ax_search` | Find symbols by name across the codebase (locations only) |
| `ax_callers` | Find what calls a function |
| `ax_callees` | Find what a function calls |
| `ax_impact` | Analyze what code is affected by changing a symbol |
| `ax_files` | Get the indexed file structure (faster than filesystem scanning) |
| `ax_status` | Check index health and statistics (includes doc counts by extension) |

Re-enable any of them with the `ax_MCP_TOOLS` environment variable — a comma-separated allowlist of short names that replaces the default:

```bash
ax_MCP_TOOLS=explore,node,search,callers
```

Each also has a CLI equivalent (`ax node` / `query` / `callers` / `callees` / `impact` / `files` / `status`) for scripts and non-MCP harnesses.

## Operational tools (v4+)

Always advertised (alongside graph extras). Mirror high-value CLI session ops — agents never need to shell out for these:

| Tool | Purpose |
|---|---|
| `ax_sync` | Incremental index sync (changed files only) — same as `ax sync` |
| `ax_index` | Re-index; optional `force: true` clears and full-indexes (default behaves like `ax_sync`) |
| `ax_lsp` | `action: "status"` lists language servers; `action: "enrich"` resolves Exact edges (`limit` optional) — same as `ax lsp status\|enrich` |
| `ax_ship` | Quality-gate pipeline: `mode: "evaluate"` (default) or `"ci"`. Returns the ship report JSON. For `ci`, MCP `isError` is set when the gate fails — **never** exits the MCP process |
| `ax_policy_index` | Re-index / import rules and skills from `.ax/policy/` (`force` optional). Always listed so empty DB projects can bootstrap after files exist |

```json
ax_sync({})
ax_index({ "force": true })
ax_lsp({ "action": "enrich", "limit": 200 })
ax_ship({ "mode": "ci" })
ax_policy_index({ "force": true })
```

## Policy tools (v2.0.0+)

When `.ax/policy/` contains indexed rules or skills, the server also exposes:

| Tool | Purpose |
|---|---|
| `ax_preflight` | Turn-start: matched rules + skills + `inject` (full markdown bodies from SQLite) + auto-injected `<ax_index>` snapshot (doc counts by type) |
| `ax_rules` | List all rules or match against a prompt |
| `ax_skill` | Load the full markdown body of a skill by name |
| `ax_guard` | Block or warn before writes that violate CRITICAL rules. Built-in checks: UTF-8 BOM/encoding, secrets paths. **Generic gate (v3.1+):** any CRITICAL rule can opt in without code changes by adding a `guard: forbid-path: "<glob>"`, `guard: forbid-content: "<substring or /regex/>"`, or `guard: require-content: "<substring or /regex/>"` line to its body (the last is scoped to files matching that rule's `globs`). |

Agents should **not** read `.ax/policy/` files when these tools are available — policy is indexed locally and returned in MCP responses.

**Cursor:** call `ax_preflight` at turn start (MCP pull only — no prompt-hook).

**Claude Code:** prompt-hook may auto-inject `<ax_policy>…</ax_policy>` before the model sees the prompt, in addition to MCP tools. A `Stop`/`SubagentStop` hook (`ax stop-hook`) also runs a lightweight post-flight check on turn end — see [Turn-end post-flight (Claude Code)](#turn-end-post-flight-claude-code) below.

Call `ax_guard` with the target file path before editing project files. See [Policy Engine](/guides/policy-engine/) for flow diagrams and delivery channels.

**Not policy:** `ax_context` builds code-graph task context. Use `ax_preflight` for rules and skills.

### Turn-end post-flight (Claude Code)

`ax install` wires two more Claude Code hooks alongside the existing `UserPromptSubmit` prompt-hook: `Stop` and `SubagentStop`, both running `ax stop-hook`. On turn end it scans the working-tree files changed since the last commit (`git status --porcelain`) through the same `ax_guard` checks described above, and — only on a CRITICAL violation — returns `{"decision": "block", "reason": "…"}` so Claude fixes the issue before actually finishing. It always honors `stop_hook_active` to avoid looping, and no-ops entirely when there's no `.ax/policy/` or the project isn't indexed. Disable with `AX_NO_STOP_HOOK=1`.

### Diagnostics bridge: `ax_diagnostics`

ax has no way to read editor/LSP state itself. `ax_diagnostics` closes that gap from the agent side: pass in diagnostics you already gathered (Cursor's Problems panel / `ReadLints`, `tsc`, `ruff`, `eslint`, …) and get back graph-correlated context — which files intersect CRITICAL-guarded paths, and which tests `ax_affected` says are impacted by those same files. Always listed, regardless of whether policy is indexed (the guard/test correlation just degrades to empty).

```json
ax_diagnostics({ "diagnostics": [{ "path": "src/lib.rs", "line": 42, "severity": "error", "message": "…", "source": "rustc" }] })
```

## Memory tools

The [memory vault](/guides/memory/) adds two tools:

| Tool | Purpose |
|---|---|
| `ax_remember` | Store a durable project memory (decision, fix, convention). Returns similar existing memories so contradictions get updated instead of duplicated. |
| `ax_recall` | Hybrid search (full-text + vector similarity) over stored memories |

`ax_preflight` also recalls memories relevant to the prompt and injects the top matches automatically, so agents rarely need to call `ax_recall` by hand. It also injects an `<ax_index>` block with node/edge counts and **document inventory by extension** (markdown, office, PDF, other opaque types) on every turn — agents do not need a separate `ax_status` call to see what docs are indexed.

Git hooks run `ax capture-git --quiet` on every commit — commit messages with real context become `kind: git` memories without agent action. Agents should still call `ax_remember` for durable decisions that commit messages do not capture.

Responses larger than ~3k tokens carry a one-line `[ax] token budget` hint suggesting a narrower query or lower depth, nudging agents to keep context small.

## Lean responses (token savings)

Every `tools/call` reply is `{ content: [{ type: "text", text }], structuredContent?, isError }`. The `content.text` block is what strict clients and Cursor feed the model; `structuredContent` is machine-readable metadata for clients that consume it.

By default ax runs **lean**: it never ships the same data twice. The authoritative payload lives in `content.text`, and `structuredContent` is projected down to just the fields not already present in the text:

| Tool | `content.text` | Lean `structuredContent` |
|---|---|---|
| `ax_explore` | Numbered source + caller/callee spine | `query`, `summary`, `blastRadius`, compact `entries` (name/file/lines/score) — no source or neighbor duplication |
| `ax_preflight` | `inject` block (full rule/skill/memory/index bodies) | counts + `directiveDetected`, `captureProposal`, `guardRequired`, `mode`, `instruction`, `indexStats`, `pendingFiles` — no body duplication |
| `ax_status` | Markdown status summary + doc breakdown | `stats`, `lastIndexedAt`, `pendingFiles`, `policy` — no `text` duplication |
| `ax_context` | Markdown task context | `query`, `summary`, `stats`, `relatedFiles` — no `subgraph`/`codeBlocks` duplication |
| `ax_skill` | Skill body | metadata envelope (no `body`) |
| `ax_search` / `ax_node` / `ax_callers` / `ax_callees` / `ax_impact` / `ax_files` / `ax_affected` | Compact one-line-per-symbol list | omitted (text is authoritative) |

Savings measurement (`ax savings`) always runs against the full pre-projection payload, so the leaner wire format never distorts the numbers.

Set `AX_MCP_FULL=1` to restore the full `structuredContent` for every tool (for clients that read only structured data).

### Verbose MCP logging (Cursor Output)

To watch what each tool receives, how preflight enrichment builds the inject block, and what ax sends back on the wire — without changing agent-facing payloads — follow the full guide: **[MCP Logging & Quality](/guides/mcp-quality/)**. Short version:

1. Enable **Logging → Verbose MCP logging** in Command Center (writes `[ui] verbose_mcp = true` to `.ax/ship.toml` for the **active** project), **or** set `AX_MCP_VERBOSE=1` in the MCP server environment.
2. Reconnect / restart the ax MCP server in Cursor.
3. In Command Center (`ax web`), watch the status bar **Logging** chip (shows the latest tool / activity). Click it to open the **Logging** page — a table of today's `<project>/.ax/mcp-verbose-YYYY-MM-DD.log` for the active workspace (**newest at top**; scroll down for earlier days; **Scroll to new** returns to the live top; logs are not cleared from the UI).

![MCP Logging — live verbose stream with kind filters and Call Inspector](/screenshots/cc-logging.png)

4. Open the status-bar **Q** (Quality) chip for the metrics slide-out (correlation, enrichment, findings, **Copy fixpack**). CLI: `ax mcp audit`. Install `ax savings hook install` for `session=` tags.

![MCP Quality — correlation, enrichment, tool mix, findings, and Copy fixpack](/screenshots/cc-mcp-quality.png)

5. Optionally also use **View → Output → ax / MCP Logs**, or open the log file directly.

Lines are prefixed with `[ax-mcp]`. They are written to the log file (and optionally Cursor Output). They never append debug text to `content.text` or `structuredContent`. When a Cursor session id is known, lines also include `session=<uuid>` so `ax mcp audit --session <uuid>` can filter the verbose log.

`ax_guard` accepts the documented `path` + `operation` shape and common aliases (`paths[]`, `file`, `action=edit|write|delete`) so agents do not burn retries on `path required` validation errors.

### Response budget environment variables

| Variable | Default | Purpose |
|---|---|---|
| `AX_MCP_FULL` | unset | `1`/`true`/`yes` restores full `structuredContent` on every tool |
| `AX_MCP_VERBOSE` | unset | `1`/`true`/`yes` emits inbound/enrichment/outbound traces to stderr (Cursor Output); same as Logging → Verbose MCP logging |
| `AX_EXPLORE_MAX_LINES` | 40 | Max source lines per `ax_explore` snippet |
| `AX_EXPLORE_MAX_SOURCE_CHARS` | 2000 | Max source characters per `ax_explore` snippet |
| `AX_CONTEXT_MAX_BLOCKS` | 6 | Max code blocks in an `ax_context` response |
| `AX_CONTEXT_MAX_BLOCK_CHARS` | 1200 | Max characters per `ax_context` code block |

Explicit tool params (`maxLinesPerSnippet`, `maxSourceChars`) still override the env defaults per call.

## Architecture tools

For whole-graph understanding — subsystems, hot spots, and unexpected coupling — two tools sit on top of the graph analysis engine. See [Architecture Insights](/guides/architecture-insights/).

| Tool | Purpose |
|---|---|
| `ax_insights` | Leiden **communities** (subsystems), **god nodes** (most-connected concepts), and **surprising connections** (edges crossing both a community and a module boundary). Params: `resolution`, `godLimit`, `surprisingLimit`. |
| `ax_report` | Full Markdown architecture report (god nodes, communities, surprising links, dead code, unresolved refs, suggested questions). Returns `{ markdown }`. Param: `resolution`. |

Every edge also carries a **confidence** tag — `extracted` (read straight from the AST), `inferred` (resolved by a heuristic/name-matching pass), or `ambiguous` (one of several candidate targets was picked) — surfaced in `ax_node` / `ax_callers` / `ax_callees` output and the Command Center edge badges. Markdown docs (`.md`/`.mdx`) are indexed as `Doc` nodes with full parsing; PDF, Office, and other opaque formats appear as `Doc` nodes (presence only, no content extraction). Counts by extension are auto-injected via `ax_preflight` and available in `ax status --json` as `stats.docsByExtension`.

## How agents should use it

ax *is* the pre-built search index. For "how does X work?", architecture, a flow ("how does X reach Y"), or where-is-X questions — and while editing code — an agent should answer with `ax_explore` and stop, typically with **zero file reads**, rather than re-deriving the answer with `grep` + `Read`. A direct ax answer is one to a few calls; a grep/read exploration is dozens.

**Prefer MCP ops over shell CLI.** When the ax MCP server is connected, agents must call `ax_sync`, `ax_index`, `ax_lsp`, `ax_ship`, `ax_policy_index`, `ax_remember` / `ax_recall` instead of running the matching `ax …` commands in a terminal. Shell CLI is reserved for DEGRADED mode (MCP unreachable) or ops with no MCP tool (`install`, `upgrade`, `web`, `share`, `ship --watch`). IDE bootstrap files (`AGENTS.md`, `.cursor/rules/ax.mdc`, …) and the CRITICAL `prefer-mcp-ops` policy rule encode the same mapping.

The MCP server delivers this guidance to the main agent automatically, in the MCP `initialize` response (`server_instructions`). Because subagents and non-MCP harnesses never see that response, the installer also writes a short marker-fenced section into each agent's instructions file. Re-sync with `ax policy sync --fix` after upgrading ax.
