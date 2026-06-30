//! Detailed help text and clap color styles.

use clap::builder::styling::{AnsiColor, Effects, Styles};

pub fn styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Green.on_default().effects(Effects::BOLD))
        .usage(AnsiColor::Yellow.on_default().effects(Effects::BOLD))
        .literal(AnsiColor::Cyan.on_default())
        .placeholder(AnsiColor::Cyan.on_default())
        .valid(AnsiColor::Green.on_default())
        .invalid(AnsiColor::Red.on_default().effects(Effects::BOLD))
        .error(AnsiColor::Red.on_default().effects(Effects::BOLD))
}

pub const ROOT_LONG: &str = "Local-first code intelligence for AI coding agents.

Builds a queryable knowledge graph (.ax/) from your codebase: symbols, call edges,
imports, and framework routes. Agents query via MCP tools or the CLI.

Run `ax` with no arguments to install into detected agents (Cursor, Claude Code, etc.).";

pub const ROOT_AFTER: &str = "Examples:
  ax init                  Initialize and index the current project
  ax explore \"auth flow\"   Natural-language explore (same as ax_explore MCP)
  ax sync --watch          Keep the index fresh while you work
  ax callers greet         Who calls this symbol?

Environment:
  AX_FORCE_COLOR=1   Force colors (overrides NO_COLOR from Cursor/CI shells)
  AX_UNICODE=1       Force Unicode glyphs (spinners, checkmarks) on Windows
  AX_ASCII=1         Force ASCII glyphs everywhere
  NO_COLOR           Disable ANSI colors (also respected by owo-colors)

Docs: https://garywenneker.github.io/ax/";

pub const INSTALL_LONG: &str = "Interactive installer for AI agent MCP configs.

Detects installed agents (Cursor, Claude Code, Codex, opencode, Gemini, Kiro, etc.)
and writes ax MCP server entries pointing at the `ax` binary on PATH.

Does not index a project — run `ax init` inside a repo after install.

Examples:
  ax install               Run the interactive installer
  ax install --yes         Non-interactive (detected agents)
  ax install --yes --all   Configure every supported agent
  ax                       Same as `ax install` (default when no subcommand)";

pub const UNINSTALL_LONG: &str = "Remove ax entries from agent MCP configuration files.

Does not delete ~/.ax or project .ax/ indexes. Use `ax uninit` per project.

Examples:
  ax uninstall             Remove ax from all detected agent configs";

pub const INIT_LONG: &str = "Initialize ax in a project directory and build the full index.

Creates .ax/ (ax.db, ax.json, lock file), runs a full index, installs git hooks,
then offers the interactive agent installer.

Refuses home directory / filesystem roots unless you pass --force on index.

Examples:
  ax init                  Init + index current directory
  ax init ./services/api   Init a subdirectory
  ax init --force          Allow indexing a broad path (use carefully)";

pub const UNINIT_LONG: &str = "Remove ax from a project by deleting the .ax/ directory.

Permanently deletes the local index (ax.db). Does not touch agent MCP configs.

Examples:
  ax uninit                Remove .ax/ from current project
  ax uninit ./legacy-app   Remove from a specific path";

pub const INDEX_LONG: &str = "Rebuild the index from scratch (full scan + extract + resolve).

Use when the watcher is off, after large git operations, or when debugging extraction.
Normal workflows use `ax init` once then `ax sync` or `ax sync --watch`.

Shows a colored progress bar with spinner unless --quiet.

Options:
  --force    Clear the database before indexing
  --quiet    No progress bar or summary line
  --verbose  Reserved for extra diagnostics

Examples:
  ax index                 Re-index current project
  ax index --force         Wipe DB and rebuild
  ax index --quiet         CI-friendly silent run";

pub const SYNC_LONG: &str = "Incremental index update — only changed files since last run.

Compares file size and modification time against the index, re-parses dirty files,
removes stale entries, then resolves cross-references when anything changed.

Shows the same colored progress bar as `ax index` unless --quiet.

Examples:
  ax sync                  Update index for dirty files
  ax sync --watch          Watch filesystem until Ctrl+C
  ax sync --quiet          No progress bar or summary line";

pub const WATCH_LONG: &str = "Watch for file changes and auto-sync (alias for `ax sync --watch`).

Debounced incremental indexing. Leave running in a terminal while agents work.

Examples:
  ax watch                 Watch current project
  ax watch ./monorepo/pkg  Watch a specific root";

pub const STATUS_LONG: &str = "Show index statistics: node/edge/file counts, unresolved refs, last indexed time.

Examples:
  ax status                Human-readable summary
  ax status --json         Machine-readable JSON";

pub const QUERY_LONG: &str = "Full-text search over indexed symbols (FTS5).

Returns matching nodes by name and kind. For natural-language questions use `ax explore`.

