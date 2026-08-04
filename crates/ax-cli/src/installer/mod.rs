//! Installer for AI agent targets — CLI wrapper around ax-installer.

pub use ax_installer::{self};

use std::path::Path;

use ax_telemetry::telemetry;

use crate::ui::install_log;

pub struct InstallOptions {
    pub yes: bool,
    pub install_all: bool,
    /// When non-empty, only these target ids are wired (e.g. `takumi`).
    pub targets: Vec<String>,
}

pub fn run_installer(project_root: &Path, opts: InstallOptions) -> Result<(), String> {
    if !opts.yes && opts.targets.is_empty() {
        if let Ok(mut t) = telemetry().lock() {
            if !t.has_stored_choice() {
                let on = crate::commands::telemetry::ask_installer_consent();
                t.set_enabled(on, "installer");
                t.persist_sync();
            }
        }
    }

    ax_installer::ensure_global_config();

    install_log::intro(env!("CARGO_PKG_VERSION"));

    let summary = if !opts.targets.is_empty() {
        ax_installer::install_targets(project_root, &opts.targets)?
    } else {
        ax_installer::install_detected(project_root, opts.install_all || opts.yes)?
    };

    let warning = if summary.reports.is_empty() {
        Some(if opts.targets.is_empty() {
            "No supported agents detected. Install Cursor or Claude Code, or run with --all / --target."
        } else {
            "No files written for the requested --target value(s). Check the id (e.g. takumi, vscode)."
        })
    } else {
        None
    };

    let project_hint = if project_root == Path::new(".") {
        "<your-project>".to_string()
    } else {
        install_log::tildify(project_root)
    };

    install_log::render_install(&summary, &project_hint, warning);

    if let Ok(mut t) = telemetry().lock() {
        let ids: Vec<_> = summary.configured_targets();
        if !ids.is_empty() {
            t.record_lifecycle(
                "install",
                serde_json::json!({
                    "targets": ids,
                    "scope": "global",
                    "kind": "upgrade",
                }),
            );
            t.persist_sync();
        }
    }

    Ok(())
}

pub fn run_uninstall() -> Result<(), String> {
    let reports = ax_installer::uninstall_all()?;
    install_log::render_uninstall(&reports, env!("CARGO_PKG_VERSION"));
    Ok(())
}
