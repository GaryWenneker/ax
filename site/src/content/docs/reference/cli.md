---
title: CLI
description: Complete reference for every ax command, argument, and flag (v5.0.3).
---

Run `ax <command> --help` for the same information from the installed binary. Global help: `ax --help`.

The MCP server (`ax serve --mcp`) is started by your agent — you do not run it manually. See [MCP Server](/reference/mcp-server/).

## Global options

| Flag | Description |
|---|---|
| `-h`, `--help` | Print help for `ax` or a subcommand |
| `-V`, `--version` | Print installed version (same as `ax version`) |

Running `ax` with **no subcommand** starts the interactive installer (same as `ax install`).

## Environment variables

| Variable | Effect |
|---|---|
| `AX_FORCE_COLOR=1` | Force ANSI colors |
| `AX_UNICODE=1` | Force Unicode spinners/glyphs on Windows |
| `AX_ASCII=1` | Force ASCII glyphs everywhere |
| `NO_COLOR` | Disable ANSI colors |
| `AX_NO_UPDATE_CHECK=1` | Skip background upgrade notices after commands |
| `AX_NO_POLICY=1` | Skip policy injection in prompt-hook |
| `AX_NO_POLICY_CAPTURE=1` | Skip directive capture hints in prompt-hook |
| `AX_READ_GUARD=off` | Disable the read-guard hook (every Read/Grep passes) |
| `AX_READ_GUARD_STATE` | Path of the read-guard state file (default `~/.ax/read-guard.json`) |
| `AX_POLICY_MAX_CHARS` | Cap contextual policy inject (default `16000`). Always-apply rules are never hard-truncated. |
| `AX_TELEMETRY=0` / `DO_NOT_TRACK=1` | Disable anonymous telemetry |
| `AX_MS_CLIENT_ID` | Optional custom Azure AD public client ID for OneDrive policy share — defaults to the built-in Microsoft app if unset |
| `AX_OFFLOAD_URL`, `AX_OFFLOAD_KEY`, … | Override explore offload settings |

## Configuration files

| File | Scope | Contents |
|---|---|---|
| `~/.ax/config.json` | Global | `index`, `offload`, `policy.storage` |
| `<project>/ax.json` | Per-project | `share`, policy overrides, `memory.perTurn` — each project has its own remote share config |
| `<project>/.ax/` | Per-project | `ax.db`, lock file, optional `policy/` |

See [Configuration](/getting-started/configuration/) for the full schema.

---

## Install & uninstall

### `ax` / `ax install`

