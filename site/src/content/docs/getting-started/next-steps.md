---
title: Next Steps
description: Where to go once ax is installed and indexing.
---

You've got ax installed and a graph built. Here's where to go next.

## Understand the model

- [How It Works](/core-concepts/how-it-works/) — extraction, storage, resolution, sync, policy, and memory.
- [The Knowledge Graph](/core-concepts/knowledge-graph/) — node and edge kinds the graph is built from.
- [Resolution & Frameworks](/core-concepts/resolution/) — how references and framework routes get connected.

## Put it to work

- [Indexing a Project](/guides/indexing/) — full index, incremental sync, and the file watcher.
- [Workspaces (monorepo)](/guides/workspaces/) — `members`, `sync --all`, policy pull, shared memory.
- [Extractor plugins](/guides/plugins/) — out-of-tree process / WASM extractors under `.ax/plugins`.
- [LSP enrichment](/guides/lsp/) — Exact edges via rust-analyzer / tsserver / pyright / gopls.
- [Memory Vault](/guides/memory/) — store decisions, hybrid recall, git auto-capture, MCP tools.
- [Framework Routes](/guides/framework-routes/) — link URL patterns to their handlers.
- [Affected Tests in CI](/guides/affected-tests/) — run only the tests a change touches (`ax ship --ci`).
- [Command Center](/guides/command-center/) — quality gates, SonarQube, test-impact, draft PRs, and the ship dashboard.
- [Share Command Center](/guides/share/) — LAN token share, PWA, live action stream.
- [MCP Logging & Quality](/guides/mcp-quality/) — verbose traces, Logging page, Q slide-out, session hooks, and `ax mcp audit`.
- [Token Savings](/guides/token-savings/) — measure context-token savings from graph queries.
- [Agent Terminal](/guides/agent-terminal/) — run agents from Command Center with MCP wired in.
- [Architecture Insights](/guides/architecture-insights/) — communities, god nodes, interactive Graph, portable HTML export.

## Agent instructions

- [Policy Engine](/guides/policy-engine/) — IDE-agnostic rules and skills in `.ax/policy/`, MCP preflight, and ax web editor.
- [Remote Policy Share](/guides/policy-sharing/) — org-wide policy sync via GitHub or OneDrive Graph.

## Reference

- [MCP Server](/reference/mcp-server/) — the tools agents call.
- [CLI](/reference/cli/) — every command with examples.
- [Rust API](/reference/api/) — embed ax via the `ax-core` crate.
- [Integrations](/reference/integrations/) — supported agents and manual setup.
