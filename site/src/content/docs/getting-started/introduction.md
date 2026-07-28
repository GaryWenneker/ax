---
title: Introduction
description: What ax is — knowledge graph, memory vault, policy engine, and Command Center for AI coding agents.
---

![ax Command Center — completed quality gate with pipeline steps, branch overview, and SonarQube status](/screenshots/cc-ship-full.png)

**ax** is **local-first intelligence for AI coding agents** — written in Rust, installed as a single binary, with no cloud index and no API keys.

Four layers work together in every project:

| Layer | What it does |
|---|---|
| **Knowledge graph** | Tree-sitter parsing → SQLite index of symbols, calls, imports, and routes |
| **Memory vault** | Durable decisions, fixes, and conventions — hybrid recall, git auto-capture, preflight injection |
| **Policy engine** | IDE-agnostic rules and skills in `.ax/policy/`, matched and injected per turn |
| **Command Center** | Git watcher, quality gates, SonarQube, token savings, MCP Logging / Quality, SSE dashboard, draft PRs |

Agents query structure through MCP (`ax_explore`, `ax_preflight`, …) instead of fanning out across `grep`, `glob`, and `Read`. The win is **surgical context** — fewer tool calls, faster answers, on every codebase.

## What's new in v4.1.0

- **Policy layers** — every rule/skill has a `scope`: company (`~/.ax/global_policy/`), workspace, project, private user (`~/.ax/private_policy/`), or private project (`.ax/policy-private/`, gitignored). Merge order is company → workspace → project → private; later wins on the same id.
- **Pack export by default** — `ax policy pack export` includes all enabled project/workspace items (no `shared` tag required). Opt out with tags `local` / `noshare`. Company and private scopes never pack.
- **Built-in packs** — `ax policy pack install --list` / `ax policy pack install azdo-fullstack` for optional Azure DevOps ticket-to-release skills and rules.
- **Command Center Policy UX** — layer filter on Rules/Skills, Scope on editors, **Policy → Sync** and **Policy → Review**, full-height rule/skill editors, consistent card padding.
- **Prices + desktop client** — OpenRouter daily pricing catalog in Command Center; optional native `ax desktop` (wgpu/egui) embedding the same APIs.

See [Policy Engine](/guides/policy-engine/), [Workspaces](/guides/workspaces/), [Desktop Client](/guides/desktop-client/), and [Command Center](/guides/command-center/).

## What's new in v4.0.0

