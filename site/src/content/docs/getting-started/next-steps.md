---
title: Next Steps
description: Where to go once ax is installed and indexing.
---

You've got ax installed and a graph built. Here's where to go next.

## Understand the model

- [How It Works](/core-concepts/how-it-works/) — extraction, storage, resolution, sync, policy, and memory.
- [The Knowledge Graph](/core-concepts/knowledge-graph/) — node and edge kinds the graph is built from.
- [Resolution & Frameworks](/core-concepts/resolution/) — how references and framework routes get connected.
- [Headless ax and ax-admin](/guides/phase-out/) — where Ax is going: one headless `ax` on every platform, `ax-admin` as the only visual interface.

## ax: CLI and MCP

- [Indexing a Project](/guides/indexing/) — full index, incremental sync, and the file watcher.
- [Workspaces (monorepo)](/guides/workspaces/) — `members`, `sync --all`, policy pull, shared memory.
- [Extractor plugins](/guides/plugins/) — out-of-tree process / WASM extractors under `.ax/plugins`.
- [LSP enrichment](/guides/lsp/) — Exact edges via rust-analyzer / tsserver / pyright / gopls.
- [Memory](/guides/memory/) — working notes, named long-term decisions, and a permanent archive. MCP and the CLI use the same commands.
- [Framework Routes](/guides/framework-routes/) — link URL patterns to their handlers.
- [Affected Tests in CI](/guides/affected-tests/) — run only the tests a change touches (`ax ship --ci`).
- [MCP Logging & Quality](/guides/mcp-quality/) — inbound/outbound/internal traces (MCP + CLI), the daily verbose log, session hooks, and `ax mcp audit`.
- [CLI-only mode](/guides/cli-only/) — graph and policy through the CLI without MCP, with the same Logging directions from ax 8.4.4.
- [Token Savings](/guides/token-savings/) — measure context-token savings from graph queries.
- [Agent budget](/guides/budget/) — quote imported agent turns and plan a local monthly budget.
- [Pi integration](/guides/pi/) — observe a Pi agent run without taking over the agent loop.
- [Architecture Insights](/guides/architecture-insights/) — communities, god nodes, `ax insights`, `ax report`, portable HTML export.

## Agent instructions

- [Policy Engine](/guides/policy-engine/) — IDE-agnostic rules and skills in `.ax/policy/`, delivered through MCP preflight and the `ax policy` commands.
- [Remote Policy Share](/guides/policy-sharing/) — org-wide policy sync via GitHub or OneDrive Graph.

## ax-admin

- [ax-admin](/guides/remote-admin/) — the separate admin application: status, search, memory, rules, operations, usage and logging of any Ax.

## Running and deploying

- [Headless Linux build](/guides/headless-linux/) — the build without a built-in interface, for servers, sandboxes, containers and CI.
- [Remote MCP](/guides/remote-mcp/) — `ax mcp remote serve`: authenticated HTTPS access for agents on other machines and for `ax-admin`.

## Reference

- [MCP Server](/reference/mcp-server/) — the tools agents call.
- [CLI](/reference/cli/) — every command with examples.
- [Rust API](/reference/api/) — embed ax via the `ax-core` crate.
- [Integrations](/reference/integrations/) — supported agents and manual setup.

## Full ax (being phased out)

These pages describe parts that only the full ax has. They stay available while releases ship it.

- [Command Center](/guides/command-center/) — policy, graph, memory, logging, and the /ship quality-gate dashboard of the full ax.
- [Share Command Center](/guides/share/) — LAN token share, PWA, live action stream.
- [Desktop Client](/guides/desktop-client/) — the native desktop window of the full ax.
- [Cloudflare Tunnel](/guides/cloudflare/) — expose the Command Center through a Cloudflare tunnel.
- [Obsidian Vault](/guides/obsidian-vault/) — the WebDAV vault served by the full ax.
- [Standalone server & replicas](/guides/standalone-ax/) — `ax server` and replication.
