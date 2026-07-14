use std::path::PathBuf;

use crate::commands::resolve_path;
use crate::ui::SpinnerGuard;

pub async fn run(
    path: Option<String>,
    out: Option<String>,
    resolution: f64,
    stdout: bool,
) -> Result<(), String> {
    let root = resolve_path(path);
    let resolution = if resolution > 0.0 { resolution } else { 1.0 };

    let markdown = {
        let _spinner = SpinnerGuard::new("Building architecture report...".to_string(), stdout);
        let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
        ax.architecture_report(resolution).await.map_err(|e| e.to_string())?
    };

    if stdout {
        print!("{markdown}");
        return Ok(());
    }

    let out_path = out
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("AX_REPORT.md"));
    std::fs::write(&out_path, &markdown)
        .map_err(|e| format!("failed to write {}: {e}", out_path.display()))?;
    println!("Wrote architecture report to {}", out_path.display());
    Ok(())
}
