//! `ax savings` — estimated context-token savings from MCP graph queries.

use std::fs;
use std::path::Path;

use ax_usage::{
    import_agent_logs, query_savings_summary, record_session_model_tag, SavingsQuery, UsagePeriod,
};
use serde_json::{json, Value};

const HOOK_PS1: &str = include_str!("../../assets/cursor-hooks/ax-session-model.ps1");
const HOOK_SH: &str = include_str!("../../assets/cursor-hooks/ax-session-model.sh");

pub async fn run_summary(
    period: Option<String>,
    from: Option<String>,
    to: Option<String>,
    json: bool,
) -> Result<(), String> {
    let period = period
        .as_deref()
        .and_then(UsagePeriod::parse)
        .unwrap_or(UsagePeriod::MonthToDate);

    if period == UsagePeriod::Custom && from.is_none() {
        return Err("custom period requires --from YYYY-MM-DD (optional --to)".into());
    }

    let summary = query_savings_summary(&SavingsQuery { period, from, to }).await?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&summary).unwrap_or_default()
        );
        return Ok(());
    }

    println!("Context savings ({} → {})", summary.from, summary.to);
    println!("Database: {}", summary.db_path);
    println!();
    println!(
        "Tokens saved: {} ({} graph calls of {} MCP calls, {} file reads avoided)",
        format_num(summary.tokens_saved_est),
        format_num(summary.graph_calls),
        format_num(summary.mcp_calls),
        format_num(summary.counterfactual_files),
    );
    println!(
        "Cost saved: ${:.2} (input tokens at {} — ${}/M in)",
        summary.cost_saved_usd_est, summary.pricing.reference_model, summary.pricing.input_per_mtok,
    );
    println!(
        "Without ax (full files): ~{} tokens",
        format_num(summary.counterfactual_tokens_est),
    );
    println!(
        "With ax (graph response): ~{} tokens",
        format_num(summary.graph_response_tokens_est),
    );
    if summary.counterfactual_tokens_est > 0 {
        let pct = ((summary.counterfactual_tokens_est - summary.graph_response_tokens_est) as f64
            / summary.counterfactual_tokens_est as f64
            * 100.0)
            .round() as i64;
        println!("Net reduction: ~{}% fewer context tokens", pct.max(0));
    }
    println!(
        "All MCP response size: ~{} tokens",
        format_num(summary.response_tokens_est),
    );
    if summary.failed_calls > 0 {
        println!(
            "Failed calls (excluded from savings): {}",
            format_num(summary.failed_calls)
        );
    }
    if summary.assumptions.exact_tokenizer {
        println!(
            "Measurement: o200k BPE tokenizer ({} of {} counterfactual files measured exactly)",
            format_num(summary.counterfactual_exact_files),
            format_num(summary.counterfactual_files),
        );
    } else {
        println!(
            "Measurement: heuristic fallback ({} chars/token, {} tokens/line, {} tokens/file)",
            summary.assumptions.chars_per_token,
            summary.assumptions.tokens_per_line,
            summary.assumptions.avg_file_tokens,
        );
    }
    println!(
        "Pricing: {} (${}/M in, ${}/M out) — override in {}",
        summary.pricing.reference_model,
        summary.pricing.input_per_mtok,
        summary.pricing.output_per_mtok,
        summary.pricing.config_path,
    );

    if summary.by_tool.is_empty() {
        println!();
        println!("No MCP calls recorded in this period.");
        println!(
            "Use ax with MCP enabled — savings are logged on each ax_explore / graph tool call."
        );
    } else {
        println!();
        println!("By tool:");
        for row in &summary.by_tool {
            println!(
                "  {:<16} {:>6} calls  {:>12} saved  {:>6} files",
                row.tool,
                format_num(row.calls),
                format_num(row.tokens_saved_est),
                format_num(row.counterfactual_files),
            );
        }
    }

    if !summary.by_model.is_empty() {
        println!();
        println!("By model (imported sessions):");
        for row in &summary.by_model {
            println!(
                "  {:<28} {:>3} sessions  {:>12} saved  {:>10} session cost",
                row.model,
                format_num(row.sessions),
                format_num(row.tokens_saved_est),
                row.session_cost_usd_est,
            );
        }
    }

    if !summary.agent_sessions.is_empty() {
        println!();
        println!("Imported agent sessions (sample):");
        for s in summary.agent_sessions.iter().take(10) {
            let tokens = s
                .session_input_tokens
                .map(format_num)
                .unwrap_or_else(|| "n/a".into());
            let cost = s
                .session_cost_usd_est
                .map(|c| format!(" ~${c:.2}"))
                .unwrap_or_default();
            let model = s.model.as_deref().unwrap_or("");
            println!(
                "  {} {} — read:{} grep:{} ax:{} (input tokens: {}{}) {}",
                s.agent,
                &s.session_id[..s.session_id.len().min(8)],
                s.read_calls,
                s.grep_calls,
                s.ax_calls,
                tokens,
                cost,
                model,
            );
        }
        println!("Run `ax savings import --all` to refresh from local agent logs.");
    }

    Ok(())
}

