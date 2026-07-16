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

## Policy tools (v2.0.0+)

When `.ax/policy/` contains indexed rules or skills, the server also exposes:

| Tool | Purpose |
|---|---|
| `ax_preflight` | Turn-start: matched rules + skills + `inject` (full markdown bodies from SQLite) + auto-injected `<ax_index>` snapshot (doc counts by type) |
| `ax_rules` | List all rules or match against a prompt |
| `ax_skill` | Load the full markdown body of a skill by name |
| `ax_guard` | Block or warn before writes that violate CRITICAL rules (UTF-8 BOM, secrets paths) |

Agents should **not** read `.ax/policy/` files when these tools are available — policy is indexed locally and returned in MCP responses.

**Cursor:** call `ax_preflight` at turn start (MCP pull only — no prompt-hook).

**Claude Code:** prompt-hook may auto-inject `<ax_policy>…</ax_policy>` before the model sees the prompt, in addition to MCP tools.

Call `ax_guard` with the target file path before editing project files. See [Policy Engine](/guides/policy-engine/) for flow diagrams and delivery channels.

**Not policy:** `ax_context` builds code-graph task context. Use `ax_preflight` for rules and skills.

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

### Response budget environment variables

| Variable | Default | Purpose |
|---|---|---|
| `AX_MCP_FULL` | unset | `1`/`true`/`yes` restores full `structuredContent` on every tool |
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

The MCP server delivers this guidance to the main agent automatically, in the MCP `initialize` response. Because subagents and non-MCP harnesses never see that response, the installer also writes a short marker-fenced section into each agent's instructions file pointing at the `ax explore` CLI equivalent.
