//! Installer for AI agent targets.

pub mod report;
pub mod targets;

use std::path::Path;

use ax_telemetry::telemetry;

use crate::ui::install_log;

pub struct InstallOptions {
    pub yes: bool,
    pub install_all: bool,
}

pub fn run_installer(project_root: &Path, opts: InstallOptions) -> Result<(), String> {
    if !opts.yes {
        if let Ok(mut t) = telemetry().lock() {
            if !t.has_stored_choice() {
                let on = crate::commands::telemetry::ask_installer_consent();
                t.set_enabled(on, "installer");
                t.persist_sync();
            }
        }
    }

    install_log::intro(env!("CARGO_PKG_VERSION"));

    let summary = targets::install_detected(project_root, opts.install_all || opts.yes)?;

    let warning = if summary.reports.is_empty() {
        Some("No supported agents detected. Install Cursor or Claude Code, or run with --all.")
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
    let reports = targets::uninstall_all()?;
    install_log::render_uninstall(&reports, env!("CARGO_PKG_VERSION"));
    Ok(())
}
