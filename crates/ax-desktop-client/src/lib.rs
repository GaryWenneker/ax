//! Native wgpu Command Center library (egui/eframe).
//!
//! Starts an embedded `ax-web` HTTP server in-process and renders the same
//! `/api` surface as the browser Command Center. Used by the `ax-desktop`
//! binary and the `ax desktop` CLI subcommand.

mod api;
mod app;
mod pages;
mod server;
mod theme;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing_subscriber::EnvFilter;

use crate::app::DesktopApp;
use crate::server::EmbeddedServer;

/// Launch the native Command Center window for `root`.
///
/// Binds an embedded `ax-web` server at `http://{bind}:{port}` and blocks until
/// the window is closed.
pub fn run(path: Option<String>, port: u16, bind: String) -> Result<(), String> {
    // ax-cli already initializes tracing; the standalone binary may not.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .with_target(false)
        .try_init();

    let root = resolve_root(path);
    let base_url = format!("http://{bind}:{port}");

    eprintln!("ax desktop  starting embedded ax-web on {base_url}");
    eprintln!("  Graph + policy: {}", root.display());

    let server = EmbeddedServer::start(root.clone(), port, bind)
        .map(Arc::new)
        .map_err(|e| format!("failed to start embedded server: {e}"))?;

    // Give the listener a moment to bind before the first UI fetch.
    std::thread::sleep(std::time::Duration::from_millis(350));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 600.0])
            .with_title("ax — Command Center (desktop)"),
        ..Default::default()
    };

    let root_for_app = root;
    let server_for_app = server.clone();
    eframe::run_native(
        "ax-desktop",
        options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            theme::apply_dark_theme(&cc.egui_ctx);
            Ok(Box::new(DesktopApp::new(
                base_url,
                root_for_app,
                server_for_app,
            )))
        }),
    )
    .map_err(|e| e.to_string())
}

fn resolve_root(path: Option<String>) -> PathBuf {
    let raw = path
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    canonicalize_path(&raw)
}

fn canonicalize_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
