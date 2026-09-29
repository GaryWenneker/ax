---
title: Introduction
description: What ax is — knowledge graph, memory vault, policy engine, and Command Center for AI coding agents.
---

![ax Command Center — Rules page with the Policy, Code, Activity, and System sidebar](/screenshots/cc-policy-rules.png)

**ax** is **local-first intelligence for AI coding agents** — written in Rust, installed as a single binary, with no cloud index and no API keys.

Four layers work together in every project:

| Layer | What it does |
|---|---|
| **Knowledge graph** | Tree-sitter parsing → SQLite index of symbols, calls, imports, and routes |
| **Memory vault** | Durable decisions, fixes, and conventions — hybrid recall, git auto-capture, preflight injection |
| **Policy engine** | IDE-agnostic rules and skills in `.agents/`, matched and injected per turn |
| **Command Center** | Policy editors, memory vault, graph, quality gates, token savings, MCP Logging / Quality, SSE dashboard, draft PRs |

Agents query structure through MCP (`ax_explore`, `ax_preflight`, …) instead of fanning out across `grep`, `glob`, and `Read`. The win is **surgical context** — fewer tool calls, faster answers, on every codebase.

## Why it matters

When an agent explores a codebase, most of its budget goes to *discovery* — finding the right files before it can read them. ax removes that step for structure: one `ax_explore` call returns numbered source, caller/callee spines, and blast-radius summaries.

Policy removes another class of waste: re-explaining team conventions every session. Rules load once per turn via `ax_preflight`.

The memory vault removes a third: re-deriving past decisions. Relevant memories arrive with preflight; git hooks capture the "why" from commit messages automatically.

## What's in the graph

- **Symbols** — functions, classes, methods, types, routes, components, and more.
- **Edges** — calls, imports, inheritance, references, and framework-specific relationships.
- **Files** — structure plus full-text search (FTS5).

Extraction is **deterministic** — derived from the AST, never LLM-summarized.

## 100% local

No data leaves your machine. No API keys, no cloud index — just SQLite in `.ax/`.

Ready to try it? Head to the [Quickstart](/getting-started/quickstart/).

<sub>ax · Aero Xecution</sub>
