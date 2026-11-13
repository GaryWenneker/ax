---
title: How It Works
description: The extraction, storage, resolution, and auto-sync pipeline.
---

ax turns source code into a queryable graph in four stages, and indexes policy (rules and skills) and the memory vault alongside it. See [MCP Logging & Quality](/guides/mcp-quality/) for how agents and the CLI record inbound/outbound/internal traces.

```
files → Extraction (tree-sitter) → DB (nodes/edges/files, schema v7)
            ↓
      Documentation pass (Markdown + opaque docs → Doc nodes)
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

## 1b. Documentation pass

After source extraction, a dedicated pass scans for documentation files (`.md`/`.mdx`, PDF, Office, and other opaque types). Markdown is parsed with pulldown-cmark; opaque files become `Doc` nodes without content extraction. See [Languages — Documentation files](/reference/languages/#documentation-files).

## 2. Storage

Everything goes into a local SQLite database (`.ax/ax.db`) with FTS5 full-text search. ax uses **sqlx** with WAL mode for concurrent reads during MCP queries.

## 3. Resolution

After extraction, references are resolved: function calls → definitions, imports → source files, class inheritance, and framework-specific patterns. Some dynamic-dispatch boundaries (callbacks, observers, React re-render, JSX children) are bridged by synthesizers. See [Resolution & Frameworks](/core-concepts/resolution/).

## 4. Auto-sync

The MCP server watches your project using native OS file events (FSEvents / inotify / ReadDirectoryChangesW). Changes are debounced, filtered to source files, and incrementally synced — the graph stays fresh as you code.

Git hooks installed by `ax init` also run `ax sync --quiet`, `ax ship --evaluate`, and `ax capture-git --quiet` on commit/merge so the graph, ship state, and memory vault stay current.

## 5. Memory

Memory is one of the reasons to use ax, next to the code graph and the policy engine. It lives in the same local `.ax/ax.db` as the graph. There are five tiers.

Working notes are this chat only (`ax session` / `ax_session`). They do not enter the vault. Recent turns stay hot for 90 days, then move to the archive with the same id. Long-term memory is a decision, a fix, an architecture note, a convention, or a note, stored under a trigger name. Preflight injects at most three of those titles, and the block stops at 6000 characters. A registered name outranks a fuzzy match. An unknown name returns nothing.

The archive is permanent and is never injected. It holds demoted turns, git commits, folder documents, graph diffs (symbol added, removed, or signature-changed), and selected activity. Token traces are skipped. Fetch them by name, by id, or with `depth=archive` on both recall and history. Archive search uses full text plus a local vector index, then one hop over a memory link, a shared file, or a shared symbol. The code graph stays the map. A memory stores file and symbol names. It does not copy the graph. An open file raises a long-term memory that names that file or a symbol in it.

A turn or a commit that states a decision becomes a candidate. Preflight skips it until a second source agrees or you confirm it. The original episode stays. A near-duplicate is linked, not deleted. Embeddings stay on this machine. Two embedding models are never compared, and sync rewrites rows after a model change.

MCP and the CLI call one function. `ax remember --trigger` is `triggers`. `ax recall --cue` is `cue`. `ax recall --depth archive` is `depth=archive`. `ax remember --confirm` is `action=confirm`.

See [Memory](/guides/memory/).
