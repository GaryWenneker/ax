---
title: How It Works
description: The extraction, storage, resolution, and auto-sync pipeline.
---

ax turns source code into a queryable graph in four stages. **ax v2.0.0+** adds **policy** (rules and skills indexed alongside the graph). **ax v2.1.6** adds a **memory vault** — durable project knowledge with hybrid recall and git auto-capture.

```
files → Extraction (tree-sitter) → DB (nodes/edges/files, schema v7)
            ↓
      Resolution (imports, name-matching, framework patterns)
            ↓
      Graph queries (callers, callees, impact)
            ↓
      Context building (markdown / JSON for AI consumption)
            ↓
      Policy match (.ax/policy/ → MCP preflight, guard, prompt-hook)
            ↓
      Memory recall (decisions, git capture → preflight inject)
```

## 1. Extraction

Native [tree-sitter](https://tree-sitter.github.io/) parsers (Rust bindings) build ASTs. Language-specific queries extract **nodes** (functions, classes, methods, types…) and **edges** (calls, imports, extends, implements). Parsing runs in parallel via a Rayon worker pool (`AX_PARSE_WORKERS`).

## 2. Storage

Everything goes into a local SQLite database (`.ax/ax.db`) with FTS5 full-text search. ax uses **sqlx** with WAL mode for concurrent reads during MCP queries.

## 3. Resolution

After extraction, references are resolved: function calls → definitions, imports → source files, class inheritance, and framework-specific patterns. Some dynamic-dispatch boundaries (callbacks, observers, React re-render, JSX children) are bridged by synthesizers. See [Resolution & Frameworks](/core-concepts/resolution/).

## 4. Auto-sync

The MCP server watches your project using native OS file events (FSEvents / inotify / ReadDirectoryChangesW). Changes are debounced, filtered to source files, and incrementally synced — the graph stays fresh as you code.

Git hooks installed by `ax init` also run `ax sync --quiet`, `ax ship --evaluate`, and `ax capture-git --quiet` on commit/merge so the graph, ship state, and memory vault stay current.

## 5. Memory vault

Project memories live in the same `.ax/ax.db` SQLite database as the graph. Agents store durable knowledge with `ax_remember`; humans use `ax remember` or the Command Center **Memory** page.

Recall combines FTS5 keyword search with local vector embeddings (Reciprocal Rank Fusion). `ax_preflight` injects top matches every turn. Post-commit hooks capture non-trivial git commit messages as `kind: git` memories automatically.

See [Memory vault](/guides/memory/).
