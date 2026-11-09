//! CLI-only install: rules, skills, and agent instructions — no MCP, plugins, or hooks.

use std::fs;
use std::path::{Path, PathBuf};

use crate::report::{FileAction, InstallSummary, TargetReport};
use crate::targets::display_name;

const COPILOT_INSTRUCTIONS: &str = include_str!("templates/cli_only_copilot_instructions.md");
const COPILOT_SKILL: &str = include_str!("templates/cli_only_copilot_skill.md");
const CURSOR_RULE: &str = include_str!("templates/cli_only_cursor_rule.mdc");
const AGENTS_SKILL: &str = include_str!("templates/cli_only_agents_skill.md");
const AGENTS_SNIPPET: &str = include_str!("templates/cli_only_agents_snippet.md");
const WINDSURF_SNIPPET: &str = include_str!("templates/cli_only_windsurf_snippet.md");
const GEMINI_SNIPPET: &str = include_str!("templates/cli_only_gemini_snippet.md");
const CONTINUE_RULE: &str = include_str!("templates/cli_only_continue_rule.md");
const CLAUDE_RULE: &str = include_str!("templates/cli_only_claude_rule.md");
const CLINE_RULES: &str = include_str!("templates/cli_only_cline_rules.md");

const AX_CLI_START: &str = "<!-- AX_CLI_START -->";
const AX_CLI_END: &str = "<!-- AX_CLI_END -->";

/// Distinct instruction surfaces installed when `--target` is omitted.
const DEFAULT_CLI_ONLY_TARGETS: &[&str] = &[
    "vscode", "cursor", "windsurf", "continue", "claude", "gemini", "cline",
];

/// Targets that only get the shared `AGENTS.md` + `.agents/skills/ax-cli` skill.
const SHARED_ONLY_TARGETS: &[&str] = &[
    "zed",
    "jetbrains",
    "kiro",
    "codex",
    "hermes",
    "opencode",
];

/// Install CLI-only agent integration for the requested targets.
///
/// Empty `selected` installs every distinct IDE instruction surface plus shared files.
/// Never writes MCP server configs, plugins, daemons, or IDE hooks.
pub fn install_cli_only(
    project_root: &Path,
    selected: &[String],
) -> Result<InstallSummary, String> {
    let targets: Vec<String> = if selected.is_empty() {
        DEFAULT_CLI_ONLY_TARGETS
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    } else {
        let mut out = Vec::new();
        for s in selected {
            let id = normalize_target(s);
            if id.is_empty() {
                continue;
            }
            if !out.iter().any(|t| t == &id) {
                out.push(id);
            }
        }
        out
    };

    let mut reports = Vec::new();
    for target in &targets {
        reports.push(install_one(project_root, target)?);
    }
    reports.push(install_shared(project_root)?);
    Ok(InstallSummary { reports })
}

fn normalize_target(raw: &str) -> String {
    let id = raw.trim().to_ascii_lowercase();
    match id.as_str() {
        "vscode" | "copilot" => "vscode".into(),
        "antigravity" => "cline".into(),
        other => other.into(),
    }
}

