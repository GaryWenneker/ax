use crate::commands::resolve_path;

pub async fn run_status(json: bool) -> Result<(), String> {
    let servers = ax_lsp::discover_servers();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&servers).unwrap_or_default()
        );
    } else {
        println!("LSP servers:");
        for s in &servers {
            let mark = if s.available { "ok" } else { "--" };
            let path = s.path.as_deref().unwrap_or("(not found)");
            println!("  [{mark}] {:<28} {path}", s.id);
            if s.path.is_some() && !s.available {
                println!(
                    "         (shim on PATH but not runnable — e.g. `rustup component add rust-analyzer`)"
                );
            }
        }
    }
    Ok(())
}

pub async fn run_enrich(path: Option<String>, limit: usize, json: bool) -> Result<(), String> {
    let root = resolve_path(path);
    let ax = ax_core::Ax::open(&root)
        .await
        .map_err(|e| e.to_string())?;
    ax_usage::log_lsp(Some(ax.project_root()), format!("enrich start limit={limit}"));
    let report = ax_lsp::enrich_project(ax.project_root(), ax.queries(), limit)
        .await
        .map_err(|e| e.to_string())?;
    ax_usage::log_lsp(
        Some(ax.project_root()),
        format!(
            "enrich examined={} resolved={} no_server={} no_def={} errors={}",
            report.examined,
            report.resolved,
            report.skipped_no_server,
            report.skipped_no_definition,
            report.errors.len()
        ),
    );
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
    } else {
        println!(
            "LSP enrich: examined={} resolved={} no_server={} no_def={} errors={}",
            report.examined,
            report.resolved,
            report.skipped_no_server,
            report.skipped_no_definition,
            report.errors.len()
        );
        for e in report.errors.iter().take(10) {
            eprintln!("  warn: {e}");
        }
        if report.skipped_no_server > 0 && report.resolved == 0 {
            eprintln!(
                "hint: install a language server (e.g. `rustup component add rust-analyzer`) then re-run"
            );
        }
    }
    Ok(())
}
