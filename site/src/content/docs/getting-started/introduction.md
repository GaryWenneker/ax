---
title: Introduction
description: What ax is — knowledge graph, memory vault, policy engine, and Command Center for AI coding agents.
---

**ax** is **local-first intelligence for AI coding agents** — written in Rust, installed as a single binary, with no cloud index and no API keys.

Four layers work together in every project:

| Layer | What it does |
|---|---|
| **Knowledge graph** | Tree-sitter parsing → SQLite index of symbols, calls, imports, and routes |
| **Memory vault** | Durable decisions, fixes, and conventions — hybrid recall, git auto-capture, preflight injection |
| **Policy engine** | IDE-agnostic rules and skills in `.ax/policy/`, matched and injected per turn |
| **Command Center** | Git watcher, quality gates, SonarQube, token savings, SSE dashboard, draft PRs |

Agents query structure through MCP (`ax_explore`, `ax_preflight`, …) instead of fanning out across `grep`, `glob`, and `Read`. The win is **surgical context** — fewer tool calls, faster answers, on every codebase.

## What's new in v2.1.6

- **Memory vault** — `ax remember`, `ax recall`, `ax capture-git`, MCP `ax_remember` / `ax_recall`, Command Center **Memory** page. Hybrid search (FTS5 + local embeddings). Git hooks auto-capture non-trivial commits after every `git commit`.
- **Token savings** — real BPE token estimates, dollar pricing, **Savings** page in Command Center (no longer beta). Import Cursor / Claude Code session logs via `ax savings import`.
- **Command Center** — workspace switcher, Agent terminal, SonarQube reverse proxy with auto-login, policy view-first editors, Memory and Savings pages.
- **Performance** — batch graph inserts, policy cache, incremental index content hashing, CI workflow for `cargo test`.

See [Memory vault](/guides/memory/), [Token savings](/guides/token-savings/), and [Command Center](/guides/command-center/).

## Earlier releases

**v2.1.5** — context-token savings tracking in `~/.ax/usage.db`.

**v2.1.3** — `ax policy storage status` for effective policy storage mode.

**v2.1.2** — database migration with recursive policy scan and interview questions.

**v2.1.1** — policy capture from durable directives (`always`, `you must`, `@rule`).

**v2.1.0** — Command Center: git-aware quality gates, test-impact, SSE dashboard, draft PRs.

**v2.0.0** — policy engine: rules and skills via MCP, CLI, prompt-hook, and ax web.

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