Options:
  --kind <kind>    Filter by node kind (function, class, file, ...)
  --limit <n>      Max results (default from query layer)
  --json           JSON array output

Examples:
  ax query auth              Search symbols matching \"auth\"
  ax query User --kind class --limit 20
  ax query handler --json";

pub const EXPLORE_LONG: &str = "Explore an area: summary, blast radius, callers/callees, and numbered source.

Same output shape as the ax_explore MCP tool. Optional BYO LLM offload via `ax offload`.

Examples:
  ax explore \"how does login work\"
  ax explore greet --json    Structured JSON for scripts";

pub const NODE_LONG: &str = "One symbol's details, or a file with line numbers and dependents.

Mirrors the ax_node MCP tool.

Examples:
  ax node greet              Lookup symbol by name
  ax node path/to/file.ts    File-centric view when name omitted";

pub const FILES_LONG: &str = "List indexed files and detected languages.

Examples:
  ax files                   Plain list
  ax files --json            JSON with paths and languages
  ax files --format tree     Tree-style listing when supported";

pub const CONTEXT_LONG: &str = "Build task-oriented markdown context for a coding task.

Assembles relevant symbols, subgraph, and file markers for agent prompts.

Examples:
  ax context \"add rate limiting to the API\"";

pub const CALLERS_LONG: &str = "Find symbols that call the given function/method/class.

Traversal follows call edges outward (who invokes this?).

Examples:
  ax callers authenticate
  ax callers main::handler";

pub const CALLEES_LONG: &str = "Find symbols called by the given function/method.

Traversal follows call edges inward (what does this invoke?).

Examples:
  ax callees handle_request
  ax callees App::run";

pub const IMPACT_LONG: &str = "Blast-radius subgraph for a symbol — what breaks if you change it?

Returns nodes and edges within the impact radius (same semantics as ax_impact MCP).

Examples:
  ax impact UserService
  ax impact validate_token";

pub const AFFECTED_LONG: &str = "Reverse impact: test files affected by changes to given source paths.

Use before/after editing to find tests that exercise changed code.

Examples:
  ax affected src/auth/login.ts
  ax affected packages/api/src/handler.rs src/db/user.rs";

pub const UNLOCK_LONG: &str = "Force-remove a stale .ax/ax.lock left by a crashed process.

Safe when no other ax process is indexing the same project.

Examples:
  ax unlock
  ax unlock ./services/worker";

pub const DAEMON_LONG: &str = "MCP background daemon control (TCP / named pipe per project).

The daemon shares one index connection for multiple MCP clients.

Subcommands:
  status   Show pid, port, socket path from .ax/daemon.json
  stop     Stop the running daemon for this project root

Examples:
  ax daemon status
  ax daemon stop
  ax daemon ./repo status";

pub const VERSION_LONG: &str = "Print the installed ax version.

Also available as `ax -V` / `ax --version`.";

pub const UPGRADE_LONG: &str = "Self-update from GitHub Releases (platform zip/tar.gz) or cargo install fallback.

Uses AX_GITHUB_REPO (default GaryWenneker/ax). Matches bundle name to OS/arch.

Examples:
  ax upgrade               Latest release
  ax upgrade v0.2.0          Specific tag
  ax upgrade --check       Check for updates without installing

Background checks: after most commands, ax may print an update notice (cached ~24h).
Private GitHub repos need GITHUB_TOKEN, GH_TOKEN, or `gh auth login`.
Disable with AX_NO_UPDATE_CHECK=1.";

pub const TELEMETRY_LONG: &str = "Anonymous usage telemetry (opt-in/out).

Records command names and coarse buckets — never source code or paths.
Endpoint: getax.wenneker.io/v1/events (see docs/TELEMETRY.md).

Examples:
  ax telemetry status
  ax telemetry on
  ax telemetry off

Also: DO_NOT_TRACK=1 or AX_TELEMETRY=0 disables sending.";

pub const WEB_LONG: &str = "Start a local web server and open the ax graph viewer in the browser.

Serves a React dashboard at http://localhost:<port> that lets you browse nodes,
files, and edges of the current project's index without the CLI.

Options:
  --port <port>   Port to listen on (default 7070)
  --open          Open the browser automatically after starting

Examples:
  ax web                   Start the viewer on port 7070
  ax web --port 8080       Use a different port
  ax web --open            Start and immediately open in browser
  ax web ./my-project      Browse a specific project root";

pub const OFFLOAD_LONG: &str = "Configure optional LLM offload for `ax explore` (BYO OpenAI-compatible API).

Stored in ~/.ax/config.json or via AX_OFFLOAD_URL / AX_OFFLOAD_KEY env vars.

Subcommands:
  status          Show current endpoint and key env name
  set-endpoint    Save base URL (must end with /v1)
  clear           Remove offload config

Examples:
  ax offload status
  ax offload set-endpoint https://api.openai.com/v1 --key-env OPENAI_API_KEY
  ax offload clear";
