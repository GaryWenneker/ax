---
title: Introduction
description: What ax is — a headless knowledge graph, memory vault and policy engine for AI coding agents, with ax-admin as the visual interface.
---

**ax** is **local-first intelligence for AI coding agents** — written in Rust, installed as a single binary, with no cloud index and no API keys.

Ax is two programs:

- **`ax`** is headless. It holds the knowledge graph, the memory and the policy engine, and serves them through the CLI and the MCP server. It has no built-in interface.
- **`ax-admin`** is the visual interface. It is a separate application that connects to an Ax through its [remote MCP](/guides/remote-mcp/) address.

![ax-admin — sync and rebuild index on a connected environment](/screenshots/ax-admin-operations.png)

The full ax, with its built-in Command Center, is being phased out; see [Headless ax and ax-admin](/guides/phase-out/). The headless build is released for Linux today. On macOS and Windows you install the full ax, whose CLI and MCP server are the same commands. See [Installation](/getting-started/installation/).

The first three layers are in `ax` and work together in every project. The fourth is `ax-admin`:

| Layer | What it does |
|---|---|
| **Knowledge graph** | Tree-sitter parsing → SQLite index of symbols, calls, imports, and routes |
| **Memory** | Five tiers in one local database: this chat's notes, recent turns, named long-term decisions, a permanent archive, and the live code graph. A name fetches the exact memory. Preflight injects at most three current titles and never the archive. MCP and the CLI do the same thing. |
| **Policy engine** | IDE-agnostic rules and skills in `.agents/`, matched and injected per turn |
| **ax-admin** | A separate application and the visual interface: status, search, symbols, insights, rules, skills, memory, sessions, operations, files, usage and logging of any Ax, through its remote MCP address. See [ax-admin](/guides/remote-admin/) |

Agents query structure through MCP (`ax_explore`, `ax_preflight`, …) instead of fanning out across `grep`, `glob`, and `Read`. The win is **surgical context** — fewer tool calls, faster answers, on every codebase.

Use [Smart Output](/guides/smart-output/) for compact agent context, [Remote MCP](/guides/remote-mcp/) for authenticated HTTPS access, and [Primary / replica synchronization](/guides/standalone-ax/) to share authorized project state across machines.

The documentation describes the current source. Binary downloads use the published release version; [build from main](/getting-started/installation/#build-from-main) when you need the latest source changes.

## Why it matters

When an agent explores a codebase, most of its budget goes to *discovery* — finding the right files before it can read them. ax removes that step for structure: one `ax_explore` call returns numbered source, caller/callee spines, and blast-radius summaries.

Policy removes another class of waste: re-explaining team conventions every session. Rules load once per turn via `ax_preflight`.

Memory removes a third: re-deriving past decisions. Store a durable decision with a trigger name (`ax remember --trigger`, or MCP `triggers`). Fetch that name before exploring (`ax recall --cue`, or MCP `cue`). An unknown name returns nothing. Old turns, commits, graph diffs, and logs stay in the archive and are fetched with `depth=archive`. A turn older than 90 days is demoted, not deleted. A decision spotted in a turn or a commit stays a candidate until you confirm it. The raw episode stays, and an open file raises a long-term memory that names that file or a symbol in it. The long guide is [Memory](/guides/memory/).

## What's in the graph

- **Symbols** — functions, classes, methods, types, routes, components, and more.
- **Edges** — calls, imports, inheritance, references, and framework-specific relationships.
- **Files** — structure plus full-text search (FTS5).

Extraction is **deterministic** — derived from the AST, never LLM-summarized.

## 100% local

No data leaves your machine. No API keys, no cloud index — just SQLite in `.ax/`.

## Link previews

Sharing a page URL uses a preview of that page. The site build screenshots the page, paints the CLI source version from `Cargo.toml` on the card, and points `og:image` at that file. The version on the card matches the documentation; binary downloads use the separate published-release pointer.

Ready to try it? Head to the [Quickstart](/getting-started/quickstart/).

<sub>AX · AgentXpansion</sub>
