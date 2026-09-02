//! ax CLI entry point.

const _AX_SHIP_MARKER: &str = "ax-ship-marker-v1";

mod commands;
mod help_text;
mod installer;
mod ui;
mod version_check;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use ax_policy::PolicyStorage;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "ax",
    version,
    about = "ax code intelligence tool",
    long_about = help_text::ROOT_LONG,
    after_help = help_text::ROOT_AFTER,
    styles = help_text::styles(),
    color = clap::ColorChoice::Auto,
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Interactive installer
    #[command(long_about = help_text::INSTALL_LONG)]
    Install {
        #[arg(long, action = clap::ArgAction::SetTrue, help = "Non-interactive: skip prompts, install detected agents")]
        yes: bool,
        #[arg(long, action = clap::ArgAction::SetTrue, help = "Install all agent targets, not only detected ones")]
        all: bool,
        #[arg(
            long = "target",
            value_name = "ID",
            num_args = 1,
            action = clap::ArgAction::Append,
            help = "Wire a specific agent target (repeatable), e.g. takumi, vscode, cursor"
        )]
        target: Vec<String>,
        #[arg(
            long,
            value_name = "DIR",
            help = "Project root for workspace MCP files (default: current directory). Use from Takumi with an explicit folder."
        )]
        path: Option<String>,
    },
    /// Remove ax from agent configs
    #[command(long_about = help_text::UNINSTALL_LONG)]
    Uninstall,
    /// Initialize project and index
    #[command(long_about = help_text::INIT_LONG)]
    Init {
        path: Option<String>,
        #[arg(long, help = "Discover monorepo members and write ax.json members[]")]
        workspace: bool,
    },
    /// Remove .ax directory
    #[command(long_about = help_text::UNINIT_LONG)]
    Uninit { path: Option<String> },
    /// Full re-index
    #[command(long_about = help_text::INDEX_LONG)]
    Index {
        path: Option<String>,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        quiet: bool,
        #[arg(long)]
        verbose: bool,
        #[arg(long = "all", help = "Index every workspace member from ax.json")]
        all_members: bool,
    },
    /// Incremental sync
    #[command(long_about = help_text::SYNC_LONG)]
    Sync {
        path: Option<String>,
        #[arg(long)]
        quiet: bool,
        #[arg(long, help = "Watch for changes and auto-sync (debounced)")]
        watch: bool,
        #[arg(long = "all", help = "Sync every workspace member from ax.json")]
        all_members: bool,
    },
    /// Watch for file changes and auto-sync (alias for sync --watch)
    #[command(long_about = help_text::WATCH_LONG)]
    Watch {
        path: Option<String>,
        #[arg(long)]
        quiet: bool,
    },
    /// Index statistics
    #[command(long_about = help_text::STATUS_LONG)]
    Status {
        path: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// FTS symbol search
    #[command(long_about = help_text::QUERY_LONG)]
    Query {
        text: String,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long)]
        limit: Option<u32>,
        #[arg(long)]
        json: bool,
    },
    /// Explore (same as ax_explore MCP tool)
    #[command(long_about = help_text::EXPLORE_LONG)]
    Explore { query: Vec<String>, #[arg(long)] json: bool },
    /// Store a durable project memory (decision, fix, convention)
    Remember {
        /// The memory content — what to remember and why
        text: String,
        #[arg(long, help = "Short title (defaults to first line)")]
        title: Option<String>,
        #[arg(long, help = "decision | bug_fix | architecture | convention | note")]
        kind: Option<String>,
        #[arg(long = "tag", help = "Tag (repeatable)")]
        tags: Vec<String>,
        #[arg(long = "file", help = "Related file path (repeatable)")]
        files: Vec<String>,
        #[arg(long)]
        json: bool,
    },
    /// Search project memories by free text
    Recall {
        query: String,
        #[arg(long)]
        limit: Option<u32>,
        #[arg(long)]
        json: bool,
    },
    /// Mine recent git commits into memories (the "why" behind changes)
    CaptureGit {
        #[arg(long, help = "Number of commits to scan (default 100)")]
        limit: Option<u32>,
        #[arg(long, help = "No output (for git hooks)")]
        quiet: bool,
        #[arg(long)]
        json: bool,
    },
    /// Shared memory vault export/import for team git sync
    Memory {
        #[command(subcommand)]
        action: MemoryCommands,
    },
    /// Node details (same as ax_node MCP tool)
    #[command(long_about = help_text::NODE_LONG)]
    Node { name: Option<String> },
    /// List project files
    #[command(long_about = help_text::FILES_LONG)]
    Files {
        #[arg(long)]
        format: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Build task context
    #[command(long_about = help_text::CONTEXT_LONG)]
    Context { task: String },
    /// Find callers
    #[command(long_about = help_text::CALLERS_LONG)]
    Callers { symbol: String },
    /// Find callees
    #[command(long_about = help_text::CALLEES_LONG)]
    Callees { symbol: String },
    /// Impact radius
    #[command(long_about = help_text::IMPACT_LONG)]
    Impact { symbol: String },
    /// List call-graph cycles (non-trivial SCCs)
    Cycles {
        #[arg(long, default_value_t = 50, help = "Max cycles to report")]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Shortest Calls/References path between two symbols
    Path {
        from: String,
        to: String,
        #[arg(long)]
        json: bool,
    },
    /// Public API surface for a module / path prefix
    Api {
        module: String,
        #[arg(long, default_value_t = 200, help = "Max exported symbols")]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Graph insights: communities, god nodes, surprising connections
    Insights {
        path: Option<String>,
        #[arg(long, default_value_t = 1.0, help = "Cluster granularity (higher = more communities)")]
        resolution: f64,
        #[arg(long, default_value_t = 20, help = "Max god nodes to show")]
        god_limit: usize,
        #[arg(long, default_value_t = 20, help = "Max surprising connections to show")]
        surprising_limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Architecture report (AX_REPORT.md) from graph insights
    Report {
        path: Option<String>,
        #[arg(long, help = "Output file (default: AX_REPORT.md at project root)")]
        out: Option<String>,
        #[arg(long, default_value_t = 1.0, help = "Cluster granularity (higher = more communities)")]
        resolution: f64,
        #[arg(long, help = "Print the report to stdout instead of writing a file")]
        stdout: bool,
    },
    /// Export the graph to a portable format
    Export {
        #[command(subcommand)]
        action: ExportCommands,
    },
    /// Graph hygiene: isolated symbols, dangling edges, orphan docs
    Validate {
        path: Option<String>,
        #[arg(long, help = "Exit non-zero when dangling edges or isolated symbols exist")]
        ci: bool,
        #[arg(long)]
        json: bool,
    },
    /// Git diff symbol-level blast radius vs base branch
    Diff {
        #[arg(long, default_value = "main")]
        base: String,
        #[arg(long)]
        json: bool,
    },
    /// Test impact analysis (git diff + TIA)
    TestImpact {
        #[arg(long, default_value = "main")]
        base: String,
        #[arg(long)]
        json: bool,
    },
    /// Git Command Center — watch, evaluate, draft PR
    Ship {
        path: Option<String>,
        #[arg(long, help = "Watch git events and open dashboard")]
        watch: bool,
        #[arg(long, help = "Run one quality gate evaluation")]
        evaluate: bool,
        #[arg(long, help = "Headless CI mode: evaluate, print JSON, exit 1 if gate failed")]
        ci: bool,
        #[arg(long, help = "Create draft PR after quality gate")]
        draft: bool,
        #[arg(long, help = "PR title")]
        title: Option<String>,
        #[arg(long, default_value = "7070")]
        port: u16,
        #[arg(long, help = "Open browser")]
        open: bool,
        #[arg(long, help = "Auto-commit uncommitted changes before the gate runs (this run only; see [auto_commit] in ship.toml to persist)")]
        auto_commit: bool,
        #[arg(long, help = "With --auto-commit: undo the checkpoint commit (git reset --mixed, never --hard) if the gate fails (this run only)")]
        revert_on_fail: bool,
    },
    /// Affected tests
    #[command(long_about = help_text::AFFECTED_LONG)]
    Affected {
        files: Vec<String>,
        #[arg(long, help = "Read changed file paths from stdin")]
        stdin: bool,
        #[arg(long, default_value = "main", help = "Base branch for git diff when no files given")]
        base: String,
        #[arg(short, long, default_value = "5", help = "Max dependency traversal depth")]
        depth: u32,
        #[arg(short, long, help = "Glob filter for test files")]
        filter: Option<String>,
        #[arg(short, long, help = "Output as JSON")]
        json: bool,
        #[arg(short, long, help = "Output file paths only")]
        quiet: bool,
    },
    /// Remove stale ax.lock
    #[command(long_about = help_text::UNLOCK_LONG)]
    Unlock { path: Option<String> },
    /// MCP daemon status/stop
    #[command(long_about = help_text::DAEMON_LONG)]
    Daemon {
        path: Option<String>,
        #[command(subcommand)]
        action: Option<DaemonCommands>,
    },
    /// Print version
    #[command(long_about = help_text::VERSION_LONG)]
    Version,
    /// Self-update from getax CDN (GitHub fallback)
    #[command(long_about = help_text::UPGRADE_LONG)]
    Upgrade {
        #[arg(help = "Optional release tag (e.g. v0.1.0)")]
        version: Option<String>,
        #[arg(long, action = clap::ArgAction::SetTrue, help = "Check for updates without installing")]
        check: bool,
        #[arg(long, value_name = "ARCHIVE", num_args = 0..=1, default_missing_value = "", help = "Install from local dist/ archive (dev); optional path to zip/tar.gz")]
        local: Option<String>,
    },
    /// Anonymous usage telemetry (on|off|status)
    #[command(long_about = help_text::TELEMETRY_LONG)]
    Telemetry {
        #[arg(help = "on, off, or status")]
        action: Option<String>,
    },
    /// Estimated context-token savings from MCP graph queries
    #[command(long_about = help_text::SAVINGS_LONG)]
    Savings {
        #[command(subcommand)]
        action: Option<SavingsAction>,
        #[arg(long, value_name = "PERIOD", help = "week | month_to_date | month | year | custom")]
        period: Option<String>,
        #[arg(long, value_name = "YYYY-MM-DD", help = "Start date (required for custom)")]
        from: Option<String>,
        #[arg(long, value_name = "YYYY-MM-DD", help = "End date (optional for custom)")]
        to: Option<String>,
        #[arg(long, help = "JSON output")]
        json: bool,
    },
    /// Daily model price sync (OpenRouter + Artificial Analysis)
    #[command(long_about = help_text::PRICING_LONG)]
    Pricing {
        #[command(subcommand)]
        action: PricingAction,
    },
    /// MCP quality audit (verbose log ↔ Cursor transcript)
    #[command(long_about = help_text::MCP_LONG)]
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },
    /// Explore reasoning offload configuration
    #[command(long_about = help_text::OFFLOAD_LONG)]
    Offload {
        #[command(subcommand)]
        action: Option<OffloadCommands>,
    },
    /// Browse the local ax code graph in a web UI
    #[command(long_about = help_text::WEB_LONG)]
    Web {
        path: Option<String>,
        #[arg(long, default_value = "7070", help = "Port to listen on")]
        port: u16,
        #[arg(long, help = "Open the browser automatically after starting")]
        open: bool,
    },
    /// Native wgpu Command Center (embeds ax-web in-process)
    #[command(long_about = help_text::DESKTOP_LONG)]
    Desktop {
        path: Option<String>,
        #[arg(long, default_value = "7070", help = "Port for the embedded ax-web server")]
        port: u16,
        #[arg(long, default_value = "127.0.0.1", help = "Bind address for the embedded server")]
        bind: String,
    },
    /// Share Command Center on the LAN with a token (read-only)
    Share {
        path: Option<String>,
        #[arg(long, default_value = "7070", help = "Port to listen on")]
        port: u16,
        #[arg(long, default_value = "0.0.0.0", help = "Bind address")]
        bind: String,
        #[arg(long, help = "Open the browser automatically after starting")]
        open: bool,
        #[arg(long, help = "Share token (default: random)")]
        token: Option<String>,
    },
    /// Language Server bridge — enrich unresolved refs with Exact edges
    Lsp {
        #[command(subcommand)]
        action: LspCommands,
    },
    /// Sync AzDO wiki + workspace docs into ax.db (documentation-catalog)
    #[command(name = "docs-catalog", long_about = help_text::DOCS_CATALOG_LONG)]
    DocsCatalog {
        #[command(subcommand)]
        action: DocsCatalogAction,
    },
    /// Policy rules and skills
    Policy {
        #[command(subcommand)]
        action: PolicyCommands,
    },
    /// Authentication for remote share providers
    Auth {
        #[command(subcommand)]
        action: AuthCommands,
    },
    /// Cursor IDE auth profiles (subscription switching)
    #[command(long_about = help_text::CURSOR_LONG)]
    Cursor {
        #[command(subcommand)]
        action: CursorCommands,
    },
    /// Claude UserPromptSubmit hook (hidden; reads {prompt,cwd} JSON on stdin)
    #[command(hide = true, name = "prompt-hook")]
    PromptHook,
    /// Cursor sessionStart hook (hidden; reads hook JSON on stdin)
    #[command(hide = true, name = "session-hook")]
    SessionHook,
    /// Claude Stop/SubagentStop hook (hidden; reads hook JSON on stdin, may block via decision JSON)
    #[command(hide = true, name = "stop-hook")]
    StopHook,
    /// Hidden liveness watchdog child (spawned by ax MCP/daemon)
    #[command(hide = true, name = "watchdog-child")]
    WatchdogChild {
        parent_pid: u32,
        timeout_ms: u64,
    },
    /// Hidden Windows upgrade helper (spawned by ax upgrade; no PowerShell required)
    #[command(hide = true, name = "upgrade-apply")]
    UpgradeApply {
        #[arg(long)]
        parent_pid: u32,
        #[arg(long)]
        staging: std::path::PathBuf,
        #[arg(long)]
        dest: std::path::PathBuf,
    },
    /// Start MCP server (hidden)
    Serve {
        #[arg(long, hide = true)]
        mcp: bool,
        #[arg(long, hide = true)]
        daemon: bool,
        #[arg(long, hide = true)]
        path: Option<String>,
    },
}

#[derive(Subcommand)]
enum DocsCatalogAction {
    /// Pull wiki, scan workspace, import memories, sync graph
    Sync {
        #[arg(long, help = "Skip git pull/clone of AzDO wiki")]
        skip_wiki_pull: bool,
        #[arg(long, help = "Build JSONL only; skip import and graph sync")]
        dry_run: bool,
        #[arg(long, help = "JSON output")]
        json: bool,
    },
}

#[derive(Subcommand)]
enum PricingAction {
    /// Fetch today's prices into ~/.ax/usage.db
    Sync {
        #[arg(long, help = "Re-fetch even if already synced today")]
        force: bool,
    },
    /// Show last sync status per source
    Status {
        #[arg(long, help = "JSON output")]
        json: bool,
    },
    /// List latest synced model rates
    List {
        #[arg(long, value_name = "SOURCE", help = "Filter by source (default: openrouter)")]
        source: Option<String>,
        #[arg(long, help = "JSON output")]
        json: bool,
    },
    /// Daily rate history for a model id/substring
    History {
        model: String,
        #[arg(long, value_name = "SOURCE", help = "Filter by source (default: all, UI uses openrouter)")]
        source: Option<String>,
        #[arg(long, default_value = "30", help = "Max days of history")]
        days: i64,
        #[arg(long, help = "JSON output")]
        json: bool,
    },
}

#[derive(Subcommand)]
enum SavingsAction {
    /// Import tool-call stats from local Cursor / Claude Code session logs
    Import {
        #[arg(long, help = "Import ~/.claude/projects/*.jsonl")]
        claude: bool,
        #[arg(long, help = "Import ~/.cursor/projects/*/agent-transcripts/*.jsonl")]
        cursor: bool,
        #[arg(long, help = "Import both Claude and Cursor logs")]
        all: bool,
    },
    /// Record the model name for an agent session (debug / manual tagging)
    TagSession {
        #[arg(long, default_value = "cursor", help = "Agent source (cursor or claude)")]
        agent: String,
        #[arg(long, help = "Session / conversation id")]
        session_id: String,
        #[arg(long, help = "Model label (e.g. composer-2.5-fast)")]
        model: String,
    },
    /// Install or manage Cursor hooks for savings tracking
    Hook {
        #[command(subcommand)]
        action: SavingsHookAction,
    },
}

#[derive(Subcommand)]
enum McpAction {
    /// Correlate .ax/mcp-verbose.log with a Cursor transcript and score quality
    Audit {
        path: Option<String>,
        #[arg(long, value_name = "UUID|PATH", help = "Cursor session id or transcript .jsonl path")]
        session: Option<String>,
        #[arg(long, value_name = "MIN", help = "Rolling window minutes when no --session (default 30)")]
        window_minutes: Option<u64>,
        #[arg(long, help = "JSON output")]
        json: bool,
    },
}

#[derive(Subcommand)]
enum SavingsHookAction {
    /// Copy sessionStart hook into ~/.cursor/hooks/
    Install,
}

#[derive(Subcommand)]
enum MemoryCommands {
    /// Export memories tagged for team sync (default tag: shared)
    Export {
        #[arg(long, default_value = "shared", help = "Only export memories with this tag")]
        tag: String,
        #[arg(long, help = "Output path (default: .ax/memory/shared.jsonl)")]
        out: Option<String>,
        #[arg(long)]
        quiet: bool,
    },
    /// Import shared memories from JSONL (default: .ax/memory/shared.jsonl)
    Import {
        #[arg(long, help = "Input path (default: .ax/memory/shared.jsonl)")]
        path: Option<String>,
        #[arg(long)]
        quiet: bool,
    },
}

#[derive(Subcommand)]
enum LspCommands {
    /// Show which language servers are available on PATH
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Resolve unresolved refs via LSP definition → Exact edges
    Enrich {
        path: Option<String>,
        #[arg(long, default_value_t = 200, help = "Max unresolved refs to examine")]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum ExportCommands {
    /// Export an Open Knowledge Format (OKF) Markdown bundle
    #[command(long_about = help_text::EXPORT_OKF_LONG)]
    Okf {
        path: Option<String>,
        #[arg(long, help = "Output directory (default: ax.json okf.outDir or .ax/knowledge)")]
        out: Option<String>,
        #[arg(long, default_value_t = 0, help = "Max concepts (0 = all)")]
        limit: usize,
        #[arg(long, help = "Validate the OKF bundle (index + relative links)")]
        check: bool,
        #[arg(long, help = "With --check: fail non-zero on OKF validation issues")]
        ci: bool,
        #[arg(
            long,
            help = "Publish OKF bundle to okf.azdoWiki git remote (Azure DevOps Wiki or any wiki)"
        )]
        publish_wiki: bool,
        #[arg(long, help = "With --publish-wiki: preview only (no clone/commit/push)")]
        dry_run: bool,
        #[arg(long, help = "With --publish-wiki: commit locally but do not push")]
        no_push: bool,
        #[arg(long)]
        json: bool,
    },
    /// Alias for `ax export okf` (Open Knowledge Format)
    Concepts {
        path: Option<String>,
        #[arg(long, help = "Output directory (default: ax.json okf.outDir or .ax/knowledge)")]
        out: Option<String>,
        #[arg(long, default_value_t = 0, help = "Max concepts (0 = all)")]
        limit: usize,
    },
    /// Self-contained interactive HTML graph (portable, no server needed)
    GraphHtml {
        path: Option<String>,
        #[arg(long, help = "Output file (default: graph.html at project root)")]
        out: Option<String>,
        #[arg(long, default_value_t = 1.0, help = "Cluster granularity (higher = more communities)")]
        resolution: f64,
        #[arg(long, default_value_t = 3000, help = "Max nodes to include (top by degree)")]
        limit: usize,
    },
    /// Export the knowledge graph (html|json|dot|graphml|gexf|cypher|mermaid|plantuml)
    Graph {
        path: Option<String>,
        #[arg(long, default_value = "json", help = "html|json|dot|graphml|gexf|cypher|mermaid|plantuml")]
        format: String,
        #[arg(long, help = "Output file (default depends on format)")]
        out: Option<String>,
        #[arg(long, default_value_t = 1.0, help = "Cluster granularity (higher = more communities)")]
        resolution: f64,
        #[arg(long, default_value_t = 3000, help = "Max nodes to include (top by degree)")]
        limit: usize,
    },
}

#[derive(Subcommand)]
enum OffloadCommands {
    /// Show offload configuration
    Status,
    /// Save BYO endpoint URL
    SetEndpoint {
        url: String,
        #[arg(long, help = "Env var name holding the API key")]
        key_env: Option<String>,
    },
    /// Remove offload configuration
    Clear,
}

#[derive(Subcommand)]
enum CursorCommands {
    /// Cursor auth session management
    Auth {
        #[command(subcommand)]
        action: CursorAuthCommands,
    },
}

#[derive(Subcommand)]
enum CursorAuthCommands {
    /// Show live Cursor auth (plan, email, token age)
    Status {
        #[arg(long)]
        json: bool,
    },
    /// List saved auth profiles
    List {
        #[arg(long)]
        json: bool,
    },
    /// Save current Cursor auth as a named profile
    Save {
        name: String,
        #[arg(long, help = "Human-readable label")]
        label: Option<String>,
        #[arg(
            long,
            help = "Save from auth.json only (for bootstrapping a stale personal session)"
        )]
        from_auth_json: bool,
        #[arg(long)]
        email: Option<String>,
        #[arg(long)]
        membership: Option<String>,
        #[arg(long)]
        subscription_status: Option<String>,
        #[arg(long)]
        sign_up_type: Option<String>,
    },
    /// Apply a saved profile to live Cursor data (restart Cursor after)
    Use {
        name: String,
        #[arg(long, help = "Apply even when Cursor is running")]
        force: bool,
        #[arg(long)]
        json: bool,
    },
    /// Show a saved profile without applying
    Show {
        name: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum PolicyCommands {
    /// Index .ax/policy files into SQLite (files mode) or show DB counts (database mode)
    Index {
        path: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// Import .mdc / SKILL.md from disk into database (merge; keeps DB-only rows)
    Import {
        path: Option<String>,
    },
    /// Pull shared policy rules/skills from a git repository URL
    Pull {
        /// Git URL (https or ssh) of a policy registry repo
        url: String,
        path: Option<String>,
        #[arg(long, help = "Vendor subdirectory name under .ax/policy/vendored/")]
        name: Option<String>,
    },
    /// Export database policy to .mdc / SKILL.md files
    Export {
        path: Option<String>,
        #[arg(long, default_value = ".ax/policy/export")]
        out: String,
    },
    /// Match rules/skills for a prompt
    Match {
        prompt: String,
        path: Option<String>,
        #[arg(long)]
        file: Vec<String>,
        #[arg(long)]
        json: bool,
    },
    /// List indexed rules
    Rules {
        path: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// List indexed skills
    Skills {
        path: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Show one skill body
    Skill {
        name: String,
        path: Option<String>,
    },
    /// Pre-write guard check
    Guard {
        #[arg(help = "File path relative to project root")]
        file: String,
        #[arg(short, long, help = "Project root (defaults to cwd)")]
        path: Option<String>,
        #[arg(long, help = "Delete guard check (default is write)")]
        delete: bool,
        #[arg(long)]
        json: bool,
    },
    /// Run policy smoke tests (match, guard, bootstrap, subagents)
    Test {
        path: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Verify ax preflight instruction and IDE bootstrap files (Recall instruction-sync parity)
    Sync {
        path: Option<String>,
        #[arg(long, help = "Restore missing or drifted managed policy files from embedded templates")]
        fix: bool,
    },
    /// Propose or save a policy rule from directive language in a prompt
    Capture {
        prompt: String,
        path: Option<String>,
        #[arg(long)]
        file: Vec<String>,
        #[arg(long, help = "Save the proposed rule without further confirmation")]
        yes: bool,
        #[arg(long)]
        json: bool,
    },
    /// Show or set policy storage mode (files vs database)
    Storage {
        #[command(subcommand)]
        action: PolicyStorageCommands,
    },
    /// Per-project shared pack export/import (git team sync)
    Pack {
        #[command(subcommand)]
        action: PolicyPackCommands,
    },
    /// Review pending pack-imported rules/skills
    Review {
        #[command(subcommand)]
        action: PolicyReviewCommands,
    },
    /// Remote policy share sync (GitHub / OneDrive Graph)
    Share {
        #[command(subcommand)]
        action: PolicyShareCommands,
    },
    /// Enable a rule or skill (matcher/preflight include it)
    Enable {
        /// Rule id or skill name
        id: String,
        path: Option<String>,
    },
    /// Disable a rule or skill (kept on disk/DB, skipped by matcher)
    Disable {
        /// Rule id or skill name
        id: String,
        path: Option<String>,
    },
    /// Restore a portable `.ax-policy.zip` into `.agents/`
    Restore {
        /// Zip path
        zip: String,
        path: Option<String>,
        #[arg(long, help = "Print preview JSON and do not write")]
        preview: bool,
        #[arg(long, help = "JSON file of rule:<id>|skill:<name> → overwrite|skip")]
        decisions: Option<String>,
    },
}

#[derive(Subcommand)]
enum PolicyPackCommands {
    /// Export shareable rules/skills (tag `shared`) to `.ax/policy/shared/`
    Export {
        path: Option<String>,
        #[arg(long, default_value = "shared")]
        tag: String,
        #[arg(long, help = "Output pack directory (default: .ax/policy/shared)")]
        out: Option<String>,
        #[arg(long)]
        quiet: bool,
    },
    /// Import pack into local policy (respects policy.requireReview)
    Import {
        path: Option<String>,
        #[arg(long, help = "Pack directory (default: .ax/policy/shared)")]
        pack: Option<String>,
        #[arg(long, help = "Overwrite conflicting local items without staging pending")]
        force: bool,
        #[arg(long)]
        quiet: bool,
    },
    /// Show local vs pack status
    Status {
        path: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Install a built-in pack (e.g. azdo-fullstack) into project policy
    Install {
        /// Pack name (see `ax policy pack install --list`)
        name: Option<String>,
        path: Option<String>,
        #[arg(long, help = "Overwrite existing rules/skills with the same id/name")]
        force: bool,
        #[arg(long, help = "List available built-in packs")]
        list: bool,
        #[arg(long)]
        json: bool,
    },
    /// Build a portable zip of selected rules and skills
    Zip {
        path: Option<String>,
        #[arg(long, help = "Output .ax-policy.zip path")]
        out: String,
        #[arg(long, help = "Package display name")]
        name: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long, help = "Comma-separated rule ids")]
        rules: Option<String>,
        #[arg(long, help = "Comma-separated skill names")]
        skills: Option<String>,
    },
}

#[derive(Subcommand)]
enum PolicyShareCommands {
    /// Show merged share config (~/.ax/config.json + project override)
    Config {
        path: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Pull (or push) rules/skills/memory from configured remote
    Sync {
        path: Option<String>,
        #[arg(long, help = "Pull from remote (default)")]
        pull: bool,
        #[arg(long, help = "Push local pack to remote (OneDrive only)")]
        push: bool,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum AuthCommands {
    /// Microsoft / OneDrive device-code sign-in
    Microsoft {
        #[command(subcommand)]
        action: MicrosoftAuthCommands,
    },
}

#[derive(Subcommand)]
enum MicrosoftAuthCommands {
    /// Start device code sign-in (interactive)
    Login,
    /// Clear stored Microsoft tokens
    Logout,
    /// Show Microsoft auth status
    Status {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum PolicyReviewCommands {
    /// List pending rules/skills
    List {
        path: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Show pending item + local diff
    Show {
        id: String,
        path: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Approve a pending item into active policy
    Approve {
        id: String,
        path: Option<String>,
    },
    /// Reject and drop a pending item
    Reject {
        id: String,
        path: Option<String>,
    },
}

#[derive(Subcommand)]
enum PolicyStorageCommands {
    /// Show effective policy storage mode (project + global)
    Status {
        path: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Store policy in ax.db (database source of truth)
    Database {
        path: Option<String>,
        #[arg(long, help = "Write to ~/.ax/config.json instead of project ax.json")]
        global: bool,
        #[arg(long, help = "Scan repo and import rules/skills into database")]
        migrate: bool,
        #[arg(long, help = "Apply migration with parsed defaults (skip per-item interview)")]
        yes: bool,
        #[arg(long)]
        json: bool,
    },
    /// Store policy on disk in .ax/policy/ (files source of truth)
    Files {
        path: Option<String>,
        #[arg(long, help = "Write to ~/.ax/config.json instead of project ax.json")]
        global: bool,
        #[arg(long, help = "Export database policy to .ax/policy/ files")]
        migrate: bool,
        #[arg(long)]
        json: bool,
    },
    /// Set per-item storage override for one rule or skill
    #[command(name = "set-item")]
    SetItem {
        /// Rule id or skill name
        id: String,
        /// Target storage: files | database
        storage: String,
        path: Option<String>,
        #[arg(long, help = "When switching to database, keep the markdown file on disk")]
        keep_file: bool,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum DaemonCommands {
    /// Show daemon status
    Status,
    /// Stop running daemon
    Stop,
    /// Restart shared MCP daemon (clears stale locks)
    Restart,
}

fn main() {
    // `ax desktop` must run on the OS main thread — winit panics on Windows if
    // EventLoop is created from the ax-main worker (see EventLoopBuilderExtWindows).
    if std::env::args().nth(1).as_deref() == Some("desktop") {
        run_desktop_on_os_main_thread();
        return;
    }

    // Windows main threads get 1 MB of stack; the combined command future is
    // large enough to overflow it in unoptimized builds. Run everything on a
    // worker thread with a roomier stack instead.
    let child = std::thread::Builder::new()
        .name("ax-main".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_stack_size(16 * 1024 * 1024)
                .build()
                .expect("failed to build tokio runtime");
            runtime.block_on(async_main());
        })
        .expect("failed to spawn main thread");
    if child.join().is_err() {
        std::process::exit(1);
    }
}

fn run_desktop_on_os_main_thread() {
    ui::init_terminal();
    commands::upgrade::apply_pending_upgrade();

    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("ax=info".parse().unwrap()))
        .with_writer(std::io::stderr)
        .try_init();

    let mut cmd = Cli::command();
    ui::configure_clap(&mut cmd);
    let matches = cmd.get_matches();
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());

    let result = match cli.command {
        Some(Commands::Desktop { path, port, bind }) => {
            commands::desktop::run(path, port, bind)
        }
        _ => Err("internal: expected `ax desktop`".into()),
    };

    if let Err(e) = result {
        if !e.is_empty() {
            eprintln!("{e}");
        }
        std::process::exit(1);
    }
}

async fn async_main() {
    ui::init_terminal();

    commands::upgrade::apply_pending_upgrade();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("ax=info".parse().unwrap()))
        .with_writer(std::io::stderr)
        .init();

    let mut cmd = Cli::command();
    ui::configure_clap(&mut cmd);
    let matches = cmd.get_matches();
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
    let cmd_name = cli_command_name(&cli.command);
    let should_check_update = should_notify_update(&cli.command);
    let result = match cli.command {
        None | Some(Commands::Install { .. }) => {
            let (yes, all, targets, path) = match &cli.command {
                Some(Commands::Install {
                    yes,
                    all,
                    target,
                    path,
                }) => (*yes, *all, target.clone(), path.clone()),
                _ => (false, false, Vec::new(), None),
            };
            commands::install::run(yes, all, targets, path)
        }
        Some(Commands::Uninstall) => commands::uninstall::run(),
        Some(Commands::Init { path, workspace }) => commands::init::run(path, workspace).await,
        Some(Commands::Uninit { path }) => commands::uninit::run(path).await,
        Some(Commands::Index {
            path,
            force,
            quiet,
            verbose,
            all_members,
        }) => commands::index::run(path, force, quiet, verbose, all_members).await,
        Some(Commands::Sync {
            path,
            quiet,
            watch,
            all_members,
        }) => commands::sync::run(path, quiet, watch, all_members).await,
        Some(Commands::Watch { path, quiet }) => {
            commands::sync::run(path, quiet, true, false).await
        }
        Some(Commands::Status { path, json }) => commands::status::run(path, json).await,
        Some(Commands::Query { text, kind, limit, json }) => {
            commands::query::run(text, kind, limit, json).await
        }
        Some(Commands::Explore { query, json }) => commands::explore::run(query, json).await,
        Some(Commands::Remember { text, title, kind, tags, files, json }) => {
            commands::memory::run_remember(text, title, kind, tags, files, json).await
        }
        Some(Commands::Recall { query, limit, json }) => {
            commands::memory::run_recall(query, limit, json).await
        }
        Some(Commands::CaptureGit { limit, quiet, json }) => {
            commands::memory::run_capture_git(limit, quiet, json).await
        }
        Some(Commands::Memory { action }) => match action {
            MemoryCommands::Export { tag, out, quiet } => {
                commands::memory::run_export(Some(tag), out, quiet).await
            }
            MemoryCommands::Import { path, quiet } => {
                commands::memory::run_import(path, quiet).await
            }
        },
        Some(Commands::Node { name }) => commands::node::run(name).await,
        Some(Commands::Files { format, json }) => commands::files::run(format, json).await,
        Some(Commands::Context { task }) => commands::context::run(task).await,
        Some(Commands::Callers { symbol }) => commands::callers::run(symbol).await,
        Some(Commands::Callees { symbol }) => commands::callees::run(symbol).await,
        Some(Commands::Impact { symbol }) => commands::impact::run(symbol).await,
        Some(Commands::Cycles { limit, json }) => commands::cycles::run(limit, json).await,
        Some(Commands::Path { from, to, json }) => commands::path::run(from, to, json).await,
        Some(Commands::Api { module, limit, json }) => {
            commands::api::run(module, limit, json).await
        }
        Some(Commands::Insights { path, resolution, god_limit, surprising_limit, json }) => {
            commands::insights::run(path, resolution, god_limit, surprising_limit, json).await
        }
        Some(Commands::Report { path, out, resolution, stdout }) => {
            commands::report::run(path, out, resolution, stdout).await
        }
        Some(Commands::Export { action }) => match action {
            ExportCommands::Okf {
                path,
                out,
                limit,
                check,
                ci,
                publish_wiki,
                dry_run,
                no_push,
                json,
            } => {
                commands::export_okf::run(commands::export_okf::ExportOkfArgs {
                    path,
                    out,
                    limit,
                    check,
                    ci,
                    publish_wiki,
                    dry_run,
                    no_push,
                    json,
                })
                .await
            }
            ExportCommands::Concepts { path, out, limit } => {
                commands::export_concepts::run(path, out, limit).await
            }
            ExportCommands::GraphHtml {
                path,
                out,
                resolution,
                limit,
            } => commands::export::run_graph_html(path, out, resolution, limit).await,
            ExportCommands::Graph {
                path,
                format,
                out,
                resolution,
                limit,
            } => commands::export::run_graph(path, out, &format, resolution, limit).await,
        },
        Some(Commands::Validate { path, ci, json }) => {
            commands::validate::run(path, ci, json).await
        }
        Some(Commands::Affected {
            files,
            stdin,
            base,
            depth,
            filter,
            json,
            quiet,
        }) => {
            commands::affected::run(commands::affected::AffectedArgs {
                files,
                stdin,
                base,
                depth,
                filter,
                json,
                quiet,
            })
            .await
        }
        Some(Commands::Diff { base, json }) => commands::diff::run(base, json).await,
        Some(Commands::TestImpact { base, json }) => commands::test_impact::run(base, json).await,
        Some(Commands::Ship {
            path,
            watch,
            evaluate,
            ci,
            draft,
            title,
            port,
            open,
            auto_commit,
            revert_on_fail,
        }) => {
            commands::ship::run(
                path,
                watch,
                evaluate,
                ci,
                draft,
                title,
                port,
                open,
                auto_commit,
                revert_on_fail,
            )
            .await
        }
        Some(Commands::Unlock { path }) => commands::unlock::run(path).await,
        Some(Commands::Daemon { path, action }) => {
            let act = match action {
                Some(DaemonCommands::Stop) => commands::daemon::DaemonAction::Stop,
                Some(DaemonCommands::Restart) => commands::daemon::DaemonAction::Restart,
                Some(DaemonCommands::Status) | None => commands::daemon::DaemonAction::Status,
            };
            commands::daemon::run(path, act).await
        }
        Some(Commands::Web { path, port, open }) => commands::web::run(path, port, open).await,
        Some(Commands::Desktop { .. }) => Err(
            "internal: `ax desktop` must run on the OS main thread (handled in main())".into(),
        ),
        Some(Commands::Share {
            path,
            port,
            bind,
            open,
            token,
        }) => commands::share::run(path, port, bind, open, token).await,
        Some(Commands::Lsp { action }) => match action {
            LspCommands::Status { json } => commands::lsp::run_status(json).await,
            LspCommands::Enrich { path, limit, json } => {
                commands::lsp::run_enrich(path, limit, json).await
            }
        },
        Some(Commands::Cursor { action }) => match action {
            CursorCommands::Auth { action } => match action {
                CursorAuthCommands::Status { json } => commands::cursor::run_status(json),
                CursorAuthCommands::List { json } => commands::cursor::run_list(json),
                CursorAuthCommands::Save {
                    name,
                    label,
                    from_auth_json,
                    email,
                    membership,
                    subscription_status,
                    sign_up_type,
                } => commands::cursor::run_save(
                    name,
                    label,
                    from_auth_json,
                    email,
                    membership,
                    subscription_status,
                    sign_up_type,
                ),
                CursorAuthCommands::Use { name, force, json } => {
                    commands::cursor::run_use(name, force, json)
                }
                CursorAuthCommands::Show { name, json } => commands::cursor::run_show(name, json),
            },
        },
        Some(Commands::Policy { action }) => match action {
            PolicyCommands::Index { path, force } => commands::policy::run_index(path, force).await,
            PolicyCommands::Import { path } => commands::policy::run_import(path).await,
            PolicyCommands::Pull { url, path, name } => {
                commands::policy::run_pull(url, path, name).await
            }
            PolicyCommands::Export { path, out } => commands::policy::run_export(path, out).await,
            PolicyCommands::Match { prompt, path, file, json } => {
                commands::policy::run_match(path, prompt, file, json).await
            }
            PolicyCommands::Rules { path, json } => commands::policy::run_rules(path, json).await,
            PolicyCommands::Skills { path, json } => commands::policy::run_skills(path, json).await,
            PolicyCommands::Skill { name, path } => commands::policy::run_skill(path, name).await,
            PolicyCommands::Guard { file, path, delete, json } => {
                commands::policy::run_guard(path, file, delete, json).await
            }
            PolicyCommands::Sync { path, fix } => commands::policy::run_sync(path, fix).await,
            PolicyCommands::Test { path, json } => commands::policy::run_test(path, json).await,
            PolicyCommands::Capture { prompt, path, file, yes, json } => {
                commands::policy::run_capture(path, prompt, file, yes, json).await
            }
            PolicyCommands::Storage { action } => match action {
                PolicyStorageCommands::Status { path, json } => {
                    commands::policy::run_storage_status(path, json).await
                }
                PolicyStorageCommands::Database { path, global, migrate, yes, json } => {
                    commands::policy::run_storage_set(
                        path,
                        PolicyStorage::Database,
                        global,
                        migrate,
                        yes,
                        json,
                    )
                    .await
                }
                PolicyStorageCommands::Files { path, global, migrate, json } => {
                    commands::policy::run_storage_set(
                        path,
                        PolicyStorage::Files,
                        global,
                        migrate,
                        false,
                        json,
                    )
                    .await
                }
                PolicyStorageCommands::SetItem {
                    id,
                    storage,
                    path,
                    keep_file,
                    json,
                } => {
                    commands::policy::run_storage_set_item(path, id, storage, keep_file, json).await
                }
            },
            PolicyCommands::Pack { action } => match action {
                PolicyPackCommands::Export { path, tag, out, quiet } => {
                    commands::policy::run_pack_export(path, tag, out, quiet).await
                }
                PolicyPackCommands::Import {
                    path,
                    pack,
                    force,
                    quiet,
                } => commands::policy::run_pack_import(path, pack, force, quiet).await,
                PolicyPackCommands::Status { path, json } => {
                    commands::policy::run_pack_status(path, json).await
                }
                PolicyPackCommands::Install {
                    name,
                    path,
                    force,
                    list,
                    json,
                } => commands::policy::run_pack_install(path, name, force, list, json).await,
                PolicyPackCommands::Zip {
                    path,
                    out,
                    name,
                    description,
                    rules,
                    skills,
                } => commands::policy::run_pack_zip(path, out, name, description, rules, skills).await,
            },
            PolicyCommands::Review { action } => match action {
                PolicyReviewCommands::List { path, json } => {
                    commands::policy::run_review_list(path, json).await
                }
                PolicyReviewCommands::Show { id, path, json } => {
                    commands::policy::run_review_show(path, id, json).await
                }
                PolicyReviewCommands::Approve { id, path } => {
                    commands::policy::run_review_approve(path, id).await
                }
                PolicyReviewCommands::Reject { id, path } => {
                    commands::policy::run_review_reject(path, id).await
                }
            },
            PolicyCommands::Share { action } => match action {
                PolicyShareCommands::Config { path, json } => {
                    commands::policy_share::run_config(path, json).await
                }
                PolicyShareCommands::Sync { path, pull, push, json } => {
                    commands::policy_share::run_sync(path, pull, push, json).await
                }
            },
            PolicyCommands::Enable { id, path } => commands::policy::run_enable(path, id).await,
            PolicyCommands::Disable { id, path } => commands::policy::run_disable(path, id).await,
            PolicyCommands::Restore {
                zip,
                path,
                preview,
                decisions,
            } => commands::policy::run_policy_restore(path, zip, preview, decisions).await,
        },
        Some(Commands::Auth { action }) => match action {
            AuthCommands::Microsoft { action } => match action {
                MicrosoftAuthCommands::Login => commands::policy_share::ms_login().await,
                MicrosoftAuthCommands::Logout => commands::policy_share::ms_logout(),
                MicrosoftAuthCommands::Status { json } => commands::policy_share::ms_status(json),
            },
        },
        Some(Commands::PromptHook) => commands::prompt_hook::run().await,
        Some(Commands::SessionHook) => commands::session_hook::run().await,
        Some(Commands::StopHook) => commands::stop_hook::run().await,
        Some(Commands::WatchdogChild { parent_pid, timeout_ms }) => {
            ax_mcp::run_watchdog_child(parent_pid, timeout_ms);
            Ok(())
        }
        Some(Commands::UpgradeApply { parent_pid, staging, dest }) => {
            commands::upgrade::run_upgrade_apply(parent_pid, staging, dest)
        }
        Some(Commands::Version) => {
            println!("{} {}", ui::accent("ax"), env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some(Commands::Upgrade { version, check, local }) => {
            commands::upgrade::run(version, check, local).await
        }
        Some(Commands::Telemetry { action }) => commands::telemetry::run(action).await,
        Some(Commands::Savings { action, period, from, to, json }) => match action {
            Some(SavingsAction::Import { claude, cursor, all }) => {
                commands::savings::run_import(claude, cursor, all).await
            }
            Some(SavingsAction::TagSession {
                agent,
                session_id,
                model,
            }) => commands::savings::run_tag_session(agent, session_id, model).await,
            Some(SavingsAction::Hook { action }) => match action {
                SavingsHookAction::Install => commands::savings::run_hook_install(),
            },
            None => commands::savings::run_summary(period, from, to, json).await,
        },
        Some(Commands::Pricing { action }) => match action {
            PricingAction::Sync { force } => commands::pricing::run_sync(force).await,
            PricingAction::Status { json } => commands::pricing::run_status(json).await,
            PricingAction::List { source, json } => {
                commands::pricing::run_list(source, json).await
            }
            PricingAction::History {
                model,
                source,
                days,
                json,
            } => commands::pricing::run_history(model, source, days, json).await,
        },
        Some(Commands::DocsCatalog { action }) => match action {
            DocsCatalogAction::Sync {
                skip_wiki_pull,
                dry_run,
                json,
            } => commands::docs_catalog::run_sync(skip_wiki_pull, dry_run, json).await,
        },
        Some(Commands::Mcp { action }) => match action {
            McpAction::Audit {
                path,
                session,
                window_minutes,
                json,
            } => commands::mcp::run(path, session, window_minutes, json),
        },
        Some(Commands::Offload { action }) => match action {
            Some(OffloadCommands::Status) => commands::offload::run(Some("status".into()), None, None),
            Some(OffloadCommands::SetEndpoint { url, key_env }) => {
                commands::offload::run(Some("set-endpoint".into()), Some(url), key_env)
            }
            Some(OffloadCommands::Clear) => commands::offload::run(Some("clear".into()), None, None),
            None => commands::offload::run(Some("status".into()), None, None),
        },
        Some(Commands::Serve { mcp, daemon, path }) if mcp && daemon => {
            let root = commands::resolve_path(path);
            ax_mcp::run_daemon(root).await.map_err(|e| e.to_string())
        }
        Some(Commands::Serve { mcp, path, .. }) if mcp => {
            let root = path.map(std::path::PathBuf::from);
            ax_mcp::run_stdio_server(root).await.map_err(|e| e.to_string())
        }
        Some(Commands::Serve { .. }) => Err("use ax serve --mcp".to_string()),
    };

    if result.is_ok() && should_check_update {
        version_check::maybe_notify_update().await;
    }

    if let Some(name) = cmd_name {
        if !matches!(name.as_str(), "telemetry" | "serve" | "upgrade") {
            if let Ok(mut t) = ax_telemetry::telemetry().lock() {
                t.record_usage("cli_command", &name, result.is_ok(), None);
                t.persist_sync();
                t.flush_now(ax_telemetry::DEFAULT_FLUSH_TIMEOUT_MS).await;
            }
        }
    }

    if let Err(e) = result {
        eprintln!("{}", ui::err_line(e));
        std::process::exit(1);
    }
}

fn should_notify_update(cmd: &Option<Commands>) -> bool {
    match cmd {
        Some(Commands::Serve { .. })
        |         Some(Commands::PromptHook)
        | Some(Commands::SessionHook)
        | Some(Commands::StopHook)
        | Some(Commands::WatchdogChild { .. })
        | Some(Commands::UpgradeApply { .. })
        | Some(Commands::Upgrade { .. })
        | Some(Commands::Version) => false,
        Some(Commands::Index { quiet: true, .. })
        | Some(Commands::Sync { quiet: true, .. })
        | Some(Commands::Watch { quiet: true, .. }) => false,
        _ => true,
    }
}

fn cli_command_name(cmd: &Option<Commands>) -> Option<String> {
    match cmd {
        None => Some("install".into()),
        Some(Commands::Install { .. }) => Some("install".into()),
        Some(Commands::Uninstall) => Some("uninstall".into()),
        Some(Commands::Init { .. }) => Some("init".into()),
        Some(Commands::Uninit { .. }) => Some("uninit".into()),
        Some(Commands::Index { .. }) => Some("index".into()),
        Some(Commands::Sync { .. }) => Some("sync".into()),
        Some(Commands::Watch { .. }) => Some("watch".into()),
        Some(Commands::Status { .. }) => Some("status".into()),
        Some(Commands::Query { .. }) => Some("query".into()),
        Some(Commands::Explore { .. }) => Some("explore".into()),
        Some(Commands::Remember { .. }) => Some("remember".into()),
        Some(Commands::Recall { .. }) => Some("recall".into()),
        Some(Commands::CaptureGit { .. }) => Some("capture-git".into()),
        Some(Commands::Memory { .. }) => Some("memory".into()),
        Some(Commands::Node { .. }) => Some("node".into()),
        Some(Commands::Files { .. }) => Some("files".into()),
        Some(Commands::Context { .. }) => Some("context".into()),
        Some(Commands::Callers { .. }) => Some("callers".into()),
        Some(Commands::Callees { .. }) => Some("callees".into()),
        Some(Commands::Impact { .. }) => Some("impact".into()),
        Some(Commands::Cycles { .. }) => Some("cycles".into()),
        Some(Commands::Path { .. }) => Some("path".into()),
        Some(Commands::Api { .. }) => Some("api".into()),
        Some(Commands::Insights { .. }) => Some("insights".into()),
        Some(Commands::Report { .. }) => Some("report".into()),
        Some(Commands::Export { .. }) => Some("export".into()),
        Some(Commands::Validate { .. }) => Some("validate".into()),
        Some(Commands::Affected { .. }) => Some("affected".into()),
        Some(Commands::Diff { .. }) => Some("diff".into()),
        Some(Commands::TestImpact { .. }) => Some("test-impact".into()),
        Some(Commands::Ship { .. }) => Some("ship".into()),
        Some(Commands::Unlock { .. }) => Some("unlock".into()),
        Some(Commands::Daemon { .. }) => Some("daemon".into()),
        Some(Commands::Version) => Some("version".into()),
        Some(Commands::Upgrade { .. }) => Some("upgrade".into()),
        Some(Commands::Telemetry { .. }) => Some("telemetry".into()),
        Some(Commands::Savings { .. }) => Some("savings".into()),
        Some(Commands::Pricing { .. }) => Some("pricing".into()),
        Some(Commands::DocsCatalog { .. }) => Some("docs-catalog".into()),
        Some(Commands::Mcp { .. }) => Some("mcp".into()),
        Some(Commands::Offload { .. }) => Some("offload".into()),
        Some(Commands::Web { .. }) => Some("web".into()),
        Some(Commands::Desktop { .. }) => Some("desktop".into()),
        Some(Commands::Share { .. }) => Some("share".into()),
        Some(Commands::Lsp { .. }) => Some("lsp".into()),
        Some(Commands::Cursor { .. }) => Some("cursor".into()),
        Some(Commands::Policy { .. }) => Some("policy".into()),
        Some(Commands::Auth { .. }) => Some("auth".into()),
        Some(Commands::PromptHook) => None,
        Some(Commands::SessionHook) => None,
        Some(Commands::StopHook) => None,
        Some(Commands::WatchdogChild { .. }) => None,
        Some(Commands::UpgradeApply { .. }) => None,
        Some(Commands::Serve { .. }) => Some("serve".into()),
    }
}