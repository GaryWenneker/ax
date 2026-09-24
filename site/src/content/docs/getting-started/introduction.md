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
| **Policy engine** | IDE-agnostic rules and skills in `.agents/`, matched and injected per turn |
| **Command Center** | Git watcher, quality gates, SonarQube, token savings, MCP Logging / Quality, SSE dashboard, draft PRs |

Agents query structure through MCP (`ax_explore`, `ax_preflight`, …) instead of fanning out across `grep`, `glob`, and `Read`. The win is **surgical context** — fewer tool calls, faster answers, on every codebase.

## What's new in v5.2.0

v5.2.0 makes turn memories more useful:

- **Outcomes.** A turn memory now also stores the agent's final reply as the outcome (secrets redacted). Cursor delivers it through a new `afterAgentResponse` hook, and Claude Code delivers it on `Stop`.
- **Related turns in preflight.** Preflight adds up to 3 related past turns: turns that changed a file you have open, or that match your prompt.
- **`ax_history` and `ax history`.** These list the dated turns and git commits for a file, symbol or topic, so "when did I change X" gets a direct answer.
- **Retention.** Turns are kept for 90 days, and they are backed up to `.ax/backups/` before they are deleted.

Run `ax install` again to add the new Cursor hook. See [Per-turn memories](/guides/memory/#per-turn-memories).

It also fixes install and daemon hygiene:

- **Agent configs.** The installer writes the `ax` found on your `PATH` into agent configs, instead of whichever binary ran it. It also repairs Claude hooks that point at a stale path.
- **Local data out of git.** `ax init` writes `.ax/.gitignore`, so the database, logs and backups stay out of git. The team policy in `.ax/policy/` stays committable.
- **Checkouts without `.ax/`.** Git hooks in a checkout without `.ax/`, such as a fresh worktree, no longer report a failed hook.
- **Daemon restarts.** The MCP proxy survives a daemon restart, so Cursor no longer shows "Connection closed". A daemon stops by itself when its binary is replaced, and a proxy on a newer build restarts an older daemon.

## What's new in v5.1.0

v5.1.0 saves a memory after every agent turn that changed files or made a commit, so you no longer depend on the agent calling `ax_remember`. The memory holds the prompt (first 300 characters, secrets redacted), the files changed during the turn, and the commits made. Turns that change nothing are not recorded, and ax's own runtime files under `.ax/` are not counted as changes.

Turn memories are local and quiet: `ax_recall` and `ax recall` find them, but preflight never injects them, `ax memory export` skips them, and they are deleted after 30 days. Run `ax install` again to add the turn hooks for Cursor and Claude Code. To switch them off, set `"memory": { "perTurn": false }` in `ax.json`. See [Memory](/guides/memory/#per-turn-memories).

## What's new in v5.0.3

v5.0.3 makes git auto-capture work again on macOS and Linux. ax wrote its git hooks without a `#!/bin/sh` line and without the execute bit, so git skipped them and commits were no longer saved as memories. Hooks are now written correctly, and broken ones are repaired when you run `ax init` or `ax sync` and when the MCP server starts. Run `ax capture-git --limit 100` once to backfill commits you missed.

The hooks are also quiet now. They run `ax ship --evaluate --quiet`, which prints nothing when the quality gate passes and one line when it fails. `--quiet` on any command hides ax's INFO log lines. `ax --help` links to the live docs site.

## What's new in v5.0.2

v5.0.2 fixes the docs site on phones. The menu button was a blank square (a light icon on a light background) and now shows its icon on the dark header. The header bar also runs edge to edge instead of sitting inset. The CLI behaves the same as v5.0.1.

## What's new in v5.0.1

v5.0.1 removes built-in defaults that only made sense for one team.

- **`ax docs-catalog sync` has no built-in wiki** — set `docsCatalog.wiki_remote` (and optionally `name`, `wiki_products_page`, `skill`, …) in `ax.json`. Without it, the wiki step is skipped and the local sources are still cataloged. See [`ax docs-catalog sync`](/reference/cli/#ax-docs-catalog-sync).
- **No default OneDrive share folder** — the OneDrive provider needs `share.onedrive.shareUrl` in `ax.json`; without it, sync fails with a clear message.
- **Generic PR skills** — the `pr`, `preq`, `pre-pr-check`, and `no-ab-prefix` skill templates are now in English and read the organization, project, and repo from `git remote -v`.

**Upgrade notes**

- The `ax docs-catalog sync --json` report renames `integratiePages` to `integrationPages` and `digitaleProducten` to `products`.
- A workspace that relied on the old wiki defaults must now set them in `ax.json` under `docsCatalog`. Existing catalog memories keep their ids and are updated in place.

## What's new in v5.0.0

v5.0.0 is a major release because ax now removes duplicate policy rows from your databases on its own and `ax init` seeds less into projects. Nothing on disk is deleted, and every removal is versioned.

- **One copy per rule and skill** — a rule or skill stored at the global level is no longer kept again in a project `ax.db`. The longest copy wins and moves up to `~/.ax/global.db` as a new version. The cleanup runs after sync, index, `ax global sync`, `ax install`, and in `ax web` every 10 minutes. Run it by hand with `ax policy dedup --dry-run`. See [One copy per name](/guides/policy-engine/#one-copy-per-name-dedup).
- **Read guard hook** — `ax install` adds a blocking tool hook for Cursor, Claude Code, VS Code Copilot, Codex, Gemini CLI, and Windsurf. The first whole-file read of an indexed source file, or the first search for a symbol the graph knows, is denied and points the agent at `ax_node` / `ax_callers`. The identical retry is allowed. Turn it off with `AX_READ_GUARD=off`. See [Enforcing graph-first reads](/guides/policy-engine/#enforcing-graph-first-reads).
- **Review loop** — old-coder work now runs a code-review loop between the gauntlet and EVIDENCE, one review skill per stack, until a round has zero findings. See [Review loop](/guides/policy-engine/#review-loop).
- **Colleague PR reviews** — the new `pr-review-comments` skill reviews someone else's pull request without changing their code and asks you per comment what to post. See [Reviewing a colleague's pull request](/guides/policy-engine/#reviewing-a-colleagues-pull-request).
- **Principal-level stack reviews** — the review skills of 28 stack packs (from Angular and C to Rust and TypeScript) now reach the level of `dotnet-code-review` and `nextjs-review`, both extended in this release: each has 10 to 12 checkable review sections and a fixed output format. Refresh installed stacks with `ax policy stack upgrade`.

**Upgrade notes**

- `global.db` gets a `level` column and a revisions table. The migration runs on first start.
- `ax init` no longer writes the global skills (`review-loop`, `pr-review-comments`) into `.agents/skills/`; every project gets them from the global level.
- Restore a removed project row with `ax policy restore`. Run `ax policy dedup --dry-run` first if you want to see what will change.

See [Policy Engine](/guides/policy-engine/) and [CLI](/reference/cli/).

## What's new in v4.12.0

- **Stack seeding** — `ax init` asks which language, framework, and CMS stacks to install (including on a later init). Core policy stays universal. Stacks such as `dotnet`, `react`, `nextjs`, `rust`, and `python` install only their skills and rules. See [Policy Engine](/guides/policy-engine/).
- **Configurable policy folder** — on-disk rules and skills default to `.agents`. When `policy.agentsDir` is unset, init asks and suggests `.agents`. Change it with `ax policy agents-dir` or Command Center settings.
- **Stack commands** — `ax policy stack list`, `detect`, `apply`, `remove`, `status`, and `upgrade`. A file you edited is left in place unless you pass `--force`.

See [Policy Engine](/guides/policy-engine/) and [CLI](/reference/cli/).

## What's new in v4.8.0

- **Policy revision history** — Command Center **History** keeps up to 20 hash-on-change snapshots per rule/skill (editor Save and accepted zip restores). Identical saves are skipped. See [Policy Engine](/guides/policy-engine/).
- **Zip pack integrity** — packs carry blake3 `contentHash`; restore actions are **Reject** / **Accept**. See [Policy Engine](/guides/policy-engine/).

See [Policy Engine](/guides/policy-engine/) and [Command Center](/guides/command-center/).

## What's new in v4.7.1

- **Safer policy zip restore** — new items have Skip / Install; conflicts show **Local newer** / **Package newer**. Restore skips older package bytes over a newer local file unless you overwrite. See [Policy Engine](/guides/policy-engine/).
- **macOS status bar contrast** — footer ink is computed from the painted accent (WCAG AA 4.5:1). Light system blue uses **dark** letters. CRITICAL rule `wcag-contrast`.
- **Update notice in Command Center** — `ax web` / `ax desktop` check for a newer GitHub release when the UI starts.

See [Policy Engine](/guides/policy-engine/) and [Command Center](/guides/command-center/).

## What's new in v4.7.0

- **Portable policy zip** — Command Center Policy → Rules / Skills can **compose** and **restore** `.ax-policy.zip` packs (select all/none, generated descriptions, compare badges, **Local newer** / **Package newer**, Skip/Install on new items, git-style diffs). CLI: `ax policy pack zip` and `ax policy restore [--preview]`. See [Policy Engine](/guides/policy-engine/).
- **Skill and rule groups** — lists use a shared folder catalog (collapse, **Collapse all** / **Expand all**, **Groups** filter). Optional `group` on skill (schema v18) and rule (schema v19) frontmatter.
- **Git-shared `.agents/`** — Command Center shows which rules/skills are on the git-shared team path versus private overlays.
- **Command Center polish** — policy table one-line tags, group-filter collapse, Markdown editor caret, content width on small screens, macOS theme contrast, Settings verbose MCP toggle.

See [Policy Engine](/guides/policy-engine/) and [Command Center](/guides/command-center/).

## What's new in v4.6.0

- **Graph Start here** — Command Center Graph left panel lists Leiden **subsystems** (click to filter the canvas), a **god-node tour** (Prev/Next), and **Ask the graph** prompts (the same templates as `ax report`). No LLM required.
- **Domain overlay** — toggle **Structure / Domain** for a horizontal `domain` → `flow` → `step` map from `.ax/domain-graph.json`. It does **not** change `ax.db`. Seeded `domain` skill on `ax init`; `GET`/`PUT /api/domain-graph` load and save the file.
- **`ax_insights` suggested questions** — JSON includes `suggestedQuestions` so agents can hand the same prompts to a human.

See [Architecture Insights](/guides/architecture-insights/) and [Command Center](/guides/command-center/).

## What's new in v4.5.0

- **Graph-only snippets** — `ax_explore` and `ax_context` serve source from the indexed store in `ax.db` (hash-checked, no silent disk fallback). After upgrading from a pre-v17 index, run `ax index` once to backfill.
- **Database policy storage** — `policy.storage: "database"` in `ax.json` makes SQLite the source of truth (`.mdc` export only), with `ax policy import` / `ax policy export`.

## What's new in v4.4.0

- **Guarded old-coder** — `old-coder` skill has `alwaysApply: true` (matches empty prompts); `old-coder-mandatory` adds `guard: require-skill: "old-coder"` so `ax_guard` blocks Write/Delete when the skill is missing, disabled, or not always-apply. Policy/template paths stay writable for seed repair.
- **Preflight contract** — always-apply rules **and** always-apply skills are never hard-truncated; contextual rules/skills drop first. Policy match errors return degraded JSON instead of MCP `isError`. Optional `projectPath` on `ax_preflight`.
- **Skill `alwaysApply` in Command Center** — toggle on Policy → Skills; persisted in `ax.db` (schema v16).
- **OKF export** — `ax export okf` / `ax export concepts` writes a portable Markdown concept bundle; optional Azure DevOps Wiki publish. See [OKF](/guides/okf/).

See [Policy Engine](/guides/policy-engine/) and [OKF](/guides/okf/).

## What's new in v4.3.1

- **Global old-coder policy** — `ax init` and `ax install` seed [old-coder](https://github.com/AmazingAng/old-coder) skills to `~/.ax/global_policy/` and `~/.cursor/skills/` (MIT, includes reference docs).
- **`old-coder-mandatory` rule** — CRITICAL `alwaysApply` company rule requires SPEC → gauntlet → EVIDENCE for implementation work; load full workflows via `ax_skill("old-coder")` / `ax_skill("old-coder-api")`.
- **Policy re-import on init** — every `ax init` merges all layers (including global) into `ax.db` so seeded company policy is active without manual `ax policy index --force`.

See [Policy Engine](/guides/policy-engine/).

## What's new in v4.2.0

- **Label autocomplete on Rules/Skills** — type to search and pick tags as chips (multi-label AND filter); click a tag in the table to toggle it. Saving a rule or skill requires at least one tag.
- **Expanded `azdo-fullstack` skills** — full agent workflows (when to load, checklists, hard rules) for refinement → development → testing → code review → pipelines → release. Refresh with `ax policy pack install azdo-fullstack --force`.
- **Pack install imports into the database** — in `policy.storage = database` mode, install now force-imports `.ax/policy/` into `ax.db` so Command Center and MCP see new bodies immediately.

See [Policy Engine](/guides/policy-engine/) and [Command Center](/guides/command-center/).

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
