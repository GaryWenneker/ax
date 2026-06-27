//! ax CLI entry point.

mod commands;
mod help_text;
mod installer;
mod ui;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "ax", version, about = "ax code intelligence tool")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Interactive installer
    Install,
    /// Remove ax from agent configs
    Uninstall,
    /// Initialize project and index
    Init { path: Option<String> },
    /// Remove .ax directory
    Uninit { path: Option<String> },
    /// Full re-index
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
    Sync {
        path: Option<String>,
        #[arg(long)]
        quiet: bool,
        #[arg(long, help = "Watch for changes and auto-sync (debounced)")]
        watch: bool,
    },
    /// Watch for file changes and auto-sync (alias for sync --watch)
    Watch {
        path: Option<String>,
        #[arg(long)]
        quiet: bool,
    },
    /// Index statistics
    Status {
        path: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// FTS symbol search
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
    Explore { query: Vec<String>, #[arg(long)] json: bool },
    /// Node details (same as ax_node MCP tool)
    Node { name: Option<String> },
    /// List project files
    Files {
        #[arg(long)]
        format: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Build task context
    Context { task: String },
    /// Find callers
    Callers { symbol: String },
    /// Find callees
    Callees { symbol: String },
    /// Impact radius
    Impact { symbol: String },
    /// Affected tests
    Affected { files: Vec<String> },
    /// Remove stale ax.lock
    Unlock { path: Option<String> },
    /// MCP daemon status/stop
    Daemon {
        path: Option<String>,
        #[command(subcommand)]
        action: Option<DaemonCommands>,
    },
    /// Print version
    Version,
    /// Self-update from GitHub releases or cargo install
    Upgrade {
        #[arg(help = "Optional release tag (e.g. v0.1.0)")]
        version: Option<String>,
    },
    /// Anonymous usage telemetry (on|off|status)
    Telemetry {
        #[arg(help = "on, off, or status")]
        action: Option<String>,
    },
    /// Explore reasoning offload configuration
    Offload {
        #[command(subcommand)]
        action: Option<OffloadCommands>,
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
enum DaemonCommands {
    /// Show daemon status
    Status,
    /// Stop running daemon
    Stop,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("ax=info".parse().unwrap()))
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let cmd_name = cli_command_name(&cli.command);
    let result = match cli.command {
        None | Some(Commands::Install) => commands::install::run(),
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
        Some(Commands::PromptHook) => commands::prompt_hook::run().await,
        Some(Commands::WatchdogChild { parent_pid, timeout_ms }) => {
            ax_mcp::run_watchdog_child(parent_pid, timeout_ms);
            Ok(())
        }
        Some(Commands::Version) => {
            println!("ax {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some(Commands::Upgrade { version }) => commands::upgrade::run(version),
        Some(Commands::Telemetry { action }) => commands::telemetry::run(action),
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

    if let Some(name) = cmd_name {
        if name != "telemetry" {
            if let Ok(mut t) = ax_telemetry::telemetry().lock() {
                t.record_usage("cli_command", &name, result.is_ok(), None);
                t.persist_sync();
            }
        }
    }

    if let Err(e) = result {
        eprintln!("{}", ui::err_line(e));
        std::process::exit(1);
    }
}

fn cli_command_name(cmd: &Option<Commands>) -> Option<String> {
    match cmd {
        None => Some("install".into()),
        Some(Commands::Install) => Some("install".into()),
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
        Some(Commands::PromptHook) => None,
        Some(Commands::WatchdogChild { .. }) => None,
        Some(Commands::Serve { .. }) => Some("serve".into()),
    }
}