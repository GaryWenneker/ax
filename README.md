# ax — local-first code intelligence for AI agents

**ax** parses your codebase with [tree-sitter](https://tree-sitter.github.io/), stores symbols and relationships in a local SQLite graph (`.ax/`), and exposes them through a **CLI** and **MCP tools** so coding agents answer structural questions without scanning files.

- **100% local** — no source code leaves your machine
- **Deterministic** — graph data comes from AST extraction, not LLM summaries
- **Agent-native** — MCP integration for Cursor, Claude Code, Codex, opencode, Gemini CLI, Kiro, and more
- **Native Rust** — single binary, no Node.js runtime required

Docs: [getax.wenneker.io](https://getax.wenneker.io)

---

## Quick start

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/GaryWenneker/ax/main/install.sh | sh

# Windows (PowerShell)
irm https://raw.githubusercontent.com/GaryWenneker/ax/main/install.ps1 | iex

# Or build from source
cargo install --path crates/ax-cli
```

```bash
# Wire MCP into detected agents
ax install

# Index a project (creates .ax/ and builds the graph)
cd your-project
ax init

# Keep the index fresh while you work
ax sync --watch
```

---

## What ax builds

Each indexed project gets a `.ax/` directory:

| File | Purpose |
|------|---------|
| `ax.db` | SQLite database (WAL mode) — nodes, edges, files, FTS5 search |
| `ax.json` | Project config (extensions, ignore rules) |
| `ax.lock` | Cross-process lock during indexing |
| `daemon.json` | MCP daemon metadata (when running) |

### Graph contents

- **Nodes** — functions, classes, methods, types, routes, components, files, …
- **Edges** — calls, imports, extends, implements, framework-specific links
- **Files** — structure + full-text symbol search (FTS5)

---

## How it works

```
source files
    │
    ▼
Extraction (tree-sitter, parallel via rayon)
    │  AST → nodes + edges + unresolved refs
    ▼
Storage (SQLite .ax/ax.db, schema v6, FTS5)
    │
    ▼
Resolution (imports, name-matching, framework synthesizers)
    │
    ▼
Graph queries (callers, callees, impact, explore, context)
    │
    ▼
CLI / MCP / markdown context for agents
```

### 1. Extraction

Native [tree-sitter](https://tree-sitter.github.io/) grammars parse source into ASTs. Language-specific extractors emit **nodes** and **edges**. Parsing runs in parallel (`AX_PARSE_WORKERS`, default: CPU count).

Supported languages include Rust, TypeScript/JavaScript, Python, Go, Java, and 30+ file extensions via framework extractors (React, Angular, Django, Spring, etc.).

### 2. Storage

Everything lands in `.ax/ax.db`:

- `nodes` + `edges` tables for the call/import graph
- `nodes_fts` virtual table (FTS5) for symbol search
- WAL mode for concurrent MCP reads during sync

### 3. Resolution

After extraction, unresolved references are matched to definitions:

- Import paths → source files
- Function calls → symbol definitions
- Class inheritance and framework patterns (routes, DI, JSX, …)

Some dynamic boundaries (callbacks, observers) are bridged by **synthesizers** so call flows connect end-to-end.

### 4. Auto-sync

`ax sync` incrementally re-indexes changed files. `ax sync --watch` (or `ax watch`) debounces filesystem events via `notify` and keeps the graph current. Git hooks (`post-commit`, `post-merge`, `post-checkout`) can trigger sync automatically after `ax init`.

---

## CLI commands

The CLI uses **colored output**, **progress bars** (index/init), and **spinners** (explore, sync, query, …). Disable with `--quiet` or `NO_COLOR=1`.

| Command | Description |
|---------|-------------|
| `ax` / `ax install` | Interactive MCP installer for detected agents |
| `ax uninstall` | Remove ax from agent configs |
| `ax init [path]` | Create `.ax/`, full index, git hooks, offer installer |
| `ax uninit [path]` | Delete `.ax/` directory |
| `ax index [--force] [--quiet]` | Full re-index with progress bar |
| `ax sync [--watch] [--quiet]` | Incremental sync; `--watch` keeps running |
| `ax watch [path]` | Alias for `ax sync --watch` |
| `ax status [--json]` | Node/edge/file counts, last indexed time |
| `ax query <text> [--json]` | FTS symbol search |
| `ax explore <query> [--json]` | Natural-language explore (same as MCP) |
| `ax node [name]` | Symbol or file details |
| `ax files [--json]` | List indexed files |
| `ax context <task>` | Build markdown task context |
| `ax callers <symbol>` | Who calls this symbol? |
| `ax callees <symbol>` | What does this symbol call? |
| `ax impact <symbol>` | Blast-radius subgraph |
| `ax affected <files…>` | Tests affected by file changes |
| `ax unlock [path]` | Remove stale `ax.lock` |
| `ax daemon [status\|stop]` | MCP daemon control |
| `ax upgrade [tag]` | Self-update from GitHub releases |
| `ax telemetry [on\|off\|status]` | Anonymous usage telemetry |
| `ax offload …` | Optional BYO LLM for explore synthesis |

Run `ax help <command>` for detailed help with examples.

### Terminal UX

| Feature | When | Flag to disable |
|---------|------|-----------------|
| Colored help (clap) | `ax help` | `NO_COLOR=1` |
| Progress bar + spinner | `ax index`, `ax init` | `--quiet` |
| Spinner | `ax sync`, `ax explore`, `ax query`, graph commands | `--quiet` or `--json` |
| Styled status lines | `ax status`, success/error messages | `--json` |

Environment:

| Variable | Effect |
|----------|--------|
| `NO_COLOR` | Disable all ANSI colors |
| `AX_FORCE_COLOR=1` | Force colors (overrides `NO_COLOR` — needed in Cursor/CI shells) |
| `AX_UNICODE=1` | Force Unicode glyphs (✓, spinner frames) on Windows |
| `AX_ASCII=1` | Force ASCII glyphs everywhere |
| `AX_PARSE_WORKERS` | Parallel parse thread count |
| `AX_QUERY_POOL_SIZE` | MCP query pool size |
| `AX_GITHUB_REPO` | Override repo for `ax upgrade` (default `GaryWenneker/ax`) |

---

## MCP server

ax exposes a [Model Context Protocol](https://modelcontextprotocol.io/) server. Agents call tools instead of grepping the tree.

### Tools

| Tool | Purpose |
|------|---------|
| `ax_explore` | Semantic search + graph traversal + numbered source |
| `ax_node` | Single symbol or file details |
| `ax_search` | FTS symbol lookup |
| `ax_status` | Index stats and staleness |
| `ax_index` | Trigger incremental sync |
| `ax_files` | Project file listing |
| `ax_context` | Task-oriented markdown context |
| `ax_callers` | Incoming call edges |
| `ax_callees` | Outgoing call edges |
| `ax_impact` | Blast-radius subgraph |
| `ax_affected` | Reverse impact → affected tests |

**Agent rule:** for structural questions (how does X work, call paths, impact), call `ax_explore` first. Treat returned numbered source as already read.

### Transport

- **stdio** — default when launched by an agent (`ax serve --mcp`)
- **Daemon** — shared per-project daemon (TCP / named pipe / Unix socket) for multiple MCP clients
- Watchdogs: PPID + liveness child processes; set `AX_NO_WATCHDOG=1` to disable

Per-project indexes: pass `projectPath` when the workspace root differs from cwd (monorepos).

---

## Architecture (Rust workspace)

| Crate | Role |
|-------|------|
| `ax-cli` | CLI entry point, terminal UX (colors, progress, spinners) |
| `ax-core` | `Ax` facade — open, index, explore, graph queries |
| `ax-extraction` | tree-sitter parsing, orchestrator, parallel parse pool |
| `ax-resolution` | Reference resolution + framework synthesizers |
| `ax-db` | SQLite schema, migrations, FTS5 |
| `ax-graph` | BFS/DFS traversal, petgraph cycle detection |
| `ax-context` | Explore formatting, task context builder |
| `ax-sync` | File watcher, git hooks, incremental sync |
| `ax-mcp` | MCP server, daemon, query pool, tool handlers |
| `ax-telemetry` | Opt-in anonymous usage events |
| `ax-reasoning` | Optional BYO LLM offload for explore |
| `ax-types` | Shared types (`Node`, `Edge`, `ExploreResult`, …) |
| `ax-utils` | Errors, paths, config helpers |

Build:

```bash
cargo build --release -p ax-cli
./target/release/ax --version
```

Run MCP (hidden command):

```bash
ax serve --mcp          # stdio transport
ax serve --mcp --daemon # background daemon
```

---

## Explore offload (optional)

`ax explore` returns deterministic graph output. Optionally synthesize a narrative via your own OpenAI-compatible API:

```bash
ax offload set-endpoint https://api.openai.com/v1 --key-env OPENAI_API_KEY
ax offload status
ax offload clear
```

Or set `AX_OFFLOAD_URL` and `AX_OFFLOAD_KEY` environment variables.

---

## Telemetry

Anonymous, opt-in usage metrics (command names, coarse buckets — never source code or paths).

```bash
ax telemetry status
ax telemetry on
ax telemetry off
```

Also disabled by `DO_NOT_TRACK=1` or `AX_TELEMETRY=0`. See [docs/TELEMETRY.md](docs/TELEMETRY.md).

---

## Development

```bash
# Run tests
cargo test

# Smoke test on hello-world fixture
cargo test -p ax-smoke-tests

# Release packaging (maintainer)
cargo build --release -p ax-cli --target x86_64-pc-windows-msvc
bash scripts/package-release.sh win32-x64 x86_64-pc-windows-msvc
```

See [docs/PRODUCTION.md](docs/PRODUCTION.md) for GitHub Releases, Netlify docs site, and telemetry worker setup.

---

## License

See repository license file. ax is an independent Rust code-intelligence tool — MCP-first, native tree-sitter, local SQLite graph.
