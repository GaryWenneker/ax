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
| `ax_status` | Check index health and statistics |

Re-enable any of them with the `ax_MCP_TOOLS` environment variable — a comma-separated allowlist of short names that replaces the default:

```bash
ax_MCP_TOOLS=explore,node,search,callers
```

Each also has a CLI equivalent (`ax node` / `query` / `callers` / `callees` / `impact` / `files` / `status`) for scripts and non-MCP harnesses.

## Policy tools (v2.0.0+)

When `.ax/policy/` contains indexed rules or skills, the server also exposes:

| Tool | Purpose |
|---|---|
| `ax_preflight` | Turn-start: matched rules + skills + `inject` (full markdown bodies from SQLite) |
| `ax_rules` | List all rules or match against a prompt |
| `ax_skill` | Load the full markdown body of a skill by name |
| `ax_guard` | Block or warn before writes that violate CRITICAL rules (UTF-8 BOM, secrets paths) |

Agents should **not** read `.ax/policy/` files when these tools are available — policy is indexed locally and returned in MCP responses.

**Cursor:** call `ax_preflight` at turn start (MCP pull only — no prompt-hook).

**Claude Code:** prompt-hook may auto-inject `<ax_policy>…</ax_policy>` before the model sees the prompt, in addition to MCP tools.

Call `ax_guard` with the target file path before editing project files. See [Policy Engine](/guides/policy-engine/) for flow diagrams and delivery channels.

**Not policy:** `ax_context` builds code-graph task context. Use `ax_preflight` for rules and skills.

## How agents should use it

ax *is* the pre-built search index. For "how does X work?", architecture, a flow ("how does X reach Y"), or where-is-X questions — and while editing code — an agent should answer with `ax_explore` and stop, typically with **zero file reads**, rather than re-deriving the answer with `grep` + `Read`. A direct ax answer is one to a few calls; a grep/read exploration is dozens.

The MCP server delivers this guidance to the main agent automatically, in the MCP `initialize` response. Because subagents and non-MCP harnesses never see that response, the installer also writes a short marker-fenced section into each agent's instructions file pointing at the `ax explore` CLI equivalent.
