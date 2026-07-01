//! ax CLI entry point.

mod commands;
mod help_text;
mod installer;
mod ui;
mod version_check;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
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
    },
    /// Remove ax from agent configs
    #[command(long_about = help_text::UNINSTALL_LONG)]
    Uninstall,
    /// Initialize project and index
    #[command(long_about = help_text::INIT_LONG)]
    Init { path: Option<String> },
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
    },
    /// Incremental sync
    #[command(long_about = help_text::SYNC_LONG)]
    Sync {
        path: Option<String>,
        #[arg(long)]
        quiet: bool,
        #[arg(long, help = "Watch for changes and auto-sync (debounced)")]
        watch: bool,
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
    /// Affected tests
    #[command(long_about = help_text::AFFECTED_LONG)]
    Affected { files: Vec<String> },
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
    },
    /// Anonymous usage telemetry (on|off|status)
    #[command(long_about = help_text::TELEMETRY_LONG)]
    Telemetry {
        #[arg(help = "on, off, or status")]
        action: Option<String>,
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
    /// Policy rules and skills
    Policy {
        #[command(subcommand)]
        action: PolicyCommands,
    },
    /// Claude UserPromptSubmit hook (hidden; reads {prompt,cwd} JSON on stdin)
    #[command(hide = true, name = "prompt-hook")]
    PromptHook,
    /// Hidden liveness watchdog child (spawned by ax MCP/daemon)
    #[command(hide = true, name = "watchdog-child")]
    WatchdogChild {
        parent_pid: u32,
        timeout_ms: u64,
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
        path: Option<String>,
        file: String,
        #[arg(long)]
        write: bool,
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
}

#[tokio::main]
async fn main() {
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
            let (yes, all) = match &cli.command {
                Some(Commands::Install { yes, all }) => (*yes, *all),
                _ => (false, false),
            };
            commands::install::run(yes, all)
        }
        Some(Commands::Uninstall) => commands::uninstall::run(),
        Some(Commands::Init { path }) => commands::init::run(path).await,
        Some(Commands::Uninit { path }) => commands::uninit::run(path).await,
        Some(Commands::Index { path, force, quiet, verbose }) => {
            commands::index::run(path, force, quiet, verbose).await
        }
        Some(Commands::Sync { path, quiet, watch }) => commands::sync::run(path, quiet, watch).await,
        Some(Commands::Watch { path, quiet }) => commands::sync::run(path, quiet, true).await,
        Some(Commands::Status { path, json }) => commands::status::run(path, json).await,
        Some(Commands::Query { text, kind, limit, json }) => {
            commands::query::run(text, kind, limit, json).await
        }
        Some(Commands::Explore { query, json }) => commands::explore::run(query, json).await,
        Some(Commands::Node { name }) => commands::node::run(name).await,
        Some(Commands::Files { format, json }) => commands::files::run(format, json).await,
        Some(Commands::Context { task }) => commands::context::run(task).await,
        Some(Commands::Callers { symbol }) => commands::callers::run(symbol).await,
        Some(Commands::Callees { symbol }) => commands::callees::run(symbol).await,
        Some(Commands::Impact { symbol }) => commands::impact::run(symbol).await,
        Some(Commands::Affected { files }) => commands::affected::run(files).await,
        Some(Commands::Unlock { path }) => commands::unlock::run(path).await,
        Some(Commands::Daemon { path, action }) => {
            let act = match action {
                Some(DaemonCommands::Stop) => commands::daemon::DaemonAction::Stop,
                Some(DaemonCommands::Status) | None => commands::daemon::DaemonAction::Status,
            };
            commands::daemon::run(path, act).await
        }
        Some(Commands::Web { path, port, open }) => commands::web::run(path, port, open).await,
        Some(Commands::Policy { action }) => match action {
            PolicyCommands::Index { path, force } => commands::policy::run_index(path, force).await,
            PolicyCommands::Import { path } => commands::policy::run_import(path).await,
            PolicyCommands::Export { path, out } => commands::policy::run_export(path, out).await,
            PolicyCommands::Match { prompt, path, file, json } => {
                commands::policy::run_match(path, prompt, file, json).await
            }
            PolicyCommands::Rules { path, json } => commands::policy::run_rules(path, json).await,
            PolicyCommands::Skills { path, json } => commands::policy::run_skills(path, json).await,
            PolicyCommands::Skill { name, path } => commands::policy::run_skill(path, name).await,
            PolicyCommands::Guard { path, file, write, json } => {
                commands::policy::run_guard(path, file, write, json).await
            }
        },
        Some(Commands::PromptHook) => commands::prompt_hook::run().await,
        Some(Commands::WatchdogChild { parent_pid, timeout_ms }) => {
            ax_mcp::run_watchdog_child(parent_pid, timeout_ms);
            Ok(())
        }
        Some(Commands::Version) => {
            println!("{} {}", ui::accent("ax"), env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some(Commands::Upgrade { version, check }) => commands::upgrade::run(version, check).await,
        Some(Commands::Telemetry { action }) => commands::telemetry::run(action).await,
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
        Some(Commands::Serve { mcp, .. }) if mcp => {
            ax_mcp::run_stdio_server().await.map_err(|e| e.to_string())
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
        | Some(Commands::PromptHook)
        | Some(Commands::WatchdogChild { .. })
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
        Some(Commands::Node { .. }) => Some("node".into()),
        Some(Commands::Files { .. }) => Some("files".into()),
        Some(Commands::Context { .. }) => Some("context".into()),
        Some(Commands::Callers { .. }) => Some("callers".into()),
        Some(Commands::Callees { .. }) => Some("callees".into()),
        Some(Commands::Impact { .. }) => Some("impact".into()),
        Some(Commands::Affected { .. }) => Some("affected".into()),
        Some(Commands::Unlock { .. }) => Some("unlock".into()),
        Some(Commands::Daemon { .. }) => Some("daemon".into()),
        Some(Commands::Version) => Some("version".into()),
        Some(Commands::Upgrade { .. }) => Some("upgrade".into()),
        Some(Commands::Telemetry { .. }) => Some("telemetry".into()),
        Some(Commands::Offload { .. }) => Some("offload".into()),
        Some(Commands::Web { .. }) => Some("web".into()),
        Some(Commands::Policy { .. }) => Some("policy".into()),
        Some(Commands::PromptHook) => None,
        Some(Commands::WatchdogChild { .. }) => None,
        Some(Commands::Serve { .. }) => Some("serve".into()),
    }
}