pub async fn run_tag_session(
    agent: String,
    session_id: String,
    model: String,
) -> Result<(), String> {
    record_session_model_tag(&agent, &session_id, &model).await?;
    println!("Tagged session {session_id} as {model} ({agent})");
    Ok(())
}

pub fn run_hook_install() -> Result<(), String> {
    let cursor_dir = ax_utils::paths::home_dir()
        .map(|h| h.join(".cursor"))
        .ok_or("could not resolve home directory")?;
    let script_path = install_cursor_hook(&cursor_dir)?;

    println!("Installed Cursor sessionStart and beforeSubmitPrompt hooks:");
    println!("  {}", script_path.display());
    println!("  {}", cursor_dir.join("hooks.json").display());
    println!("Start a new Composer chat to tag the model, then run `ax savings import --all`.");
    Ok(())
}

/// Writes the session-model hook script under `cursor_dir/hooks` and
/// registers it in `cursor_dir/hooks.json`. Returns the script path.
fn install_cursor_hook(cursor_dir: &Path) -> Result<std::path::PathBuf, String> {
    let hooks_dir = cursor_dir.join("hooks");
    fs::create_dir_all(&hooks_dir).map_err(|e| e.to_string())?;

    let is_windows = cfg!(windows);
    let script_name = if is_windows {
        "ax-session-model.ps1"
    } else {
        "ax-session-model.sh"
    };
    let script_body = if is_windows { HOOK_PS1 } else { HOOK_SH };
    let script_path = hooks_dir.join(script_name);
    write_utf8_no_bom(&script_path, script_body)?;
    // hooks.json runs the script directly (`./hooks/<name>`), so it must be
    // executable; fs::write leaves it at 0644 under the default umask.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("{}: {e}", script_path.display()))?;
    }

    merge_hooks_json(&cursor_dir.join("hooks.json"), script_name)?;
    Ok(script_path)
}

fn write_utf8_no_bom(path: &Path, content: &str) -> Result<(), String> {
    fs::write(path, content.as_bytes()).map_err(|e| e.to_string())
}

fn merge_hooks_json(path: &Path, script_name: &str) -> Result<(), String> {
    let command = format!("./hooks/{script_name}");
    let entry = json!({ "command": command });

    // A file we cannot parse (JSONC, a trailing comma, a half-written file)
    // is left untouched: rewriting it would drop the user's own hooks.
    let mut root: Value = if path.exists() {
        let raw = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_str(&raw).map_err(|e| {
            format!(
                "{} is not valid JSON ({e}); not modified. Fix it or add the ax hooks by hand.",
                path.display()
            )
        })?
    } else {
        json!({ "version": 1, "hooks": {} })
    };

    let not_object = || {
        format!(
            "{} does not have the expected {{\"version\", \"hooks\": {{...}}}} shape; not modified.",
            path.display()
        )
    };
    let obj = root.as_object_mut().ok_or_else(not_object)?;
    obj.entry("version").or_insert_with(|| json!(1));
    let hooks = obj
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(not_object)?;

    let session_start = hooks
        .entry("sessionStart")
        .or_insert_with(|| Value::Array(vec![]));
    let arr = session_start
        .as_array_mut()
        .ok_or("hooks.sessionStart must be an array")?;

    if !arr
        .iter()
        .any(|item| hook_points_to_ax_session_model(item, &command))
    {
        arr.push(entry.clone());
    }

    let before_submit = hooks
        .entry("beforeSubmitPrompt")
        .or_insert_with(|| Value::Array(vec![]));
    let before_arr = before_submit
        .as_array_mut()
        .ok_or("hooks.beforeSubmitPrompt must be an array")?;
    if !before_arr
        .iter()
        .any(|item| hook_points_to_ax_session_model(item, &command))
    {
        before_arr.push(entry);
    }

    let pretty = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    // A symlinked hooks.json (a dotfiles setup) is written through to its
    // target, so the rename does not replace the link with a regular file.
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    ax_utils::atomic_write(&target, (pretty + "\n").as_bytes())
        .map_err(|e| format!("{}: {e}", path.display()))
}

