use ax_admin::{
    login::BrowserLogin,
    secrets::{KeyringStore, MemoryStore, SecretStore},
    server::{bind, router, App},
    store::{home_dir, AuthKind, Store},
};
use clap::{Parser, Subcommand};
use std::sync::Arc;

/// Admin application for a remote Ax. It connects through the environment's
/// remote MCP endpoint and serves its interface on this computer only.
#[derive(Parser)]
#[command(name = "ax-admin", version, about)]
struct Cli {
    /// Loopback port for the interface (0 picks a free port)
    #[arg(long, default_value_t = 7080)]
    port: u16,
    /// Do not open the browser
    #[arg(long)]
    no_open: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// List saved connections (names and addresses; never keys)
    Connections,
    /// Print the version
    Version,
}

fn secret_store() -> Arc<dyn SecretStore> {
    // In-memory secrets are for automated UI tests: nothing is written, and
    // saved connections lose their key when the process exits.
    if std::env::var("AX_ADMIN_SECRET_STORE").as_deref() == Ok("memory") {
        eprintln!("ax-admin: keys are kept in memory only (AX_ADMIN_SECRET_STORE=memory)");
        Arc::new(MemoryStore::default())
    } else {
        Arc::new(KeyringStore)
    }
}

fn list_connections() -> Result<(), String> {
    let store = Store::open(&home_dir()?)?;
    if store.list().is_empty() {
        println!("No saved connections. Start ax-admin and add one.");
        return Ok(());
    }
    for connection in store.list() {
        let marker = if store.active_id() == Some(connection.id.as_str()) {
            "*"
        } else {
            " "
        };
        let auth = match connection.auth {
            AuthKind::ApiKey => "API key",
            AuthKind::Oauth => "browser sign-in",
        };
        println!("{marker} {}\t{}\t{auth}", connection.name, connection.url);
    }
    Ok(())
}

async fn serve(port: u16, open: bool) -> Result<(), String> {
    let listener = bind(port).await?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let app = App::new(home_dir()?, secret_store(), Arc::new(BrowserLogin), port)?;
    let url = format!("http://127.0.0.1:{port}/");
    println!("ax-admin is running at {url}");
    println!("Press Ctrl+C to stop.");
    if open {
        ax_remote_client::client::open_browser(&url);
    }
    axum::serve(listener, router(app))
        .with_graceful_shutdown(async {
            if tokio::signal::ctrl_c().await.is_err() {
                // Without a signal handler, keep serving until the process is killed.
                std::future::pending::<()>().await;
            }
        })
        .await
        .map_err(|e| e.to_string())
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Some(Command::Version) => {
            println!("ax-admin {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some(Command::Connections) => list_connections(),
        None => serve(cli.port, !cli.no_open).await,
    };
    if let Err(message) = result {
        eprintln!("ax-admin: {message}");
        std::process::exit(1);
    }
}
