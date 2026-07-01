---
title: How It Works
description: The extraction, storage, resolution, and auto-sync pipeline.
---

ax turns source code into a queryable graph in four stages. **ax v2.0.0** adds a fifth layer — **policy** — that indexes rules and skills alongside the code graph.

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
```

## 1. Extraction

Native [tree-sitter](https://tree-sitter.github.io/) parsers (Rust bindings) build ASTs. Language-specific queries extract **nodes** (functions, classes, methods, types…) and **edges** (calls, imports, extends, implements). Parsing runs in parallel via a Rayon worker pool (`AX_PARSE_WORKERS`).

## 2. Storage

Everything goes into a local SQLite database (`.ax/ax.db`) with FTS5 full-text search. ax uses **sqlx** with WAL mode for concurrent reads during MCP queries.

## 3. Resolution

After extraction, references are resolved: function calls → definitions, imports → source files, class inheritance, and framework-specific patterns. Some dynamic-dispatch boundaries (callbacks, observers, React re-render, JSX children) are bridged by synthesizers. See [Resolution & Frameworks](/core-concepts/resolution/).

## 4. Auto-sync

The MCP server watches your project using native OS file events (FSEvents / inotify / ReadDirectoryChangesW). Changes are debounced, filtered to source files, and incrementally synced — the graph stays fresh as you code.