fn install_one(project_root: &Path, target: &str) -> Result<TargetReport, String> {
    let (id, label): (&'static str, &'static str) = match target {
        "vscode" => ("vscode", display_name("vscode")),
        "cursor" => ("cursor", display_name("cursor")),
        "windsurf" => ("windsurf", display_name("windsurf")),
        "continue" => ("continue", display_name("continue")),
        "claude" => ("claude", display_name("claude")),
        "gemini" => ("gemini", display_name("gemini")),
        "cline" => ("cline", "Cline / Antigravity"),
        "zed" => ("zed", display_name("zed")),
        "jetbrains" => ("jetbrains", display_name("jetbrains")),
        "kiro" => ("kiro", display_name("kiro")),
        "codex" => ("codex", display_name("codex")),
        "hermes" => ("hermes", display_name("hermes")),
        "opencode" => ("opencode", display_name("opencode")),
        _ => ("other", "Other"),
    };
    let mut report = TargetReport::new(id, label);
    report.note("CLI-only mode: no MCP config, plugin, daemon, or IDE hooks installed.");

    match id {
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
        "windsurf" => {
            upsert_marked_file(
                project_root.join(".windsurfrules"),
                WINDSURF_SNIPPET,
                &mut report,
            )?;
        }
        "continue" => {
            write_file(
                &mut report,
                project_root
                    .join(".continue")
                    .join("rules")
                    .join("ax-cli.md"),
                CONTINUE_RULE,
            )?;
            // Explicitly do not write `.continue/mcpServers/ax.json`.
            report.note("Continue MCP server file not written (CLI-only).");
        }
        "claude" => {
            write_file(
                &mut report,
                project_root
                    .join(".claude")
                    .join("rules")
                    .join("ax-cli.md"),
                CLAUDE_RULE,
            )?;
            upsert_marked_file(
                project_root.join(".claude").join("CLAUDE.md"),
                &format!("{AX_CLI_START}\n{AGENTS_SNIPPET}\n{AX_CLI_END}\n"),
                &mut report,
            )?;
        }
        "gemini" => {
            upsert_marked_file(
                project_root.join("GEMINI.md"),
                GEMINI_SNIPPET,
                &mut report,
            )?;
        }
        "cline" => {
            write_file(
                &mut report,
                project_root.join(".clinerules"),
                CLINE_RULES,
            )?;
        }
        t if SHARED_ONLY_TARGETS.contains(&t) => {
            report.note(format!(
                "{t} uses shared AGENTS.md + .agents/skills/ax-cli (no IDE-specific MCP config)."
            ));
        }
        _ => {
            report.note(format!(
                "Unknown CLI-only target '{target}' — shared AGENTS skill still installed."
            ));
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
    upsert_marked_file(
        project_root.join("AGENTS.md"),
        &format!("{AX_CLI_START}\n{AGENTS_SNIPPET}\n{AX_CLI_END}\n"),
        &mut report,
    )?;
    Ok(report)
}

fn write_file(report: &mut TargetReport, path: PathBuf, body: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let action = if path.exists() {
        FileAction::Updated
    } else {
        FileAction::Created
    };
    let content = if body.ends_with('\n') {
        body.to_string()
    } else {
        format!("{body}\n")
    };
    fs::write(&path, content).map_err(|e| e.to_string())?;
    report.push_file(path, action);
    Ok(())
}

fn upsert_marked_file(
    path: PathBuf,
    block: &str,
    report: &mut TargetReport,
) -> Result<(), String> {
    let block = if block.ends_with('\n') {
        block.to_string()
    } else {
        format!("{block}\n")
    };
    if path.exists() {
        let mut content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        if let (Some(s), Some(e)) = (content.find(AX_CLI_START), content.find(AX_CLI_END)) {
            let end = e + AX_CLI_END.len();
            content.replace_range(s..end, block.trim_end());
            if !content.ends_with('\n') {
                content.push('\n');
            }
            fs::write(&path, &content).map_err(|e| e.to_string())?;
            report.push_file(path, FileAction::Updated);
        } else {
            if !content.ends_with('\n') {
                content.push('\n');
            }
            content.push('\n');
            content.push_str(&block);
            fs::write(&path, &content).map_err(|e| e.to_string())?;
            report.push_file(path, FileAction::Updated);
        }
    } else {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&path, &block).map_err(|e| e.to_string())?;
        report.push_file(path, FileAction::Created);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn assert_cli_content(path: &Path) {
        let body = fs::read_to_string(path).unwrap_or_else(|_| String::new());
        assert!(
            !body.is_empty(),
            "expected non-empty {}",
            path.display()
        );
        assert!(
            body.contains("ax turn")
                || body.contains("ax preflight")
                || body.contains("ax explore")
                || body.contains("ax session"),
            "{} missing CLI workflow keywords",
            path.display()
        );
    }

    #[test]
    fn default_cli_only_covers_all_ide_instruction_surfaces() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let summary = install_cli_only(root, &[]).unwrap();
        assert!(summary.reports.len() > DEFAULT_CLI_ONLY_TARGETS.len());

        for rel in [
            ".cursor/rules/ax-cli.mdc",
            ".github/copilot-instructions.md",
            ".github/skills/ax-cli/SKILL.md",
            ".agents/skills/ax-cli/SKILL.md",
            "AGENTS.md",
            ".windsurfrules",
            ".continue/rules/ax-cli.md",
            ".claude/rules/ax-cli.md",
            ".claude/CLAUDE.md",
            "GEMINI.md",
            ".clinerules",
        ] {
            let path = root.join(rel);
            assert!(path.is_file(), "missing {rel}");
            assert_cli_content(&path);
        }

        // Must not write MCP configs.
        assert!(!root.join(".continue").join("mcpServers").join("ax.json").exists());
        assert!(!root.join(".cursor").join("mcp.json").exists());
        assert!(!root.join(".vscode").join("mcp.json").exists());
        assert!(!root.join(".mcp.json").exists());
    }

    #[test]
    fn windsurf_target_installs_without_cursor_rule() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        install_cli_only(root, &["windsurf".into()]).unwrap();
        assert!(root.join(".windsurfrules").is_file());
        assert!(root.join(".agents").join("skills").join("ax-cli").join("SKILL.md").is_file());
        assert!(root.join("AGENTS.md").is_file());
        assert!(!root.join(".cursor").join("rules").join("ax-cli.mdc").exists());
        assert!(!root.join(".continue").join("mcpServers").join("ax.json").exists());
    }

    #[test]
    fn zed_uses_shared_agents_only() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let summary = install_cli_only(root, &["zed".into()]).unwrap();
        assert!(root.join("AGENTS.md").is_file());
        assert!(root.join(".agents").join("skills").join("ax-cli").join("SKILL.md").is_file());
        assert!(!root.join(".cursor").join("rules").join("ax-cli.mdc").exists());
        let zed = summary
            .reports
            .iter()
            .find(|r| r.id == "zed")
            .expect("zed report");
        assert!(
            zed.notes.iter().any(|n| n.contains("shared AGENTS.md")),
            "expected shared note, got {:?}",
            zed.notes
        );
    }

    #[test]
    fn antigravity_aliases_to_cline_rules() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        install_cli_only(root, &["antigravity".into()]).unwrap();
        assert!(root.join(".clinerules").is_file());
        assert_cli_content(&root.join(".clinerules"));
    }

    #[test]
    fn continue_cli_only_never_writes_mcp_server() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        install_cli_only(root, &["continue".into()]).unwrap();
        assert!(root.join(".continue").join("rules").join("ax-cli.md").is_file());
        assert!(!root.join(".continue").join("mcpServers").join("ax.json").exists());
    }

    #[test]
    fn upsert_agents_md_is_idempotent() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        install_cli_only(root, &["cursor".into()]).unwrap();
        let first = fs::read_to_string(root.join("AGENTS.md")).unwrap();
        install_cli_only(root, &["cursor".into()]).unwrap();
        let second = fs::read_to_string(root.join("AGENTS.md")).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.matches(AX_CLI_START).count(), 1);
    }
}
