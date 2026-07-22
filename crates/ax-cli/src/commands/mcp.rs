//! `ax mcp audit` — correlate verbose MCP log with Cursor transcripts.

use std::path::PathBuf;

use ax_usage::{
    audit_project, format_markdown_report, AuditOptions, DEFAULT_WINDOW_MINUTES,
};

use crate::commands::resolve_path;

pub fn run(
    path: Option<String>,
    session: Option<String>,
    window_minutes: Option<u64>,
    json: bool,
) -> Result<(), String> {
    let root = resolve_path(path);
    let mut opts = AuditOptions {
        window_minutes: Some(window_minutes.unwrap_or(DEFAULT_WINDOW_MINUTES)),
        persist: true,
        ..Default::default()
    };
    if let Some(s) = session {
        let p = PathBuf::from(&s);
        if p.is_file() {
            opts.session_path = Some(p);
            opts.window_minutes = None;
        } else {
            opts.session_id = Some(s);
            opts.window_minutes = None;
        }
    }

    let snap = audit_project(&root, &opts)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&snap).unwrap_or_default());
        return Ok(());
    }

    print!("{}", format_markdown_report(&snap));

    // Non-zero exit when critical findings (useful for CI).
    if snap.critical_count > 0 {
        std::process::exit(2);
    }
    Ok(())
}