- **Monorepo workspaces** — `ax init --workspace`, `ax index --all` / `ax sync --all`, Command Center workspace switcher, and `ax policy pull` for federated policy.
- **Multi-format graph export** — CLI plus Graph → Export (Download + Copy) for JSON, GraphML, DOT, Mermaid, PlantUML, Cypher, and HTML, with node/edge summary.
- **Extractor plugins** — process plugin host (optional WASM via `plugins-wasm`); Settings → Plugins lists discovered extractors.
- **Optional ONNX embeddings** — dense memory vectors behind the `onnx` Cargo feature; Settings → Embeddings shows backend and model paths.
- **`ax ship --ci`** — headless quality gate (JSON + non-zero exit) and reusable [`.github/workflows/ax-ship.yml`](https://github.com/GaryWenneker/ax/blob/main/.github/workflows/ax-ship.yml).
- **LSP bridge** — Exact call edges via `ax-lsp`; Unresolved → enrich with limit, PATH server checklist, and post-run report.
- **Share + PWA** — `ax share` LAN token gate, Settings → Sharing status card, opt-in PWA (`?pwa=1` / Enable PWA), and StatusBar Activity chip over `/api/actions/events`.
- **ax Mint default theme** — `#3ee4b2` accent as the product default; project-browser tokens follow `--accent`.

See [Workspaces](/guides/workspaces/), [Share](/guides/share/), [Plugins](/guides/plugins/), [LSP](/guides/lsp/), and [Command Center](/guides/command-center/).

## What's new in v3.1.0

- **Diagnostics bridge** — `ax_diagnostics` correlates editor/LSP/compiler findings (Cursor Problems panel, `tsc`, `ruff`, `eslint`, …) with the graph: which files intersect CRITICAL-guarded paths, and which tests `ax_affected` says are impacted.
- **Generic guard directives** — any CRITICAL rule can opt into a static check without code changes via a `guard: forbid-path: "<glob>"`, `guard: forbid-content: "<substring or /regex/>"`, or `guard: require-content: "<substring or /regex/>"` line in its body.
- **Claude Code Stop hook** — `ax install` wires `Stop`/`SubagentStop` to `ax stop-hook`, re-running `ax_guard` on every uncommitted file at turn end and blocking only on a CRITICAL violation. Disable with `AX_NO_STOP_HOOK=1`.
- **`ax ship` auto-commit** — opt-in Aider-style checkpointing (`[auto_commit]` in `ship.toml`, or `--auto-commit`/`--revert-on-fail` for one run): commit the working tree before the quality gate runs, and safely `git reset --mixed` the checkpoint (never `--hard`) if it fails.
- **MCP Logging** — daily log rotation (`mcp-verbose-YYYY-MM-DD.log`) in the Settings timezone, a **Has query** filter, a date picker, and scroll-up history that seamlessly loads prior days.
- **SonarQube resilience** — the quality gate now retries `podman start` (up to 3x) on an existing-but-stopped container before failing.
- **New integrations** — VS Code (Copilot Chat), Windsurf (Cascade), and Zed join the interactive installer as MCP-only targets.
- **Command Center polish** — crisper title-bar wave chrome (tighter fade, no clipped Back/Follow/Full/Clear controls) and a cinematic bokeh/wave refresh on the marketing site.

See [MCP Logging & Quality](/guides/mcp-quality/), [Command Center](/guides/command-center/), and [MCP Server](/reference/mcp-server/).

## What's new in v3.0.0

- **MCP Logging** — live table of the active project's verbose MCP stream (`<project>/.ax/mcp-verbose-YYYY-MM-DD.log`, one file per calendar day in Settings timezone) with kind/tool filters, Call Inspector, scroll-up history, and project switcher.
- **MCP Quality loop** — status-bar **Q** chip + slide-out scores correlation, enrichment, Explore-before-Grep waste, and fixpacks. CLI: `ax mcp audit`.
- **Cursor sessionStart hook** — `ax savings hook install` tags Composer chats with the picker model and session id for accurate savings + audit correlation.
- **Savings dashboard** — activity heatmap, period filter, TokenViz path graph, by-model rollups, and import from Cursor / Claude Code transcripts.
- **Architecture Graph** — interactive Leiden communities, god nodes, confidence-tagged edges, and doc nodes in Command Center.
- **Document inventory** — PDF, Office, Markdown, and other docs as `Doc` nodes; `<ax_index>` auto-injected on every `ax_preflight`.

![MCP Logging — live verbose stream with kind filters and Call Inspector](/screenshots/cc-logging.png)

See [Command Center](/guides/command-center/), [Token savings](/guides/token-savings/), and [MCP Server](/reference/mcp-server/).

## What's new in v2.1.14

- **Document inventory** — PDF, Office, Markdown, and other doc types indexed as `Doc` nodes. Counts by extension (`stats.docsByExtension`) in `ax status` / `ax_status`.
- **Auto-injected index snapshot** — every `ax_preflight` response includes an `<ax_index>` block (doc totals, markdown/office/PDF breakdown, pending sync) so agents see what is indexed without a separate status call.

See [MCP Logging & Quality](/guides/mcp-quality/), [Indexing](/guides/indexing/), and [Languages](/reference/languages/).

## What's new in v2.1.7

- **SonarQube dashboard** — iframe loading overlay no longer blocks clicks; lighter dark-theme injection so the proxied UI stays responsive.
- **Token savings** — line-range counterfactuals, related-file array scanning, `codeBlocks` fallback, and `AX_SAVINGS_CF_MODE` (`full` / `range` / `max`).
- **Command Center UX** — running-state spinners on Ship, Savings, Sonar, Settings, and pipeline steps.

See [Token savings](/guides/token-savings/) and [Command Center](/guides/command-center/).

## What's new in v2.1.6

- **Memory vault** — `ax remember`, `ax recall`, `ax capture-git`, MCP `ax_remember` / `ax_recall`, Command Center **Memory** page with modal composer. Hybrid search (FTS5 + local embeddings). Git hooks auto-capture non-trivial commits after every `git commit`.
- **Token savings** — real BPE token estimates, dollar pricing, **Savings** page in Command Center (no longer beta). Import Cursor / Claude Code session logs via `ax savings import`.
- **Cursor auth switching** — `ax cursor auth save/use/list/status/show` for fast Cursor subscription switching. Snapshots `cursorAuth/*` keys from `state.vscdb` plus `auth.json` into `~/.ax/cursor-auth/`.
- **Command Center** — project browser with disk navigation, ax-project detection, and in-browser `ax init`; modal-based forms for Memory, Agent profiles, and settings; workspace switcher; Agent terminal; SonarQube reverse proxy with auto-login and comprehensive dark theme; policy view-first editors.
- **SonarQube proxy** — full dark-theme injection (CSS overrides, MutationObserver, user-preference patching), cached credentials, localhost fallback, improved URL rewriting.
- **Performance** — batch graph inserts, policy cache, incremental index content hashing, async SonarQube scans in the ship pipeline.

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

<sub>ax · Aero Xecution</sub>