fn hook_points_to_ax_session_model(item: &Value, command: &str) -> bool {
    item.get("command")
        .and_then(|v| v.as_str())
        .is_some_and(|c| c == command || c.contains("ax-session-model"))
}

pub async fn run_import(claude: bool, cursor: bool, all: bool) -> Result<(), String> {
    let do_claude = all || claude;
    let do_cursor = all || cursor;
    if !do_claude && !do_cursor {
        return Err("specify --claude, --cursor, or --all".into());
    }
    let result = import_agent_logs(do_claude, do_cursor).await?;
    if result.cursor_state_enriched > 0 {
        println!(
            "Imported {} Claude session(s), {} Cursor session(s), {} enriched from Cursor state.vscdb ({} skipped)",
            result.claude_sessions, result.cursor_sessions, result.cursor_state_enriched, result.skipped
        );
    } else {
        println!(
            "Imported {} Claude session(s), {} Cursor session(s) ({} skipped)",
            result.claude_sessions, result.cursor_sessions, result.skipped
        );
    }
    Ok(())
}

fn format_num(n: i64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}

#[cfg(test)]
mod hook_install_tests {
    use super::*;

    fn temp_cursor_dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ax-savings-hook-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[cfg(unix)]
    #[test]
    fn hook_script_is_executable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_cursor_dir("exec");
        let script = install_cursor_hook(&dir).unwrap();
        let mode = fs::metadata(&script).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o111,
            0o111,
            "script mode {mode:o} is not executable"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_hooks_json_keeps_its_link() {
        let dir = temp_cursor_dir("symlink");
        let dotfiles = dir.join("dotfiles");
        fs::create_dir_all(&dotfiles).unwrap();
        let target = dotfiles.join("hooks.json");
        fs::write(
            &target,
            "{\"version\":1,\"hooks\":{\"preToolUse\":[{\"command\":\"./hooks/mine.sh\"}]}}",
        )
        .unwrap();
        let link = dir.join("hooks.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        install_cursor_hook(&dir).unwrap();

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink(),
            "hooks.json is still a symlink"
        );
        let merged: Value = serde_json::from_str(&fs::read_to_string(&target).unwrap()).unwrap();
        assert_eq!(
            merged["hooks"]["preToolUse"][0]["command"],
            "./hooks/mine.sh"
        );
        assert_eq!(
            merged["hooks"]["sessionStart"][0]["command"],
            "./hooks/ax-session-model.sh"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unparsable_hooks_json_is_left_untouched() {
        let dir = temp_cursor_dir("invalid");
        let hooks = dir.join("hooks.json");
        let original = "{\n  // mine\n  \"version\": 1,\n  \"hooks\": { \"preToolUse\": [{ \"command\": \"./hooks/mine.sh\" }], },\n}\n";
        fs::write(&hooks, original).unwrap();
        let err = install_cursor_hook(&dir).unwrap_err();
        assert!(err.contains("hooks.json"), "error names the file: {err}");
        assert_eq!(fs::read_to_string(&hooks).unwrap(), original);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn non_object_hooks_json_is_refused_without_panic() {
        let dir = temp_cursor_dir("array");
        let hooks = dir.join("hooks.json");
        fs::write(&hooks, "[1, 2]").unwrap();
        assert!(install_cursor_hook(&dir).is_err());
        assert_eq!(fs::read_to_string(&hooks).unwrap(), "[1, 2]");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn foreign_hooks_survive_and_install_is_idempotent() {
        let dir = temp_cursor_dir("merge");
        let hooks = dir.join("hooks.json");
        fs::write(
            &hooks,
            r#"{"version":1,"hooks":{"sessionStart":[{"command":"./hooks/mine.sh"}],"preToolUse":[{"command":"x"}]}}"#,
        )
        .unwrap();
        install_cursor_hook(&dir).unwrap();
        let first = fs::read_to_string(&hooks).unwrap();
        install_cursor_hook(&dir).unwrap();
        assert_eq!(
            fs::read_to_string(&hooks).unwrap(),
            first,
            "second install changes nothing"
        );
        let v: Value = serde_json::from_str(&first).unwrap();
        let start = v["hooks"]["sessionStart"].as_array().unwrap();
        assert_eq!(start.len(), 2);
        assert_eq!(start[0]["command"], "./hooks/mine.sh");
        assert_eq!(v["hooks"]["preToolUse"][0]["command"], "x");
        assert_eq!(
            v["hooks"]["beforeSubmitPrompt"].as_array().unwrap().len(),
            1
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
