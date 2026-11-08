//! CLI-only install: rules, skills, and agent instructions — no MCP, plugins, or hooks.

use std::fs;
use std::path::{Path, PathBuf};

use crate::report::{FileAction, InstallSummary, TargetReport};

const COPILOT_INSTRUCTIONS: &str = include_str!("templates/cli_only_copilot_instructions.md");
const COPILOT_SKILL: &str = include_str!("templates/cli_only_copilot_skill.md");
const CURSOR_RULE: &str = include_str!("templates/cli_only_cursor_rule.mdc");
const AGENTS_SKILL: &str = include_str!("templates/cli_only_agents_skill.md");
const AGENTS_SNIPPET: &str = include_str!("templates/cli_only_agents_snippet.md");

/// Install CLI-only agent integration for the requested targets (default: vscode + cursor).
pub fn install_cli_only(
    project_root: &Path,
    selected: &[String],
) -> Result<InstallSummary, String> {
    let targets: Vec<&str> = if selected.is_empty() {
        vec!["vscode", "cursor"]
    } else {
        let mut out = Vec::new();
        for s in selected {
            let id = s.trim().to_ascii_lowercase();
            if id.is_empty() {
                continue;
            }
            out.push(match id.as_str() {
                "vscode" | "copilot" => "vscode",
                "cursor" => "cursor",
                _ => "other",
            });
        }
        out
    };

    let mut reports = Vec::new();
    for target in targets {
        reports.push(install_one(project_root, target)?);
    }
    reports.push(install_shared(project_root)?);
    Ok(InstallSummary { reports })
}

fn install_one(project_root: &Path, target: &str) -> Result<TargetReport, String> {
    let mut report = match target {
        "vscode" => TargetReport::new("vscode", "VS Code / GitHub Copilot"),
        "cursor" => TargetReport::new("cursor", "Cursor"),
        _ => TargetReport::new("other", "Other"),
    };
    report.note("CLI-only mode: no MCP config, plugin, daemon, or IDE hooks installed.");
    match target {
        "vscode" => {
            write_file(
                &mut report,
                project_root.join(".github").join("copilot-instructions.md"),
                COPILOT_INSTRUCTIONS,
            )?;
            write_file(
                &mut report,
                project_root
                    .join(".github")
                    .join("skills")
                    .join("ax-cli")
                    .join("SKILL.md"),
                COPILOT_SKILL,
            )?;
        }
        "cursor" => {
            write_file(
                &mut report,
                project_root.join(".cursor").join("rules").join("ax-cli.mdc"),
                CURSOR_RULE,
            )?;
        }
        _ => {
            report.note("Unknown CLI-only target — shared AGENTS skill still installed.");
        }
    }
    Ok(report)
}

fn install_shared(project_root: &Path) -> Result<TargetReport, String> {
    let mut report = TargetReport::new("shared", "Shared agent instructions");
    write_file(
        &mut report,
        project_root
            .join(".agents")
            .join("skills")
            .join("ax-cli")
            .join("SKILL.md"),
        AGENTS_SKILL,
    )?;
    upsert_agents_md(project_root, &mut report)?;
    Ok(report)
}

fn write_file(report: &mut TargetReport, path: PathBuf, body: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let existed = path.exists();
    fs::write(&path, body.as_bytes()).map_err(|e| e.to_string())?;
    report.push_file(
        path,
        if existed {
            FileAction::Updated
        } else {
            FileAction::Created
        },
    );
    Ok(())
}

fn upsert_agents_md(project_root: &Path, report: &mut TargetReport) -> Result<(), String> {
    let path = project_root.join("AGENTS.md");
    let marker_start = "<!-- AX_CLI_START -->";
    let marker_end = "<!-- AX_CLI_END -->";
    let block = format!("{marker_start}\n{AGENTS_SNIPPET}\n{marker_end}\n");
    if path.exists() {
        let mut content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        if let (Some(s), Some(e)) = (content.find(marker_start), content.find(marker_end)) {
            let end = e + marker_end.len();
            content.replace_range(s..end, block.trim_end());
            if !content.ends_with('\n') {
                content.push('\n');
            }
            fs::write(&path, &content).map_err(|e| e.to_string())?;
            report.push_file(path, FileAction::Updated);
        } else {
            content.push('\n');
            content.push_str(&block);
            fs::write(&path, &content).map_err(|e| e.to_string())?;
            report.push_file(path, FileAction::Updated);
        }
    } else {
        fs::write(&path, &block).map_err(|e| e.to_string())?;
        report.push_file(path, FileAction::Created);
    }
    Ok(())
}
