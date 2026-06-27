//! Hidden `ax prompt-hook` — Claude UserPromptSubmit stdin JSON hook.

use std::io::{self, IsTerminal, Read};
use std::path::PathBuf;

use ax_context::{
    extract_code_tokens, format_explore_text, has_structural_keyword, plan_frontload_full,
};
use ax_types::ExploreOptions;

use crate::commands::resolve_path;

const MAX_INJECT_CHARS: usize = 16000;

pub async fn run() -> Result<(), String> {
    if std::env::var("AX_NO_PROMPT_HOOK").ok().as_deref() == Some("1")
        || std::env::var("AX_PROMPT_HOOK").ok().as_deref() == Some("0")
    {
        return Ok(());
    }
    if io::stdin().is_terminal() {
        return Ok(());
    }

    let mut raw = String::new();
    if io::stdin().read_to_string(&mut raw).is_err() {
        return Ok(());
    }

    let input: serde_json::Value = serde_json::from_str(&raw).unwrap_or(serde_json::Value::Null);
    let prompt = input
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let cwd = input
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .unwrap_or_else(|| resolve_path(None));

    let keyworded = has_structural_keyword(&prompt);
    let code_tokens = if keyworded {
        vec![]
    } else {
        extract_code_tokens(&prompt)
    };
    if !keyworded && code_tokens.is_empty() {
        return Ok(());
    }

    let plan = plan_frontload_full(&cwd, &prompt);
    if plan.explore_root.is_none() && plan.nudge_projects.is_empty() {
        return Ok(());
    }

    let nudge = |projects: &[PathBuf], lead: &str| -> String {
        if projects.is_empty() {
            return String::new();
        }
        let lines: Vec<String> = projects
            .iter()
            .map(|p| format!("  - projectPath: \"{}\"", p.display()))
            .collect();
        format!("{}\n{}\n", lead, lines.join("\n"))
    };

    if let Some(root) = &plan.explore_root {
        let ax = ax_core::Ax::open(root).await.map_err(|e| e.to_string())?;
        if !keyworded {
            let mut any = false;
            for token in &code_tokens {
                let nodes = ax
                    .queries()
                    .get_nodes_by_name(token)
                    .await
                    .unwrap_or_default();
                if !nodes.is_empty() {
                    any = true;
                    break;
                }
            }
            if !any {
                return Ok(());
            }
        }
        let result = ax
            .explore(&prompt, ExploreOptions::default())
            .await
            .map_err(|e| e.to_string())?;
        let text = format_explore_text(&result);
        if text.trim().is_empty() {
            return Ok(());
        }
        let body = if text.len() > MAX_INJECT_CHARS {
            format!(
                "{}\n...(truncated; call ax_explore for the rest)",
                &text[..MAX_INJECT_CHARS]
            )
        } else {
            text
        };
        let more = if plan.via_sub_scan {
            format!("call ax_explore with projectPath: \"{}\" for more", root.display())
        } else {
            "call ax_explore for more".to_string()
        };
        let others = nudge(
            &plan.nudge_projects,
            "Other indexed projects in this workspace — pass projectPath to query them:",
        );
        print!(
            "<ax_context note=\"Structural context from ax for this prompt — treat returned source as already read; {more}.\">\n{body}{others}</ax_context>\n",
        );
    } else {
        let body = nudge(
            &plan.nudge_projects,
            "This workspace's ax indexes live in sub-projects. To use ax, call ax_explore with the projectPath of the relevant one:",
        );
        print!(
            "<ax_context note=\"ax is available for this workspace's indexed sub-projects — query one by passing projectPath to ax_explore.\">\n{body}</ax_context>\n",
        );
    }
    Ok(())
}
