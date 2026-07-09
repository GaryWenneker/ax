---
title: Introduction
description: What ax is — knowledge graph, policy engine, and Command Center for AI coding agents.
---

**ax** is **local-first intelligence for AI coding agents** — written in Rust, installed as a single binary, with no cloud index and no API keys.

Three layers work together in every project:

| Layer | What it does |
|---|---|
| **Knowledge graph** | Tree-sitter parsing → SQLite index of symbols, calls, imports, and routes |
| **Policy engine** | IDE-agnostic rules and skills in `.ax/policy/`, matched and injected per turn |
| **Command Center** | Git watcher, quality gates, test-impact, SSE dashboard, draft PRs |

Agents query structure through MCP (`ax_explore`, `ax_preflight`, …) instead of fanning out across `grep`, `glob`, and `Read`. The win is **surgical context** — fewer tool calls, faster answers, on every codebase.

**v2.1.4** adds **per-model token usage tracking** — LLM offload calls (`ax explore` / `ax_explore`) record prompt, completion, and total tokens in `~/.ax/usage.db`. Filter via `ax tokens` or the **Tokens** tab in `ax web`.

**v2.1.3** adds **`ax policy storage status`** — show effective policy storage mode with project and global config paths.

**v2.1.2** improves **database migration** — `ax policy storage database --migrate` recursively scans the repo for rules and skills, with per-item interview questions. See [Policy Engine](/guides/policy-engine/).

**v2.1.1** adds **policy capture** — durable user directives (`always`, `you must`, `@rule`) can be proposed as team rules with an interview step before saving to `ax.db`. See [Policy Engine](/guides/policy-engine/).

**v2.1.0** introduced the **Command Center** — git-aware quality gates, test-impact analysis, an SSE dashboard, and draft PR integration (Azure DevOps or GitHub). See [Command Center](/guides/command-center/).

**v2.0.0** introduced the **policy engine** — project rules and skills delivered via MCP, CLI, prompt-hook, and ax web. See [Policy Engine](/guides/policy-engine/).

## Why it matters

When an agent explores a codebase, most of its budget goes to *discovery* — finding the right files before it can read them. ax removes that step for structure: one `ax_explore` call returns numbered source, caller/callee spines, and blast-radius summaries.

Policy removes another class of waste: re-explaining team conventions every session. Rules load once per turn via `ax_preflight`.

## What's in the graph

- **Symbols** — functions, classes, methods, types, routes, components, and more.
- **Edges** — calls, imports, inheritance, references, and framework-specific relationships.
- **Files** — structure plus full-text search (FTS5).

Extraction is **deterministic** — derived from the AST, never LLM-summarized.

## 100% local

No data leaves your machine. No API keys, no cloud index — just SQLite in `.ax/`.

Ready to try it? Head to the [Quickstart](/getting-started/quickstart/).
