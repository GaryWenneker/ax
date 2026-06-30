---
title: Introduction
description: What ax is, and why it makes AI coding agents faster and more precise.
---

ax is a **local-first code-intelligence tool** written in Rust. It parses your codebase with native [tree-sitter](https://tree-sitter.github.io/) grammars, stores symbols and relationships in a local SQLite database, and exposes a queryable **knowledge graph** through the [CLI](/reference/cli/), the [MCP server](/reference/mcp-server/), and the [`ax-core`](/reference/api/) Rust crate.

It helps AI coding agents — Claude Code, Cursor, Codex CLI, opencode, Hermes Agent, Gemini CLI, Antigravity IDE, and Kiro — **answer structural questions without scanning files**. Instead of fanning out across `grep`, `glob`, and `Read`, an agent queries a pre-built index and gets call paths, source, and impact in a handful of calls.

## Why it matters

When an agent explores a codebase, most of its budget goes to *discovery* — finding the right files before it can read them. ax removes that step: one `ax_explore` call returns numbered source, caller/callee spines, and blast-radius summaries.

The win is **surgical context and speed** — fewer tool calls, faster answers, on every codebase.

## What's in the graph

- **Symbols** — functions, classes, methods, types, routes, components, and more.
- **Edges** — calls, imports, inheritance, references, and framework-specific relationships.
- **Files** — structure plus full-text search (FTS5).

Extraction is **deterministic** — derived from the AST, never LLM-summarized.

## 100% local

No data leaves your machine. No API keys, no cloud index — just SQLite in `.ax/`.

Ready to try it? Head to the [Quickstart](/getting-started/quickstart/).