Interactive installer — writes MCP config for detected AI agents (Cursor, Claude Code, Codex, opencode, Gemini CLI, Antigravity, Kiro, Hermes, VS Code Copilot, Takumi 匠, Windsurf, Zed). Does **not** index a project. Prompt-hook and stop-hook are Claude Code-specific. The turn hooks for [per-turn memories](/guides/memory/#per-turn-memories) are added for Cursor and Claude Code. The [read-guard hook](#ax-read-guard) is added for every agent that can block a tool call: Cursor, Claude Code, VS Code Copilot, Codex, Gemini CLI, and Windsurf. See [Integrations](/reference/integrations/) for per-agent details.

| Argument / flag | Type | Description |
|---|---|---|
| `--yes` | flag | Non-interactive: skip prompts, install detected agents |
| `--all` | flag | Configure every supported agent, not only detected ones |
| `--target <id>` | string | Wire a single agent (e.g. `takumi`, `vscode`, `cursor`); ids are case-insensitive |
| `--path <dir>` | string | Project root for workspace MCP files (default: current directory). Takumi passes this explicitly. |

```bash
ax install
ax install --yes
ax install --yes --all
ax install --target takumi
ax install --yes --target takumi --path .
```

### `ax uninstall`

Remove ax entries from agent MCP configuration files. Does not delete `~/.ax` or project `.ax/` indexes.

```bash
ax uninstall
```

---

## Project lifecycle

### `ax init [path]`

Initialize a project: create `.ax/` (database, lock, `ship.toml`), index the project, install git hooks, install the Cursor savings hook (`ax savings hook install`), import Claude and Cursor savings logs (`ax savings import --all`), then offer the agent installer. A workspace init runs the savings steps once, after every member.

On **first init**, runs a full index. If `.ax/ax.db` already exists, runs an incremental `ax sync` instead — use `ax index` when you need a full rebuild.

| Argument / flag | Type | Description |
|---|---|---|
| `path` | optional | Project root (default: current directory) |
| `--workspace` | flag | Discover monorepo members (Cargo workspace + nested `.ax/`), write `members` to `ax.json`, and init each member |

```bash
ax init
ax init ./services/api
ax init --workspace
```

### `ax uninit [path]`

Delete the `.ax/` directory and remove git sync hooks. Permanently removes the local index.

| Argument | Type | Description |
|---|---|---|
| `path` | optional | Project root (default: current directory) |

```bash
ax uninit
ax uninit ./old-project
```

### `ax index [path]`

Full re-index from scratch (scan → extract → resolve). Use when the watcher is off or after large git operations. Dot-directories such as `.git` and `.ax` are skipped; **`.scripts/` is indexed** (for example `.scripts/wcag`).

| Argument / flag | Type | Description |
|---|---|---|
| `path` | optional | Project root |
| `--force` | flag | Clear the database before indexing |
| `--quiet` | flag | No progress bar or summary |
| `--verbose` | flag | Reserved for extra diagnostics |
| `--all` | flag | Index every workspace member listed in root `ax.json` |

```bash
ax index
ax index ./services/api --force
ax index --quiet
ax index --all
```

### `ax sync [path]`

Incremental update — re-parses only changed files.

| Argument / flag | Type | Description |
|---|---|---|
| `path` | optional | Project root |
| `--quiet` | flag | No progress bar or summary |
| `--watch` | flag | Watch filesystem and auto-sync until Ctrl+C |
| `--all` | flag | Sync every workspace member listed in root `ax.json` |

Also refreshes git hooks when the memory-capture line is missing (idempotent upgrade path).

```bash
ax sync
ax sync --quiet
ax sync --watch
ax sync --all
```

### `ax watch [path]`

Alias for `ax sync --watch`.

| Argument / flag | Type | Description |
|---|---|---|
| `path` | optional | Project root |
| `--quiet` | flag | No progress bar or summary |

```bash
ax watch
ax watch --quiet
```

### `ax status [path]`

Index statistics: node/edge/file counts, **document inventory by extension** (markdown, office, PDF, other opaque types), unresolved refs, pending sync, and last indexed time.

| Argument / flag | Type | Description |
|---|---|---|
| `path` | optional | Project root |
| `--json` | flag | Machine-readable JSON (`stats.docsByExtension`, `pendingFiles`) |

```bash
ax status
ax status --json
```

### `ax unlock [path]`

Force-remove a stale `.ax/ax.lock` and stop orphaned `ax.exe` processes. Prefer `ax daemon restart` first — writers normally wait up to 180s for the lock and clear dead PIDs automatically.

| Argument | Type | Description |
|---|---|---|
| `path` | optional | Project root |

```bash
ax unlock
ax unlock ./repo-with-stale-lock
```

---

## Graph query

### `ax query <text>`

Full-text symbol search (FTS5). For natural-language questions use `ax explore`.

| Argument / flag | Type | Description |
|---|---|---|
| `text` | required | Search string |
| `--kind` | string | Filter by node kind (`function`, `class`, `file`, …) |
| `--limit` | number | Max results |
| `--json` | flag | JSON array output |

```bash
ax query UserService
ax query "handleRequest" --kind function --limit 10
ax query auth --json
```

### `ax explore [query…]`

Natural-language explore — same output shape as the `ax_explore` MCP tool. Optional BYO LLM offload via `ax offload`.

| Argument / flag | Type | Description |
|---|---|---|
| `query` | optional (words) | Question or symbol names (multi-word allowed) |
| `--json` | flag | Structured JSON |

```bash
ax explore "how does auth flow work"
ax explore ax_preflight token savings --json
```

### `ax node [name]`

One symbol's details, or a file with line numbers and dependents — same as `ax_node` MCP.

| Argument | Type | Description |
|---|---|---|
| `name` | optional | Symbol name or file path |

```bash
ax node handleRequest
ax node crates/ax-cli/src/main.rs
```

### `ax files`

List indexed files and detected languages.

| Flag | Type | Description |
|---|---|---|
| `--format` | string | Output format (e.g. `tree` when supported) |
| `--json` | flag | JSON with paths and languages |

```bash
ax files
ax files --format tree --json
```

### `ax context <task>`

Build task-oriented markdown context for agent prompts.

| Argument | Type | Description |
|---|---|---|
| `task` | required | Task description |

```bash
ax context "add rate limiting to the login endpoint"
```

### `ax callers <symbol>`

Find symbols that call the given function/method/class. Output is JSON.

| Argument | Type | Description |
|---|---|---|
| `symbol` | required | Symbol name |

```bash
ax callers handleRequest
```

### `ax callees <symbol>`

Find symbols called by the given function/method. Output is JSON.

| Argument | Type | Description |
|---|---|---|
| `symbol` | required | Symbol name |

```bash
ax callees run_preflight
```

### `ax impact <symbol>`

Blast-radius subgraph — what breaks if you change this symbol. Output is JSON.

| Argument | Type | Description |
|---|---|---|
| `symbol` | required | Symbol name |

```bash
ax impact AuthService
```

### `ax cycles`

List call-graph cycles: strongly connected components with more than one symbol, printed as `a → b` chains. Same engine as the `ax_cycles` MCP tool.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--limit` | number | `50` | Max cycles to report |
| `--json` | flag | — | Machine-readable JSON |

```bash
ax cycles
ax cycles --limit 10 --json
```

### `ax path <from> <to>`

Shortest path between two symbols over Calls / References edges. Prints the chain, or an error when either symbol is not in the index. Same engine as the `ax_path` MCP tool.

| Argument / flag | Type | Description |
|---|---|---|
| `from` | required | Start symbol |
| `to` | required | End symbol |
| `--json` | flag | Machine-readable JSON |

```bash
ax path handleRequest saveUser
ax path handleRequest saveUser --json
```

### `ax api <module>`

Public API surface of a module: exported symbols whose file path starts with the given prefix, with kind and file. Same engine as the `ax_api` MCP tool.

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `module` | required | — | Module name or path prefix (for example `src/auth`) |
| `--limit` | number | `200` | Max exported symbols |
| `--json` | flag | — | Machine-readable JSON |

```bash
ax api src/auth
ax api crates/ax-share/src --limit 50 --json
```

---

## Architecture insights

Whole-graph analysis built on the same engine as the `ax_insights` / `ax_report` MCP tools and the Command Center **Graph** page. See [Architecture Insights](/guides/architecture-insights/).

### `ax insights [path]`

Detect subsystems (Leiden community detection), rank **god nodes** (most-connected concepts by in+out degree), and flag **surprising connections** (edges that cross both a community and a top-level module boundary).

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `path` | optional | cwd | Project root |
| `--resolution` | number | `1.0` | Cluster granularity — higher yields more, smaller communities |
| `--god-limit` | number | `20` | Max god nodes to show |
| `--surprising-limit` | number | `20` | Max surprising connections to show |
| `--json` | flag | — | Machine-readable JSON |

Community assignments are persisted (`node_communities` table) and only recomputed after `ax index` / `ax sync` or when you re-run with a different `--resolution`.

```bash
ax insights
ax insights --resolution 1.4 --god-limit 30
ax insights --json
```

### `ax domain [path]`

Build a **domain overlay** from the indexed graph (Leiden communities → domains, call chains → flows/steps, cross-community edges → `cross_domain`) and write `.ax/domain-graph.json`. Does **not** change `ax.db`. No LLM. Command Center Graph → Domain reads this file.

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `path` | optional | cwd | Project root |
| `--resolution` | number | `1.0` | Cluster granularity — higher yields more, smaller communities |
| `--json` | flag | — | Print the overlay JSON |
| `--dry-run` | flag | — | Do not write the file |

```bash
ax domain
ax domain --resolution 1.4
ax domain --json
ax domain --dry-run
```

Empty index exits non-zero (`run ax index first`). Empty Domain view in Command Center points at this command.

### `ax report [path]`

Render a full Markdown architecture report: god nodes, communities with member counts, surprising connections, dead code, an unresolved-refs summary, and a set of suggested questions templated from the top god nodes and communities.

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `path` | optional | cwd | Project root |
| `--out` | string | `AX_REPORT.md` | Output file at project root |
| `--resolution` | number | `1.0` | Cluster granularity |
| `--stdout` | flag | — | Print to stdout instead of writing a file |

```bash
ax report
ax report --out docs/ARCHITECTURE.md
ax report --stdout
```

### `ax validate [path]`

Graph hygiene report: isolated symbols (no call or import edges), dangling edges (an edge whose target is missing), and orphan docs (Markdown with no documents / references edges). Use `--ci` to fail a pipeline on a dirty graph.

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `path` | optional | cwd | Project root |
| `--ci` | flag | — | Exit non-zero when dangling edges or isolated symbols exist |
| `--json` | flag | — | Machine-readable JSON |

```bash
ax validate
ax validate --json
ax validate --ci
```

### `ax export okf [path]` / `ax export concepts [path]`

Export an **Open Knowledge Format (OKF)** Markdown bundle from the indexed graph — one YAML-frontmatter page per concept with Calls / Called by links. Configure the relative output path under `okf` in `ax.json` (default `.ax/knowledge`). The same export can be started from Command Center **Settings → Open Knowledge Format (OKF)**. See [Open Knowledge Format (OKF)](/guides/okf/).

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `path` | optional | cwd | Project root |
| `--out` | path | `okf.outDir` or `.ax/knowledge` | Output directory |
| `--limit` | int | `0` | Max concepts (`0` = all) |
| `--check` | flag | — | Validate `index.md` + relative links |
| `--ci` | flag | — | With `--check`: exit non-zero on issues |
| `--publish-wiki` | flag | — | Publish bundle to `okf.azdoWiki` git remote |
| `--dry-run` | flag | — | With `--publish-wiki`: preview only |
| `--no-push` | flag | — | Commit wiki locally without push |
| `--json` | flag | — | Machine-readable output for check/publish |

```bash
ax export okf
ax export okf --out knowledge
ax export okf --check --ci
ax export okf --publish-wiki --dry-run
ax export concepts
```

`ax export concepts` is an alias for `ax export okf`.

### `ax export graph [path]`

Export the knowledge graph for external tools. Includes Leiden community ids, degree, and god-node flags. Command Center **Graph** can download the same formats (except interactive HTML) via **Export → Download**.

| Argument / flag | Type | Description |
|---|---|---|
| `path` | optional | Project root |
| `--format` | string | `html` \| `json` \| `dot` \| `graphml` \| `gexf` \| `cypher` \| `mermaid` \| `plantuml` (default: `json`) |
| `--out` | path | Output file (default depends on format) |
| `--resolution` | float | Cluster granularity (default: `1.0`) |
| `--limit` | int | Max nodes by degree (default: `3000`) |

```bash
ax export graph --format json
ax export graph --format graphml --out graph.graphml
ax export graph --format cypher --out import.cypher
ax export graph --format mermaid --out graph.mmd
```

### `ax export graph-html [path]`

Export the graph as a single self-contained, interactive HTML file (inline JSON + a small force-directed renderer) — portable, no server required. Node color = community, node size = degree, docs render as distinct squares.

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `path` | optional | cwd | Project root |
| `--out` | string | `graph.html` | Output file |
| `--resolution` | number | `1.0` | Cluster granularity |
| `--limit` | number | `3000` | Max nodes to include (top by degree) |

```bash
ax export graph-html
ax export graph-html --out graph.html --limit 1500
```

---

## Memory vault

See the [Memory vault guide](/guides/memory/).

### `ax remember <text>`

Store a durable project memory (decision, fix, convention). Flags similar existing memories so contradictions get updated instead of duplicated.

| Argument / flag | Type | Description |
|---|---|---|
| `text` | required | The memory content — what to remember and why |
| `--title` | string | Short title (defaults to first line) |
| `--kind` | string | `decision` \| `bug_fix` \| `architecture` \| `convention` \| `note` |
| `--tag` | string (repeat) | Tag |
| `--file` | string (repeat) | Related file path |
| `--json` | flag | JSON output |

```bash
ax remember "We use tiktoken o200k_base for token counts" --kind decision --tag tokenizer
ax remember "Sonar proxy strips Content-Encoding after decompression" --kind bug_fix --file crates/ax-web/src/sonar_proxy.rs
```

### `ax recall <query>`

Hybrid search (full-text + vector similarity, confidence-decay weighted) over project memories.

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `query` | required | — | Free-text query |
| `--limit` | number | `5` | Max results (≤50) |
| `--json` | flag | — | JSON output |

```bash
ax recall "why tiktoken"
ax recall sonar proxy --limit 10 --json
```

### `ax capture-git`

Mine recent non-merge git commits into memories (the "why" behind changes). Skips trivial messages and already-captured commits; safe to re-run.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--limit` | number | `100` | Number of commits to scan |
| `--quiet` | flag | — | No output (used by git hooks) |
| `--json` | flag | — | JSON output |

Runs automatically from **post-commit** and **post-merge** git hooks after `ax init`. Skips trivial messages and already-captured commits.

```bash
ax capture-git
ax capture-git --limit 50
ax capture-git --limit 1 --quiet    # same as the post-commit hook
```

### `ax memory export`

Export memories that carry a tag (default: `shared`) to `.ax/memory/shared.jsonl` for team git sync.

| Flag | Default | Description |
|---|---|---|
| `--tag` | `shared` | Only export memories with this tag |
| `--out` | `.ax/memory/shared.jsonl` | Output path |
| `--quiet` | off | No summary (for git hooks) |

```bash
ax remember "Adopt GraphQL federation" --kind decision --tag shared
ax memory export
ax memory export --tag team --out ./shared-memories.jsonl
```

### `ax memory import`

Import JSONL memories (upsert by id). Used after `git pull` or via the post-merge hook when `"memorySync": true` is set in `ax.json`.

```bash
ax memory import
ax memory import --path ./shared-memories.jsonl
```

### `ax docs-catalog sync`

Build `documentation-catalog` memories from an optional git wiki, `.docs/`, agent skills, and script READMEs, import them into `ax.db`, and run `ax sync`. The Command Center Memory page (**Sync docs catalog**) uses the same engine.

| Flag | Default | Description |
|---|---|---|
| `--skip-wiki-pull` | off | Use the existing wiki clone without `git pull` |
| `--dry-run` | off | Write the JSONL only; skip the import and graph sync |
| `--json` | off | Print the sync report as JSON |

```bash
ax docs-catalog sync
ax docs-catalog sync --dry-run --json
ax docs-catalog sync --skip-wiki-pull
```

Configure it in `ax.json` under `docsCatalog`. Every key is optional, and there are no built-in wiki defaults: without `wiki_remote` the wiki step is skipped (`wikiAction: "not-configured"`).

| Key | Default | Description |
|---|---|---|
| `name` | project folder name | Used in memory titles |
| `wiki_remote` | none | Git URL of the wiki to clone and pull |
| `wiki_local` | `.current/wiki` | Local clone path |
| `wiki_apps_subdir` | `""` | Wiki subfolder to scan for sections |
| `wiki_root_url` | `wiki_remote` | Browser URL stored in the catalog |
| `wiki_integrations_dir` | `Integrations` | Folder of integration pages under `wiki_apps_subdir` |
| `wiki_products_page` | none | Markdown page whose table rows list products |
| `wiki_products_skip` | `[]` | First-cell values to leave out of the products list |
| `skill` | none | Skill name mentioned in the refresh instructions |
| `jsonl_path` | `.ax/memory/documentation-catalog.jsonl` | Output file |
| `docs_root` / `skills_root` / `scripts_root` | `.docs` / `.agents/skills` / `.scripts` | Workspace sources |

```json
{
  "docsCatalog": {
    "name": "Contoso",
    "wiki_remote": "https://dev.azure.com/contoso/Docs/_git/Docs.wiki",
    "wiki_apps_subdir": "Applications",
    "wiki_products_page": "Products.md"
  }
}
```

---

## Global index

### `ax global`

Aggregate project indexes into one machine-wide database, `~/.ax/global.db`. `sync` copies nodes, files, and edges from a project's `.ax/ax.db`. Files with the same hash in more than one project are recorded as shared knowledge and cross-project references. Set `AX_GLOBAL_DB` to use a different database path.

| Subcommand | Description |
|---|---|
| `init` | Create `~/.ax/global.db` (or `AX_GLOBAL_DB`) and apply the schema |
| `sync [path]` | Copy this project's `.ax/ax.db` into the global database (default: cwd) |
| `status` | Show the database path and project, node, document, and shared-knowledge counts |

```bash
ax global init
ax global sync
ax global sync ./services/api
ax global status
```

---

## Git & testing

### `ax diff`

Symbol-level blast radius for the current git diff vs a base branch.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--base` | string | `main` | Base branch or ref |
| `--json` | flag | — | JSON output |

```bash
ax diff --base main
ax diff --base origin/main --json
```

### `ax test-impact`

Test-function impact analysis (git diff + TIA graph).

| Flag | Type | Default | Description |
|---|---|---|---|
| `--base` | string | `main` | Base branch or ref |
| `--json` | flag | — | JSON output |

```bash
ax test-impact --base main
ax test-impact --base develop --json
```

### `ax affected [files…]`

Reverse impact: find **test files** affected by changes. See [Affected Tests in CI](/guides/affected-tests/).

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `files` | optional (repeat) | — | Changed source file paths |
| `--stdin` | flag | — | Read changed paths from stdin |
| `--base` | string | `main` | Base branch when no files given |
| `-d`, `--depth` | number | `5` | Max dependency traversal depth |
| `-f`, `--filter` | string | — | Glob filter for test files |
| `-j`, `--json` | flag | — | JSON output |
| `-q`, `--quiet` | flag | — | File paths only |

```bash
ax affected src/auth/login.ts
git diff --name-only main | ax affected --stdin
```

---

## Command Center

### `ax ship [path]`

Git-aware quality gates, SSE dashboard, draft PRs. See [Command Center](/guides/command-center/).

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `path` | optional | cwd | Project root |
| `--watch` | flag | — | Watch git events and open dashboard |
| `--evaluate` | flag | — | Run one quality-gate evaluation |
| `--ci` | flag | — | Headless CI: evaluate, print JSON on stdout, summary on stderr, **exit 1** if the gate failed |
| `--draft` | flag | — | Create draft PR after quality gate |
| `--title` | string | — | PR title |
| `--port` | number | `7070` | Dashboard port |
| `--open` | flag | — | Open browser |
| `--auto-commit` | flag | — | Force-enable Aider-style checkpoint commit before this evaluation, overriding `.ax/ship.toml` `[auto_commit]` for this run only |
| `--revert-on-fail` | flag | — | With `--auto-commit`, `git reset --mixed` the checkpoint if the quality gate fails (file contents stay on disk, uncommitted) |
| `--quiet` | flag | — | With `--evaluate`: print nothing when the gate passes and one line on stderr (`ax: quality gate failed: <steps>`) when it fails. Exits 0 whether the gate passes or fails. The git hooks ax installs use this |

```bash
ax ship --watch --open
ax ship --evaluate
ax ship --evaluate --quiet       # what the git hooks run
ax ship --ci                     # CI / GitHub Actions
ax ship --evaluate --auto-commit --revert-on-fail
ax ship --draft --title "feat: memory vault"
ax web --open                    # Command Center without git watch
```

Example workflows:

- [GitHub Actions](https://github.com/GaryWenneker/ax/blob/main/docs/examples/github-actions-ship.yml)
- [GitLab CI](https://github.com/GaryWenneker/ax/blob/main/docs/examples/gitlab-ci-ship.yml)
- [Azure Pipelines](https://github.com/GaryWenneker/ax/blob/main/docs/examples/azure-pipelines-ship.yml)

---

### `ax share [path]`

Share Command Center on the LAN with a bearer token (read-only session).

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `path` | optional | cwd | Project root |
| `--port` | number | `7070` | Listen port |
| `--bind` | string | `0.0.0.0` | Bind address |
| `--open` | flag | — | Open browser with token URL |
| `--token` | string | random | Share token |

```bash
ax share --open
```

### `ax lsp status` / `ax lsp enrich`

Optional Language Server enrichment. `enrich` calls `textDocument/definition` on
unresolved refs and writes edges with confidence `exact`. Status uses `rustup which`
and project `node_modules/.bin`, not PATH shims alone.

```bash
ax lsp status
ax lsp enrich --limit 100
rustup component add rust-analyzer
```

---

### `ax stop-hook`

Claude Code `Stop` / `SubagentStop` hook target — internal command wired automatically by `ax install` for Claude Code, not meant to be run manually. Reads Claude's JSON payload on stdin, runs `ax_guard`-equivalent checks against every uncommitted file (`git status --porcelain`), and on a CRITICAL violation prints `{"decision": "block", "reason": "..."}` so Claude fixes the issue before the turn actually ends. Honors `stop_hook_active` to avoid infinite loops and no-ops when there's no indexed policy. It also writes the [per-turn memory](/guides/memory/#per-turn-memories) for the turn.

| Env var | Effect |
|---|---|
| `AX_NO_STOP_HOOK=1` | Disable — hook exits immediately without checking or writing a turn memory |

```bash
# Wired automatically:
ax install                       # adds Stop + SubagentStop hooks for Claude Code
```

---

### `ax read-guard`

Pre-tool hook target, wired automatically by `ax install` and not meant to be run by hand. Reads the IDE's hook JSON on stdin and steers agents from raw file reads to the ax graph:

- The **first whole-file read** of an indexed source file in a conversation is denied. The reply lists up to eight of the file's symbols as `ax_node("…")` calls.
- The **first search for a bare symbol name** that the graph knows (`Grep`, `rg`, `grep`, `git grep`) is denied. The reply lists the matches with file and line and points to `ax_node`, `ax_callers`, and `ax_impact`.
- **Repeating the identical call is allowed**, so an agent that really needs the raw file (for example right before an edit) is never stuck.

Partial reads (`offset`/`limit`, `head`, `sed -n`, a piped `cat`) pass where the IDE sends the range to the hook. Cursor's `preToolUse` payload carries only `file_path`, so in Cursor a partial Read of an indexed file is denied once too, and the identical retry passes. Non-source files, regex or free-text searches, symbols the graph does not know, and anything outside an indexed project always pass. Shell commands are only inspected for their first simple command. Every error allows the call. Memory of denied calls is kept per conversation for 4 hours in `~/.ax/read-guard.json`.

| `--ide` | Hooks file | Event (matcher) |
|---|---|---|
| `cursor` | `~/.cursor/hooks.json` | `preToolUse` (`Read\|Grep\|Shell`) |
| `claude` | `~/.claude/settings.json` (Claude Code and VS Code Copilot) | `PreToolUse` (`Read\|Grep\|Bash`) |
| `codex` | `~/.codex/hooks.json` | `PreToolUse` (`^Bash$`). Codex only runs new hooks after you trust them once with `/hooks` |
| `gemini` | `~/.gemini/settings.json` | `BeforeTool` (`read_file\|search_file_content\|grep\|run_shell_command`) |
| `windsurf` | `~/.codeium/windsurf/hooks.json` | `pre_read_code`, `pre_run_command` |

Zed, Continue, Kiro, opencode, Antigravity, Hermes, and Takumi 匠 have no blocking tool hook; they get the graph-first instructions only. A hooks file that is not valid JSON is left unchanged and reported by `ax install`. `ax uninstall` removes only the read-guard and turn-hook entries.

| Env var | Effect |
|---|---|
| `AX_READ_GUARD=off` | Disable — every call passes (`0`, `false`, `no` also work) |
| `AX_READ_GUARD_STATE` | Alternative state file path |

---

## Daemon

### `ax daemon [path] [status|stop|restart]`

MCP background daemon control (shared index connection per project). Cursor / Takumi attach as stdio proxies; `restart` clears a stuck daemon and stale locks without killing every `ax.exe` (unlike `ax unlock`).

| Argument / subcommand | Description |
|---|---|
| `path` | Optional project root (before subcommand) |
| `status` | Show pid, port, socket from `.ax/daemon.json` (default) |
| `stop` | Stop the running daemon |
| `restart` | Stop + start a fresh daemon (Command Center **Reload MCP** uses the same path) |

```bash
ax daemon status
ax daemon stop
ax daemon restart
ax daemon ./repo status
```

---

## Version & upgrade

### `ax version`

Print installed version. Aliases: `ax -V`, `ax --version`.

```bash
ax version
ax --version
```

### `ax upgrade [version]`

Self-update from [getax.wenneker.io/releases/latest.txt](https://getax.wenneker.io/releases/latest.txt) (GitHub fallback). Non-interactive — no confirmation prompt.

| Argument / flag | Type | Description |
|---|---|---|
| `version` | optional | Pin a release tag (e.g. `v3.0.0`) |
| `--check` | flag | Check for updates without installing |
| `--local [archive]` | flag / path | Install from local `dist/ax-<platform>.zip` (maintainers); optional explicit archive path |

```bash
ax upgrade
ax upgrade v3.0.0
ax upgrade --check
ax upgrade --local
ax upgrade --local dist/ax-win32-x64.zip
```

On Windows the process exits immediately; a detached helper finishes the binary swap. Open a new terminal and run `ax version` to verify.

---

## Telemetry

### `ax telemetry [on|off|status]`

Anonymous usage telemetry (command names and coarse buckets — never source code or paths).

| Argument | Description |
|---|---|
| `on` | Enable telemetry |
| `off` | Disable telemetry |
| `status` | Show current setting (default when omitted) |

```bash
ax telemetry status
ax telemetry on
ax telemetry off
```

---

## Context savings

### `ax savings`

Estimated context-token savings from MCP graph queries. Stored locally in `~/.ax/usage.db`. Each `ax_explore` / graph MCP call logs response size and a counterfactual file-read estimate.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--period` | string | `month_to_date` | `week` \| `month_to_date` \| `month` \| `year` \| `custom` |
| `--from` | `YYYY-MM-DD` | — | Start date (required for `custom`) |
| `--to` | `YYYY-MM-DD` | — | End date (optional for `custom`) |
| `--json` | flag | — | JSON summary with per-tool and daily breakdown |

```bash
ax savings
ax savings --period week --json
ax savings import --all
ax savings import --claude --cursor
ax savings tag-session --session-id <uuid> --model composer-2.5-fast
ax savings hook install
```

| Subcommand | Description |
|---|---|
| `import --all` | Import Cursor + Claude Code session transcripts |
| `tag-session` | Manually record model name for a session id |
| `hook install` | Install Cursor sessionStart hook into `~/.cursor/hooks/` |

The **Savings** page in `ax web` (enable in Settings) exposes the same filters. See [Token savings](/guides/token-savings/).

### `ax pricing`

Daily model price sync from [OpenRouter](https://openrouter.ai/models) (no API key). Snapshots land in `~/.ax/usage.db` and drive Savings USD estimates plus the Command Center **Prices** page. `ax web` / MCP auto-sync once per calendar day; use this command to force or inspect.

| Subcommand | Description |
|---|---|
| `sync` | Fetch today's prices (`--force` re-fetches even if already synced) |
| `status` | Last sync, row counts |
| `list` | Latest OpenRouter rates (`--json`) |
| `history <model>` | Daily series for a model id/substring (`--days`, `--json`) |

```bash
ax pricing sync
ax pricing sync --force
ax pricing status
ax pricing list
ax pricing list --json
ax pricing history claude-sonnet --days 30
```

Optional overrides in `~/.ax/pricing.toml` always win over synced rates.

### `ax mcp audit`

Correlate `<project>/.ax/mcp-verbose-*.log` (daily files) with a Cursor agent transcript and score MCP quality (preflight, enrichment, explore-before-grep, correlation). Same engine powers the Command Center **Quality** status-bar chip and slide-out. Persists a snapshot to `.ax/audit/latest.json`. Exit code `2` when critical findings are present.

Scoring notes:

- Enrichment attaches `enrich` / `enrich done` lines (including `final_inject_chars`) to the open `ax_preflight` cluster so empty-inject false positives are avoided.
- Cursor transcripts often lack JSON timestamps; the auditor parses embedded `<timestamp>…</timestamp>` markers and carries them onto following tool calls so the rolling window matches verbose activity.
- Untimed transcripts (no embed/JSON ts) are capped and capacity-matched against the verbose window; when verbose is quiet in-window, the whole-session transcript tail is dropped so idle windows do not false-flag `UncorrelatedTool` / `VerboseGap`.
- Tool mix prefers the richer of verbose vs transcript counts for explore/graph/preflight/guard (`CallDynamicTool` included).
- Default session pick prefers the recent transcript with the most ax tool calls (avoids linking an empty newest chat).
- Verbose lines may include `session=<uuid>` (from the Cursor sessionStart hook / `AX_CURSOR_SESSION_ID`) for tighter transcript↔log correlation — run `ax savings hook install` once per machine.
- MCP error clusters that recover with a successful same-tool retry within 60s do not penalize the score.
- When verbose log files exist but have no clusters in the window, `UncorrelatedTool` is medium (widen `--window-minutes` or pass `--session`) instead of critical "enable verbose".

| Flag | Type | Default | Description |
|---|---|---|---|
| `--session` | uuid \| path | latest transcript | Cursor session id or `.jsonl` path |
| `--window-minutes` | number | `30` | Rolling window when no `--session` |
| `--json` | flag | — | Machine-readable `QualitySnapshot` |

```bash
ax mcp audit
ax mcp audit --window-minutes 60
ax mcp audit --session <uuid>
ax mcp audit --session path/to/transcript.jsonl --json
```

---

## Cursor auth switching

### `ax cursor auth`

Save and restore Cursor IDE auth sessions for fast subscription switching. Snapshots `cursorAuth/*` keys from `%APPDATA%\\Cursor\\User\\globalStorage\\state.vscdb` plus `auth.json` into `~/.ax/cursor-auth/`.

| Subcommand | Description |
|---|---|
| `status` | Show live plan, email, login method |
| `list` | List saved profiles |
| `save <name>` | Snapshot current Cursor auth |
| `use <name>` | Apply a saved profile (restart Cursor after) |
| `show <name>` | Inspect a saved profile without applying |

| Flag | Applies to | Description |
|---|---|---|
| `--label` | `save` | Human-readable profile label |
| `--from-auth-json` | `save` | Bootstrap from stale `auth.json` only |
| `--email`, `--membership`, `--subscription-status`, `--sign-up-type` | `save` | Override metadata when bootstrapping |
| `--force` | `use` | Apply while Cursor is running (still restart after) |
| `--json` | all | JSON output |

Close Cursor before `use`, then restart Cursor after switching.

```bash
ax cursor auth status
ax cursor auth save enterprise --label "Work"
ax cursor auth save personal --from-auth-json --email you@gmail.com --membership pro_plus
ax cursor auth list
ax cursor auth use personal
ax cursor auth use enterprise
```

---

## Explore offload

### `ax offload`

Configure optional LLM offload for `ax explore` (BYO OpenAI-compatible API). Stored in `~/.ax/config.json`.

#### `ax offload status`

Show current endpoint and key env var name.

```bash
ax offload status
```

#### `ax offload set-endpoint <url>`

| Argument / flag | Type | Description |
|---|---|---|
| `url` | required | Base URL (must end with `/v1`) |
| `--key-env` | string | Environment variable holding the API key |

```bash
ax offload set-endpoint https://api.openai.com/v1 --key-env OPENAI_API_KEY
```

#### `ax offload clear`

Remove offload configuration.

```bash
ax offload clear
```

---

## Web UI

### `ax web [path]`

Local web UI — graph browser, policy editor, token usage dashboard, Command Center tab.

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `path` | optional | cwd | Project root |
| `--port` | number | `7070` | Listen port |
| `--open` | flag | — | Open browser after start |

Set `AX_WEB_READONLY=1` for browse-only mode.

On start, `ax web` checks GitHub for a newer release (skips the 24-hour `~/.ax/update-check.json` cache) and prints the boxed notice to stderr **before** the server blocks. Disable with `AX_NO_UPDATE_CHECK=1`. Short commands still check after they exit.

```bash
ax web
ax web ./my-repo --port 8080 --open
```

### `ax desktop [path]`

Native wgpu Command Center (egui/eframe). Embeds `ax-web` in-process — same `/api` as the browser UI, no browser required. See [Desktop Client](/guides/desktop-client/).

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `path` | optional | cwd | Project root (must contain `.ax/ax.db`) |
| `--port` | number | `7070` | Embedded `ax-web` listen port |
| `--bind` | string | `127.0.0.1` | Bind address |

```bash
ax desktop
ax desktop --port 17070
ax desktop ./my-repo --port 17070 --bind 127.0.0.1
```

### Desktop client binary (`ax-desktop`)

Standalone binary from `crates/ax-desktop-client` (same UI as `ax desktop`):

```bash
cargo run -p ax-desktop-client -- .
./target-dev/release/ax-desktop . --port 17070
```

---

## Policy

Policy commands manage `.agents/` rules and skills (legacy `.ax/policy/{rules,skills}` still indexes). See [Policy Engine](/guides/policy-engine/).

Most policy subcommands accept an optional `[path]` project root (default: current directory).

### `ax policy index [path]`

Index `.ax/policy/` into SQLite. In **database** mode without `--force`, shows DB counts only.

| Flag | Description |
|---|---|
| `--force` | Re-import from disk (database mode) or full replace (files mode) |

```bash
ax policy index
ax policy index --force
```

### `ax policy import [path]`

Import `.mdc` / `SKILL.md` from disk into database (merge — keeps DB-only rows).

```bash
ax policy import
ax policy import ./my-project
```

### `ax policy dedup [path]`

Remove project rules and skills that `~/.ax/global.db` already holds. A longer project copy is first promoted into the global row as a new version. Every removed body is kept as a revision; files on disk are not touched. This also runs automatically after sync, policy index, `ax global sync`, `ax install`, and in `ax web` every 10 minutes. Exits non-zero when the run stops on an error. `--dry-run` on a `global.db` from before this feature reports that the schema is older; any real run upgrades it.

```bash
ax policy dedup --dry-run
ax policy dedup
ax policy dedup --json
```

### `ax policy pull <git-url> [path]`

Clone a remote git policy registry into `.ax/policy/vendored/<name>/`, copy `rules/` and `skills/` into the project policy tree, and re-index.

| Argument / flag | Type | Description |
|---|---|---|
| `git-url` | required | HTTPS or SSH git URL |
| `--name` | string | Vendor subdirectory name (default: repo name) |

```bash
ax policy pull https://github.com/acme/ax-org-policy.git
ax policy pull git@github.com:acme/ax-org-policy.git --name org
```

### `ax policy export [path]`

Export database policy to files.

| Flag | Default | Description |
|---|---|---|
| `--out` | `.ax/policy/export` | Output directory |

```bash
ax policy export
ax policy export --out ./policy-backup
```

### `ax policy match <prompt> [path]`

Test which rules/skills match a prompt.

| Argument / flag | Type | Description |
|---|---|---|
| `prompt` | required | Prompt text to match |
| `--file` | string (repeat) | Open/changed file paths for glob matching |
| `--json` | flag | JSON output |

```bash
ax policy match "deploy to production" --file src/ship.rs
ax policy match "utf8 bom" --json
```

### `ax policy rules [path]`

List indexed rules.

| Flag | Description |
|---|---|
| `--json` | JSON output |

```bash
ax policy rules
ax policy rules --json
```

### `ax policy skills [path]`

List indexed skills.

| Flag | Description |
|---|---|
| `--json` | JSON output |

```bash
ax policy skills
ax policy skill startup
```

### `ax policy skill <name> [path]`

Print one skill body (markdown). `~/.ax/global.db` wins over the project `ax.db` on the same name.

| Argument | Description |
|---|---|
| `name` | Skill name |

```bash
ax policy skill release
ax policy skill azdo-pr-review
```

### `ax policy guard <file>`

Pre-write CRITICAL guard check. Built-in: UTF-8/BOM encoding, secrets paths. Generic: any CRITICAL rule can add a `guard: forbid-path: "<glob>"`, `guard: forbid-content: "<substring or /regex/>"`, `guard: require-content: "<substring or /regex/>"`, or `guard: require-skill: "<name>"` directive line to its body to opt into this check without code changes. Both content directives respect the rule's `globs`: `forbid-content` applies project-wide only when the rule declares no globs — no separate flag needed, directives are picked up automatically from indexed policy.

| Argument / flag | Type | Description |
|---|---|---|
| `file` | required | Path relative to project root |
| `-p`, `--path` | string | Project root (default: cwd) |
| `--delete` | flag | Run delete guard instead of write (default: write) |
| `--json` | flag | JSON output |

```bash
ax policy guard README.md
ax policy guard .env --json
ax policy guard old-file.txt --delete
```

### `ax policy test [path]`

Smoke tests: match, guard, bootstrap, subagents.

| Flag | Description |
|---|---|
| `--json` | JSON output |

```bash
ax policy test
ax policy test --json
```

### `ax policy sync [path]`

Verify managed policy instruction files (`.ax/policy/skills/startup/SKILL.md`, …) and IDE bootstrap files (`.cursor/rules/ax.mdc`, `.continue/rules/ax.md`, `.continue/mcpServers/ax.json`, `AGENTS.md`, etc.). Required managed files must match the embedded init templates; optional team copies only fail the basic preflight checks.

| Flag | Description |
|---|---|
| `--fix` | Restore missing or drifted managed files from embedded init templates |

```bash
ax policy sync
ax policy sync --fix
```

After `--fix` restores `.ax/policy/` files in database mode, run `ax policy import` so `ax.db` picks up the updated skill bodies.
### `ax policy capture <prompt> [path]`

Propose or save a team rule from directive language (`always`, `you must`, `@rule`, …).

| Argument / flag | Type | Description |
|---|---|---|
| `prompt` | required | User prompt containing a directive |
| `--file` | string (repeat) | Open files (for glob inference) |
| `--yes` | flag | Save with defaults (skip interview) |
| `--json` | flag | JSON proposal or result |

```bash
ax policy capture "always update docs when adding a feature"
ax policy capture "never commit secrets" --file .env --json
```

### `ax policy storage`

Show or set policy storage mode (`files` vs `database`). Supports a **project default** plus **per-item overrides**, and lists configured `policy.roots` mounts.

#### `ax policy storage status [path]`

Show effective storage mode, config paths, project/global values, and `policy.roots`.

| Flag | Description |
|---|---|
| `--json` | JSON output |

```bash
ax policy storage status
ax policy storage status --json
```

#### `ax policy storage database [path]`

Set **database** as the project default (`ax.db`). Without `--yes`, prints a scan/plan only.

| Flag | Description |
|---|---|
| `--global` | Write to `~/.ax/config.json` instead of project `ax.json` |
| `--migrate` | Same as running without `--yes`: print the scan plan |
| `--yes` | Import all candidates into `ax.db` and **delete** `.agents/` / `.ax/policy/` markdown |
| `--keep-files` | With `--yes`, import but leave markdown on disk |
| `--json` | JSON output |

```bash
ax policy storage database                # preview scan (no writes)
ax policy storage database --yes          # exclusive: DB is source of truth, files removed
ax policy storage database --yes --keep-files
```

#### `ax policy storage files [path]`

Set **files** as the project default source of truth (`.agents/` on disk).

| Flag | Description |
|---|---|
| `--global` | Write to `~/.ax/config.json` |
| `--migrate` | Preview export |
| `--yes` | Export every rule/skill from `ax.db` into `.agents/` |
| `--json` | JSON output |

```bash
ax policy storage files                   # preview
ax policy storage files --yes             # exclusive: files are source of truth
ax policy storage files --global
```

#### `ax policy storage set-item <id> <files|database> [path]`

Set a **per-item** storage override for one rule id or skill name (hybrid mode).

| Flag | Description |
|---|---|
| `--keep-file` | When switching to `database`, keep the markdown/stub file on disk |
| `--json` | JSON output |

```bash
ax policy storage set-item utf8-no-bom database
ax policy storage set-item startup files
ax policy storage set-item mobile-first database --keep-file --json
```

### `ax policy enable <id> [path]`

Enable a rule or skill so matcher/preflight include it again.

```bash
ax policy enable mobile-first
```

### `ax policy disable <id> [path]`

Disable a rule or skill without deleting it (frontmatter `enabled: false` + DB column).

```bash
ax policy disable mobile-first
```

### `ax policy agents-dir [name] [path]`

Show or set the folder that holds on-disk rules and skills. The default suggestion is `.agents`. The value is `policy.agentsDir` in project `ax.json`. When you set a new name and the old folder exists, ax renames it. `ax init` asks for this name when it is not saved yet.

```bash
ax policy agents-dir
ax policy agents-dir .agents
ax policy agents-dir team-policy
```

### `ax policy stack`

Opt-in language, framework, and CMS policy. Core rules and skills still come from `ax init`. Stacks install only their own files under `.agents/` and record the choice in `ax.json` (`policy.stacks`, `policy.stackDetect`) plus `.ax/stacks.lock.json`.

| Id | Depends on |
|---|---|
| `dotnet` | |
| `java` | |
| `react` | |
| `nextjs` | `react` |
| `angular` | |
| `vue` | |
| `php` | |
| `laravel` | `php` |
| `drupal` | `php` |
| `sitecore` | `dotnet` |
| `optimizely` | `dotnet` |
| `rust` | |
| `python` | |
| `go` | |
| `typescript` | |
| `javascript` | |
| `c` | |
| `cpp` | |
| `ruby` | |
| `swift` | |
| `kotlin` | |
| `dart` | |
| `svelte` | |
| `astro` | |
| `scala` | |
| `lua` | |
| `luau` | |
| `objc` | |
| `r` | |
| `pascal` | |

```bash
ax policy stack list
ax policy stack detect
ax policy stack apply dotnet react
ax policy stack status
ax policy stack upgrade
ax policy stack remove react
```

| Subcommand | Description |
|---|---|
| `list` | List the stack ids shipped with ax (`--json`) |
| `detect [path]` | Print the stacks detected in the project without writing anything (`--json`) |
| `apply [ids…] [path]` | Install stacks into project policy and record them in `ax.json` (`--force`, `--yes`, `--json`) |
| `remove <id> [path]` | Remove one stack's managed files when they still match the lock (`--json`) |
| `status [path]` | Compare installed stacks with the embedded catalog (`--json`) |
| `upgrade [path]` | Refresh files that still match the lock from the current catalog (`--json`) |

`apply` with no ids uses `policy.stacks`. When that list is empty it prints a detection proposal and, on a terminal, asks before writing. Pass `--yes` to apply the proposal without a prompt. `--force` overwrites files you edited. Files you changed are otherwise left in place.

`ax init` asks for stacks every time stdin is a terminal, including when the project was already initialized. Enter keeps the current set (or the detection on a first choice). `none` installs core policy only.

### `ax policy pack`

Per-project shared pack under `.ax/policy/shared/` for git team sync. Default export includes all packable (project + workspace) enabled items; company/private scopes and tags `local` / `noshare` are skipped.

#### `ax policy pack export [path]`

| Flag | Default | Description |
|---|---|---|
| `--tag` | `shared` | Default `shared` = all packable scopes; any other value filters by that frontmatter tag |
| `--out` | `.ax/policy/shared` | Pack directory |
| `--quiet` | | Suppress summary |

```bash
ax policy pack export
ax policy pack export --tag team
```

#### `ax policy pack import [path]`

Imports the shared pack into the local policy store, then refreshes **all** IDE bootstraps (Cursor, Continue, Claude, …) and MCP config for detected agents so a teammate on a different IDE picks up the same rules via `ax_preflight`.

| Flag | Description |
|---|---|
| `--pack` | Pack directory (default `.ax/policy/shared`) |
| `--force` | Overwrite conflicting local items without staging pending |
| `--quiet` | Suppress summary |

```bash
ax policy pack import
ax policy pack import --force
```

#### `ax policy pack status [path]`

```bash
ax policy pack status --json
```

#### `ax policy pack install [name] [path]`

Install a built-in pack into **project** scope (`.ax/policy/`), then import/re-index so MCP and Command Center see the new items. In **database** storage mode the install force-imports files into `ax.db` (not counts-only). Company/private scopes are not modified.

| Flag | Description |
|---|---|
| `--list` | List available built-in packs (no install) |
| `--force` | Overwrite existing rules/skills with the same id/name (use after upgrading to refresh expanded skill bodies) |
| `--json` | JSON output |

Built-in pack `azdo-fullstack` ships full Azure DevOps ticket-to-release **skills** (workflows + checklists) and matching **rules**. `azdo-code-review` requires Story scope, sibling-pattern checks, both bounds, and a named test per finding.

```bash
ax policy pack install --list
ax policy pack install azdo-fullstack
ax policy pack install azdo-fullstack --force
```

#### `ax policy pack zip [path]`

Build a portable `.ax-policy.zip` of selected **shareable** rules and skills (project/workspace, enabled). Private and disabled items are rejected.

```bash
ax policy pack zip --out team.ax-policy.zip --name "Team pack" --rules utf8-no-bom,english-only --skills startup
```

### `ax policy restore`

Preview or install a portable zip into `.agents/`. New items default to install. Conflicts default to skip (including when the local file is newer). Pass `--decisions` to skip a new item, overwrite a conflict, or merge selected hunks.

```bash
ax policy restore --preview team.ax-policy.zip
ax policy restore team.ax-policy.zip --decisions decisions.json
```

`decisions.json` maps `"rule:<id>"` / `"skill:<name>"` to `overwrite`, `skip`, `{ "action": "merge", "acceptHunks": [0] }`, or `{ "action": "merge", "hunks": [{ "index": 0, "take": "both" }] }` (`take` is `local`, `package`, `both`, or `none`). Preview JSON includes `newer` and each packed path may include `contentHash` (blake3). Missing decision keys: **new** → install (`overwrite`), **conflict** → skip. Invalid hunk indexes fail the restore. Accepted writes are recorded in the local hash-on-change revision log (Command Center **History**).

### `ax policy review`

Review pending pack imports when `policy.requireReview` is true.

```bash
ax policy review list
ax policy review show mobile-first
ax policy review approve mobile-first
ax policy review reject mobile-first
```

| Subcommand | Description |
|---|---|
| `list [path]` | List pending rules and skills (`--json`) |
| `show <id> [path]` | Show a pending item and its diff against the local copy (`--json`) |
| `approve <id> [path]` | Move a pending item into active policy |
| `reject <id> [path]` | Drop a pending item |

### `ax policy share`

Remote policy share sync from any git host (GitHub, GitLab, Azure DevOps, on-prem) or OneDrive / SharePoint. See [Remote Policy Share](/guides/policy-sharing/).

Config is stored in **`<project>/ax.json`** under the `"share"` key (per project only). Manage in Takumi Preferences or Command Center Settings.

#### `ax policy share config [path]`

Show this project's share configuration (provider, import mode, URLs, `ax.json` path, last sync).

| Flag | Description |
|---|---|
| `--json` | JSON output |

```bash
ax policy share config
ax policy share config --json
ax policy share config ./services/api
```

#### `ax policy share sync [path]`

Pull (and optionally push) rules, skills, and shared memory from the configured remote.

| Flag | Description |
|---|---|
| `--pull` | Pull from remote (default when neither flag is set) |
| `--push` | Push local pack to remote (OneDrive, git, or GitLab `/api/v4` — see [Remote Policy Share](/guides/policy-sharing/)) |
| `--json` | JSON sync status |

```bash
ax policy share sync
ax policy share sync --pull
ax policy share sync --push
ax policy share sync --pull --push
ax policy share sync --json
```

---

## Authentication

### `ax auth microsoft`

Microsoft device-code sign-in for OneDrive / SharePoint policy share. Works out of
the box using a built-in Microsoft public client — no Azure app registration
needed. Set `AX_MS_CLIENT_ID` only to override with your own Azure AD app (e.g.
tenants that block first-party app consent). Tokens stored in `~/.ax/auth/microsoft.json`.

#### `ax auth microsoft login`

Start interactive device code flow — open the verification URL and enter the code shown in the terminal.

```bash
ax auth microsoft login

# optional: use your own Azure AD app instead of the built-in default
export AX_MS_CLIENT_ID="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
ax auth microsoft login
```

#### `ax auth microsoft logout`

Clear stored Microsoft tokens.

```bash
ax auth microsoft logout
```

#### `ax auth microsoft status`

Show whether you are signed in and which account is active.

| Flag | Description |
|---|---|
| `--json` | JSON output |

```bash
ax auth microsoft status
ax auth microsoft status --json
```

---

## Hidden / internal commands

Not for daily use — invoked by agents, installers, or upgrade helpers.

| Command | Purpose |
|---|---|
| `ax serve --mcp` | Stdio MCP server (agent-launched) |
| `ax serve --mcp --daemon` | Background MCP daemon |
| `ax prompt-hook` | Claude `UserPromptSubmit` hook (stdin JSON) |
| `ax read-guard --ide <ide>` | Pre-tool hook that redirects whole-file reads and symbol searches to the graph (stdin JSON) |
| `ax turn-hook start\|end` | Cursor `beforeSubmitPrompt` / `stop` and Claude `UserPromptSubmit` hook that writes [per-turn memories](/guides/memory/#per-turn-memories) (stdin JSON, prints nothing) |
| `ax watchdog-child` | MCP liveness watchdog child |
| `ax upgrade-apply` | Windows upgrade swap helper |

---

## Quick reference

```bash
# Lifecycle
ax init [path]
ax sync --watch
ax status --json

# Graph
ax explore "auth flow" --json
ax query UserService --kind class
ax callers handleRequest
ax path handleRequest saveUser
ax cycles

# Architecture insights
ax insights --json
ax validate --ci
ax domain
ax report --out AX_REPORT.md
ax export graph-html --out graph.html

# Memory
ax remember "we picked X because Y" --kind decision
ax recall "why X"
ax capture-git

# Git / ship
ax diff --base main --json
ax test-impact --base main
ax ship --watch --open

# Policy
ax policy storage status
ax policy match "deploy" --file src/app.ts
ax policy capture "always validate input"
ax policy share sync
ax auth microsoft status

# Upgrade
ax upgrade
ax upgrade --local
```
