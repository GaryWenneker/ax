//! IDE-specific bootstrap — written on `ax init` into each agent's native
//! instructions surface. Team policy stays in `.ax/policy/` (MCP); only the
//! `ax_preflight` entry point is seeded into IDE rules/instructions files.

use std::path::{Path, PathBuf};

use crate::seed::{InstructionCheck, SyncResult};

const AX_SECTION_START: &str = "<!-- AX_START -->";
const AX_SECTION_END: &str = "<!-- AX_END -->";

const CURSOR_RULE_FILE: &str = "ax.mdc";
const MCP_CALLMCP_SHAPE_FILE: &str = "mcp-callmcp-shape.mdc";
const LEGACY_CURSOR_RULE_FILE: &str = "ax-agent-workflow.mdc";
const CURSOR_RULE_BODY: &str = include_str!("../templates/ide/cursor/ax.mdc");
const MCP_CALLMCP_SHAPE_BODY: &str = include_str!("../templates/ide/cursor/mcp-callmcp-shape.mdc");
const CLAUDE_RULE_FILE: &str = "ax.md";
const CLAUDE_RULE_BODY: &str = include_str!("../templates/ide/claude/ax.md");
const CONTINUE_RULE_FILE: &str = "ax.md";
const CONTINUE_RULE_BODY: &str = include_str!("../templates/ide/continue/ax.md");
const CONTINUE_MCP_FILE: &str = "ax.json";
const CONTINUE_MCP_BODY: &str = include_str!("../templates/ide/continue/mcp-ax.json");

const CLAUDE_INSTRUCTIONS_BLOCK: &str = r#"<!-- AX_START -->
## ax

Call `ax_preflight` exactly once per turn **before all other work** whenever the ax MCP server is available. Full workflow: see `.claude/rules/ax.md`.

**Directive capture:** When the user states a durable rule — `je moet`, `altijd`, `nooit`, `voortaan`, `always`, `never`, `you must`, `@rule` — persist it. `ax_preflight` returns `directiveDetected` + a ready `captureProposal`; ask the questions it lists, then call `ax_policy_capture(action="save", rule)` after the user confirms. This works even if the project has no policy yet (the first save bootstraps it). Never silently ignore such a directive.

**Capability discovery:** ax is actively developed. Do not rely on cached knowledge of ax features — `ax_preflight` returns the latest capabilities, rules, and skills each call. Use any new tools or rules it returns.

**Version freshness:** Call `ax_status` at session start. If the index is stale or a newer version exists, warn the user and suggest `ax upgrade` or re-index.

MCP unreachable → report `ax MCP unreachable: [error]`, state `Mode: DEGRADED`; do not proceed silently.
<!-- AX_END -->"#;

const AGENTS_INSTRUCTIONS_BLOCK: &str = r#"<!-- AX_START -->
## ax

Call `ax_preflight` exactly once per turn **before all other work** whenever the `user-ax` MCP server is available. Team policy arrives via MCP inject — do not Read `.ax/policy/` files when ax MCP tools are available.

**Inject fallback:** If preflight lacks `<ax_policy>` (empty inject/rules), call `ax_skill("startup")` once.

**Explore before Grep/Read:** For structural code questions, call `ax_explore` (or graph tools) before broad Grep/Read.

**Directive capture:** When the user states a durable rule — `je moet`, `altijd`, `nooit`, `voortaan`, `always`, `never`, `you must`, `@rule` — persist it. `ax_preflight` returns `directiveDetected` + a ready `captureProposal`; ask the questions it lists, then call `ax_policy_capture(action="save", rule)` after the user confirms. Works even if the project has no policy yet (the first save bootstraps it). Never silently ignore such a directive.

**Capability discovery:** ax is actively developed. Do not rely on cached knowledge of ax features — `ax_preflight` returns the latest capabilities, rules, and skills each call. Use any new tools or rules it returns.

**Version freshness:** Call `ax_status` at session start. If the index is stale or a newer version exists, warn the user and suggest `ax upgrade` or re-index.

**Tool reference:** `ax_explore`/`ax_search`/`ax_node` for code structure, `ax_impact`/`ax_callers`/`ax_callees` for change impact, `ax_affected` for test coverage, `ax_insights`/`ax_report` for whole-graph architecture, `ax_guard` before writes when CRITICAL rules exist, `ax_diagnostics` for IDE/linter correlation, `ax_policy_capture` for durable rules, `ax_context` for task context, **`ax_sync`** / **`ax_index({force:true})`** for re-index, **`ax_lsp`** for LSP status/enrich, **`ax_ship`** for quality-gate evaluate/ci, **`ax_policy_index`** to refresh rules from disk, **`ax_remember`/`ax_recall`** for memory. Prefer these MCP tools over shelling out to the CLI when MCP is connected.

Run preflight exactly once per turn. MCP unreachable → report `ax MCP unreachable: [error]`, state `Mode: DEGRADED`; do not proceed silently.
<!-- AX_END -->"#;

const GEMINI_INSTRUCTIONS_BLOCK: &str = AGENTS_INSTRUCTIONS_BLOCK;
const COPILOT_INSTRUCTIONS_BLOCK: &str = AGENTS_INSTRUCTIONS_BLOCK;
const WINDSURF_INSTRUCTIONS_BLOCK: &str = AGENTS_INSTRUCTIONS_BLOCK;
const CLINE_INSTRUCTIONS_BLOCK: &str = AGENTS_INSTRUCTIONS_BLOCK;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct IdeSeedResult {
    pub created: Vec<String>,
    pub updated: Vec<String>,
    pub skipped: Vec<String>,
}

impl IdeSeedResult {
    fn record_created(&mut self, rel: impl Into<String>) {
        self.created.push(rel.into());
    }

    fn record_updated(&mut self, rel: impl Into<String>) {
        self.updated.push(rel.into());
    }

    fn record_skipped(&mut self, rel: impl Into<String>) {
        self.skipped.push(rel.into());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpsertAction {
    Created,
    Updated,
    Appended,
    Unchanged,
}

fn replace_or_append_marked_section(
    path: &Path,
    body: &str,
    start_marker: &str,
    end_marker: &str,
) -> std::io::Result<UpsertAction> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, format!("{body}\n"))?;
        return Ok(UpsertAction::Created);
    }

    let content = std::fs::read_to_string(path)?;
    let start_idx = content.find(start_marker);
    let end_idx = content.find(end_marker);

    if let (Some(start), Some(end)) = (start_idx, end_idx) {
        if end <= start {
            return append_marked_section(path, &content, body);
        }
        let existing_block = &content[start..end + end_marker.len()];
        if existing_block == body {
            return Ok(UpsertAction::Unchanged);
        }
        let before = &content[..start];
        let after = &content[end + end_marker.len()..];
        std::fs::write(path, format!("{before}{body}{after}"))?;
        return Ok(UpsertAction::Updated);
    }

    append_marked_section(path, &content, body)
}

fn append_marked_section(path: &Path, content: &str, body: &str) -> std::io::Result<UpsertAction> {
    let trimmed = content.trim_end();
    let sep = if trimmed.is_empty() { "" } else { "\n\n" };
    std::fs::write(path, format!("{trimmed}{sep}{body}\n"))?;
    Ok(UpsertAction::Appended)
}

fn record_upsert(result: &mut IdeSeedResult, rel: &str, action: UpsertAction) {
    match action {
        UpsertAction::Created => result.record_created(rel),
        UpsertAction::Updated | UpsertAction::Appended => result.record_updated(rel),
        UpsertAction::Unchanged => result.record_skipped(rel),
    }
}

fn cursor_rules_dir(project_root: &Path) -> PathBuf {
    project_root.join(".cursor").join("rules")
}

fn cursor_rule_path(project_root: &Path) -> PathBuf {
    cursor_rules_dir(project_root).join(CURSOR_RULE_FILE)
}

fn legacy_cursor_rule_path(project_root: &Path) -> PathBuf {
    cursor_rules_dir(project_root).join(LEGACY_CURSOR_RULE_FILE)
}

fn bootstrap_already_present(cursor_rules: &Path) -> bool {
    bootstrap_already_present_ext(cursor_rules, "mdc")
}

fn bootstrap_already_present_ext(rules_dir: &Path, ext: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(rules_dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some(ext) {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if stem == "ax" || stem == "ax-agent-workflow" || stem == "mcp-callmcp-shape" {
            continue;
        }
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        if crate::seed::verify_content(&content).is_empty() {
            return true;
        }
    }
    false
}

fn remove_legacy_cursor_rule(project_root: &Path, result: &mut IdeSeedResult) {
    let legacy = legacy_cursor_rule_path(project_root);
    if legacy.exists() {
        let _ = std::fs::remove_file(&legacy);
        result.record_updated(format!(
            ".cursor/rules/{LEGACY_CURSOR_RULE_FILE} (removed — replaced by {CURSOR_RULE_FILE})"
        ));
    }
}

fn cursor_bootstrap_stale(content: &str) -> bool {
    !crate::seed::verify_content(content).is_empty() || content.trim() != CURSOR_RULE_BODY.trim()
}

fn mcp_callmcp_shape_rule_path(project_root: &Path) -> PathBuf {
    cursor_rules_dir(project_root).join(MCP_CALLMCP_SHAPE_FILE)
}

fn mcp_callmcp_shape_stale(content: &str) -> bool {
    content.trim() != MCP_CALLMCP_SHAPE_BODY.trim()
}

fn seed_mcp_callmcp_shape_rule(project_root: &Path, result: &mut IdeSeedResult) -> std::io::Result<()> {
    let cursor_rules = cursor_rules_dir(project_root);
    std::fs::create_dir_all(&cursor_rules)?;

    let target = mcp_callmcp_shape_rule_path(project_root);
    let rel = format!(".cursor/rules/{MCP_CALLMCP_SHAPE_FILE}");

    if target.exists() {
        let content = std::fs::read_to_string(&target)?;
        if !mcp_callmcp_shape_stale(&content) {
            result.record_skipped(rel);
            return Ok(());
        }
        std::fs::write(&target, MCP_CALLMCP_SHAPE_BODY.as_bytes())?;
        result.record_updated(rel);
        return Ok(());
    }

    std::fs::write(&target, MCP_CALLMCP_SHAPE_BODY.as_bytes())?;
    result.record_created(rel);
    Ok(())
}

fn seed_cursor_rule(project_root: &Path, result: &mut IdeSeedResult) -> std::io::Result<()> {
    let cursor_rules = cursor_rules_dir(project_root);
    std::fs::create_dir_all(&cursor_rules)?;
    remove_legacy_cursor_rule(project_root, result);

    let target = cursor_rule_path(project_root);
    let rel = format!(".cursor/rules/{CURSOR_RULE_FILE}");

    if target.exists() {
        let content = std::fs::read_to_string(&target)?;
        if !cursor_bootstrap_stale(&content) {
            result.record_skipped(rel);
            return Ok(());
        }
        std::fs::write(&target, CURSOR_RULE_BODY.as_bytes())?;
        result.record_updated(rel);
        return Ok(());
    }

    if bootstrap_already_present(&cursor_rules) {
        result.record_skipped(format!(
            "{rel} (another .cursor/rules/*.mdc already contains ax_preflight bootstrap)"
        ));
        return Ok(());
    }

    std::fs::write(&target, CURSOR_RULE_BODY.as_bytes())?;
    result.record_created(rel);
    Ok(())
}

fn continue_rules_dir(project_root: &Path) -> PathBuf {
    project_root.join(".continue").join("rules")
}

fn continue_rule_path(project_root: &Path) -> PathBuf {
    continue_rules_dir(project_root).join(CONTINUE_RULE_FILE)
}

fn continue_mcp_dir(project_root: &Path) -> PathBuf {
    project_root.join(".continue").join("mcpServers")
}

fn continue_mcp_path(project_root: &Path) -> PathBuf {
    continue_mcp_dir(project_root).join(CONTINUE_MCP_FILE)
}

fn continue_bootstrap_stale(content: &str) -> bool {
    !crate::seed::verify_content(content).is_empty() || content.trim() != CONTINUE_RULE_BODY.trim()
}

fn continue_mcp_stale(content: &str) -> bool {
    content.trim() != CONTINUE_MCP_BODY.trim()
}

fn seed_continue_rule(project_root: &Path, result: &mut IdeSeedResult) -> std::io::Result<()> {
    let continue_rules = continue_rules_dir(project_root);
    std::fs::create_dir_all(&continue_rules)?;

    let target = continue_rule_path(project_root);
    let rel = format!(".continue/rules/{CONTINUE_RULE_FILE}");

    if target.exists() {
        let content = std::fs::read_to_string(&target)?;
        if !continue_bootstrap_stale(&content) {
            result.record_skipped(rel);
            return Ok(());
        }
        std::fs::write(&target, CONTINUE_RULE_BODY.as_bytes())?;
        result.record_updated(rel);
        return Ok(());
    }

    if bootstrap_already_present_ext(&continue_rules, "md") {
        result.record_skipped(format!(
            "{rel} (another .continue/rules/*.md already contains ax_preflight bootstrap)"
        ));
        return Ok(());
    }

    std::fs::write(&target, CONTINUE_RULE_BODY.as_bytes())?;
    result.record_created(rel);
    Ok(())
}

/// Project-scoped Continue MCP server so teammates on Continue pick up ax without a global install.
fn seed_continue_mcp(project_root: &Path, result: &mut IdeSeedResult) -> std::io::Result<()> {
    let dir = continue_mcp_dir(project_root);
    std::fs::create_dir_all(&dir)?;
    let target = continue_mcp_path(project_root);
    let rel = format!(".continue/mcpServers/{CONTINUE_MCP_FILE}");
    if target.exists() {
        let content = std::fs::read_to_string(&target)?;
        if !continue_mcp_stale(&content) {
            result.record_skipped(rel);
            return Ok(());
        }
        std::fs::write(&target, CONTINUE_MCP_BODY.as_bytes())?;
        result.record_updated(rel);
        return Ok(());
    }
    std::fs::write(&target, CONTINUE_MCP_BODY.as_bytes())?;
    result.record_created(rel);
    Ok(())
}

fn write_if_missing_or_stale(path: &Path, body: &str, rel: &str, result: &mut IdeSeedResult) -> std::io::Result<()> {
    if path.exists() {
        let content = std::fs::read_to_string(path)?;
        let stale = !crate::seed::verify_content(&content).is_empty()
            || content.trim() != body.trim();
        if !stale {
            result.record_skipped(rel);
            return Ok(());
        }
        std::fs::write(path, body.as_bytes())?;
        result.record_updated(rel);
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, body.as_bytes())?;
    result.record_created(rel);
    Ok(())
}

fn seed_claude_bootstrap(project_root: &Path, result: &mut IdeSeedResult) -> std::io::Result<()> {
    let rule_path = project_root.join(".claude").join("rules").join(CLAUDE_RULE_FILE);
    let rule_rel = format!(".claude/rules/{CLAUDE_RULE_FILE}");
    write_if_missing_or_stale(&rule_path, CLAUDE_RULE_BODY, &rule_rel, result)?;

    let instructions_path = project_root.join(".claude").join("CLAUDE.md");
    let instructions_rel = ".claude/CLAUDE.md";
    let action = replace_or_append_marked_section(
        &instructions_path,
        CLAUDE_INSTRUCTIONS_BLOCK,
        AX_SECTION_START,
        AX_SECTION_END,
    )?;
    record_upsert(result, instructions_rel, action);
    Ok(())
}

fn seed_agents_bootstrap(project_root: &Path, result: &mut IdeSeedResult) -> std::io::Result<()> {
    let path = project_root.join("AGENTS.md");
    let action = replace_or_append_marked_section(
        &path,
        AGENTS_INSTRUCTIONS_BLOCK,
        AX_SECTION_START,
        AX_SECTION_END,
    )?;
    record_upsert(result, "AGENTS.md", action);
    Ok(())
}

fn seed_gemini_bootstrap(project_root: &Path, result: &mut IdeSeedResult) -> std::io::Result<()> {
    let path = project_root.join("GEMINI.md");
    let action = replace_or_append_marked_section(
        &path,
        GEMINI_INSTRUCTIONS_BLOCK,
        AX_SECTION_START,
        AX_SECTION_END,
    )?;
    record_upsert(result, "GEMINI.md", action);
    Ok(())
}

fn seed_copilot_bootstrap(project_root: &Path, result: &mut IdeSeedResult) -> std::io::Result<()> {
    let path = project_root.join(".github").join("copilot-instructions.md");
    let action = replace_or_append_marked_section(
        &path,
        COPILOT_INSTRUCTIONS_BLOCK,
        AX_SECTION_START,
        AX_SECTION_END,
    )?;
    record_upsert(result, ".github/copilot-instructions.md", action);
    Ok(())
}

fn seed_windsurf_bootstrap(project_root: &Path, result: &mut IdeSeedResult) -> std::io::Result<()> {
    let path = project_root.join(".windsurfrules");
    let action = replace_or_append_marked_section(
        &path,
        WINDSURF_INSTRUCTIONS_BLOCK,
        AX_SECTION_START,
        AX_SECTION_END,
    )?;
    record_upsert(result, ".windsurfrules", action);
    Ok(())
}

fn seed_cline_bootstrap(project_root: &Path, result: &mut IdeSeedResult) -> std::io::Result<()> {
    let path = project_root.join(".clinerules");
    let action = replace_or_append_marked_section(
        &path,
        CLINE_INSTRUCTIONS_BLOCK,
        AX_SECTION_START,
        AX_SECTION_END,
    )?;
    record_upsert(result, ".clinerules", action);
    Ok(())
}

/// Ensure IDE bootstrap files exist (create or repair on init).
///
/// Seeds **all** agent surfaces (Cursor, Continue, Claude, …) so a teammate on a
/// different IDE still gets `ax_preflight` after pack import / `ax init`.
pub fn seed_ide_agent_workflow(project_root: &Path) -> std::io::Result<IdeSeedResult> {
    let mut result = IdeSeedResult::default();
    seed_cursor_rule(project_root, &mut result)?;
    seed_mcp_callmcp_shape_rule(project_root, &mut result)?;
    seed_continue_rule(project_root, &mut result)?;
    seed_continue_mcp(project_root, &mut result)?;
    seed_claude_bootstrap(project_root, &mut result)?;
    seed_agents_bootstrap(project_root, &mut result)?;
    seed_gemini_bootstrap(project_root, &mut result)?;
    seed_copilot_bootstrap(project_root, &mut result)?;
    seed_windsurf_bootstrap(project_root, &mut result)?;
    seed_cline_bootstrap(project_root, &mut result)?;
    Ok(result)
}

fn verify_marked_instructions(path: &Path, expected_block: &str) -> Vec<String> {
    if !path.exists() {
        return vec!["missing".into()];
    }
    let content = std::fs::read_to_string(path).unwrap_or_default();
    let Some(start) = content.find(AX_SECTION_START) else {
        return vec!["missing AX marker block".into()];
    };
    let Some(end) = content.find(AX_SECTION_END) else {
        return vec!["missing AX marker block".into()];
    };
    if end <= start {
        return vec!["invalid AX marker block".into()];
    }
    let block = &content[start..end + AX_SECTION_END.len()];
    if block == expected_block {
        return vec![];
    }
    crate::seed::verify_content(block)
}

fn verify_dedicated_file(path: &Path) -> Vec<String> {
    if !path.exists() {
        return vec!["missing".into()];
    }
    let content = std::fs::read_to_string(path).unwrap_or_default();
    crate::seed::verify_content(&content)
}

fn verify_cursor_bootstrap(project_root: &Path) -> InstructionCheck {
    let path = cursor_rule_path(project_root);
    let label = format!(".cursor/rules/{CURSOR_RULE_FILE}");
    let cursor_rules = cursor_rules_dir(project_root);
    if !path.exists() && bootstrap_already_present(&cursor_rules) {
        return InstructionCheck {
            label,
            path,
            ok: true,
            issues: vec![],
            optional: true,
        };
    }
    let issues = verify_dedicated_file(&path);
    if path.exists() {
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        if cursor_bootstrap_stale(&content) {
            return InstructionCheck {
                label,
                path,
                ok: false,
                issues: vec!["bootstrap content drifts from embedded template".into()],
                optional: false,
            };
        }
    }
    InstructionCheck {
        label,
        path,
        ok: issues.is_empty(),
        issues,
        optional: false,
    }
}

fn verify_continue_bootstrap(project_root: &Path) -> InstructionCheck {
    let path = continue_rule_path(project_root);
    let label = format!(".continue/rules/{CONTINUE_RULE_FILE}");
    let continue_rules = continue_rules_dir(project_root);
    if !path.exists() && bootstrap_already_present_ext(&continue_rules, "md") {
        return InstructionCheck {
            label,
            path,
            ok: true,
            issues: vec![],
            optional: true,
        };
    }
    let issues = verify_dedicated_file(&path);
    if path.exists() {
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        if continue_bootstrap_stale(&content) {
            return InstructionCheck {
                label,
                path,
                ok: false,
                issues: vec!["bootstrap content drifts from embedded template".into()],
                optional: false,
            };
        }
    }
    InstructionCheck {
        label,
        path,
        ok: issues.is_empty(),
        issues,
        optional: false,
    }
}

fn verify_continue_mcp(project_root: &Path) -> InstructionCheck {
    let path = continue_mcp_path(project_root);
    let label = format!(".continue/mcpServers/{CONTINUE_MCP_FILE}");
    if !path.exists() {
        return InstructionCheck {
            label,
            path,
            ok: false,
            issues: vec!["missing".into()],
            optional: false,
        };
    }
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    if continue_mcp_stale(&content) {
        return InstructionCheck {
            label,
            path,
            ok: false,
            issues: vec!["MCP config drifts from embedded template".into()],
            optional: false,
        };
    }
    if !content.contains("\"ax\"") || !content.contains("serve") || !content.contains("--mcp") {
        return InstructionCheck {
            label,
            path,
            ok: false,
            issues: vec!["missing ax MCP serve entry".into()],
            optional: false,
        };
    }
    InstructionCheck {
        label,
        path,
        ok: true,
        issues: vec![],
        optional: false,
    }
}

fn verify_claude_rule_bootstrap(project_root: &Path) -> InstructionCheck {
    let path = project_root.join(".claude").join("rules").join(CLAUDE_RULE_FILE);
    let label = format!(".claude/rules/{CLAUDE_RULE_FILE}");
    let issues = verify_dedicated_file(&path);
    if path.exists() {
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        if content.trim() != CLAUDE_RULE_BODY.trim() {
            return InstructionCheck {
                label,
                path,
                ok: false,
                issues: vec!["bootstrap content drifts from embedded template".into()],
                optional: false,
            };
        }
    }
    InstructionCheck {
        label,
        path,
        ok: issues.is_empty(),
        issues,
        optional: false,
    }
}

fn check_instruction(label: impl Into<String>, path: PathBuf, issues: Vec<String>, optional: bool) -> InstructionCheck {
    InstructionCheck {
        label: label.into(),
        path,
        ok: issues.is_empty(),
        issues,
        optional,
    }
}

fn verify_mcp_callmcp_shape_bootstrap(project_root: &Path) -> InstructionCheck {
    let path = mcp_callmcp_shape_rule_path(project_root);
    let label = format!(".cursor/rules/{MCP_CALLMCP_SHAPE_FILE}");
    if !path.exists() {
        return InstructionCheck {
            label,
            path,
            ok: false,
            issues: vec!["missing".into()],
            optional: false,
        };
    }
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    if mcp_callmcp_shape_stale(&content) {
        return InstructionCheck {
            label,
            path,
            ok: false,
            issues: vec!["bootstrap content drifts from embedded template".into()],
            optional: false,
        };
    }
    InstructionCheck {
        label,
        path,
        ok: true,
        issues: vec![],
        optional: false,
    }
}

/// Verify per-IDE bootstrap instruction files (Cursor, Continue, Claude, Gemini, Copilot, Windsurf, Cline).
pub fn verify_ide_bootstrap(project_root: &Path) -> Vec<InstructionCheck> {
    let claude_md = project_root.join(".claude").join("CLAUDE.md");
    let agents_md = project_root.join("AGENTS.md");
    let gemini_md = project_root.join("GEMINI.md");
    let copilot_md = project_root.join(".github").join("copilot-instructions.md");
    let windsurf_rules = project_root.join(".windsurfrules");
    let cline_rules = project_root.join(".clinerules");

    vec![
        verify_cursor_bootstrap(project_root),
        verify_mcp_callmcp_shape_bootstrap(project_root),
        verify_continue_bootstrap(project_root),
        verify_continue_mcp(project_root),
        verify_claude_rule_bootstrap(project_root),
        check_instruction(
            ".claude/CLAUDE.md",
            claude_md.clone(),
            verify_marked_instructions(&claude_md, CLAUDE_INSTRUCTIONS_BLOCK),
            false,
        ),
        check_instruction(
            "AGENTS.md",
            agents_md.clone(),
            verify_marked_instructions(&agents_md, AGENTS_INSTRUCTIONS_BLOCK),
            false,
        ),
        check_instruction(
            "GEMINI.md",
            gemini_md.clone(),
            verify_marked_instructions(&gemini_md, GEMINI_INSTRUCTIONS_BLOCK),
            false,
        ),
        check_instruction(
            ".github/copilot-instructions.md",
            copilot_md.clone(),
            verify_marked_instructions(&copilot_md, COPILOT_INSTRUCTIONS_BLOCK),
            false,
        ),
        check_instruction(
            ".windsurfrules",
            windsurf_rules.clone(),
            verify_marked_instructions(&windsurf_rules, WINDSURF_INSTRUCTIONS_BLOCK),
            false,
        ),
        check_instruction(
            ".clinerules",
            cline_rules.clone(),
            verify_marked_instructions(&cline_rules, CLINE_INSTRUCTIONS_BLOCK),
            false,
        ),
    ]
}

/// Verify IDE bootstrap files; with `fix`, create or repair via `seed_ide_agent_workflow`.
pub fn sync_ide_bootstrap(project_root: &Path, fix: bool) -> std::io::Result<SyncResult> {
    let mut result = SyncResult::default();
    result.checks = verify_ide_bootstrap(project_root);
    result.fail_count = result
        .checks
        .iter()
        .filter(|c| !c.ok && !c.optional)
        .count();

    if fix {
        // Always re-seed on --fix so template drift is repaired even when
        // basic verify_content checks still pass (e.g. customized Claude rule).
        let seed = seed_ide_agent_workflow(project_root)?;
        result.fixed.extend(seed.created);
        result.fixed.extend(seed.updated);
        result.checks = verify_ide_bootstrap(project_root);
        result.fail_count = result
            .checks
            .iter()
            .filter(|c| !c.ok && !c.optional)
            .count();
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn creates_mcp_callmcp_shape_rule_on_init() {
        let dir = tempdir().unwrap();
        let result = seed_ide_agent_workflow(dir.path()).unwrap();
        assert!(result
            .created
            .iter()
            .any(|p| p.contains("mcp-callmcp-shape.mdc")));
        let path = mcp_callmcp_shape_rule_path(dir.path());
        assert!(path.exists());
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("CallMcpTool"));
        assert!(content.contains("toolName"));
    }

    #[test]
    fn creates_cursor_ax_rule_on_init() {
        let dir = tempdir().unwrap();
        let result = seed_ide_agent_workflow(dir.path()).unwrap();
        assert!(result.created.iter().any(|p| p.contains("ax.mdc")));
        let path = cursor_rule_path(dir.path());
        assert!(path.exists());
        let content = std::fs::read_to_string(path).unwrap();
        assert!(crate::seed::verify_content(&content).is_empty());
    }

    #[test]
    fn migrates_legacy_ax_agent_workflow_to_ax_mdc() {
        let dir = tempdir().unwrap();
        let legacy = legacy_cursor_rule_path(dir.path());
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::fs::write(&legacy, CURSOR_RULE_BODY.as_bytes()).unwrap();
        let result = seed_ide_agent_workflow(dir.path()).unwrap();
        assert!(!legacy.exists());
        assert!(cursor_rule_path(dir.path()).exists());
        assert!(result.updated.iter().any(|p| p.contains("ax-agent-workflow")));
    }

    #[test]
    fn skips_when_bootstrap_already_in_another_cursor_rule() {
        let dir = tempdir().unwrap();
        let rules = cursor_rules_dir(dir.path());
        std::fs::create_dir_all(&rules).unwrap();
        std::fs::write(
            rules.join("custom.mdc"),
            b"---\nalwaysApply: true\n---\nCall ax_preflight exactly once per turn.\n",
        )
        .unwrap();
        let result = seed_ide_agent_workflow(dir.path()).unwrap();
        assert!(!cursor_rule_path(dir.path()).exists());
        assert!(result.skipped.iter().any(|p| p.contains("ax.mdc")));
    }

    #[test]
    fn repairs_stale_cursor_ax_rule() {
        let dir = tempdir().unwrap();
        let path = cursor_rule_path(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"---\nstale\n---\nno preflight here\n").unwrap();
        let result = seed_ide_agent_workflow(dir.path()).unwrap();
        assert!(result.updated.iter().any(|p| p.contains("ax.mdc")));
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("ax_preflight"));
    }

    #[test]
    fn seeds_continue_rule() {
        let dir = tempdir().unwrap();
        let result = seed_ide_agent_workflow(dir.path()).unwrap();
        assert!(result
            .created
            .iter()
            .any(|p| p == ".continue/rules/ax.md"));
        let path = continue_rule_path(dir.path());
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("ax_preflight"));
        assert!(crate::seed::verify_content(&content).is_empty());
    }

    #[test]
    fn seeds_continue_mcp_server() {
        let dir = tempdir().unwrap();
        let result = seed_ide_agent_workflow(dir.path()).unwrap();
        assert!(result
            .created
            .iter()
            .any(|p| p == ".continue/mcpServers/ax.json"));
        let path = continue_mcp_path(dir.path());
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("serve"));
        assert!(content.contains("--mcp"));
        assert!(!continue_mcp_stale(&content));
    }

    #[test]
    fn skips_when_bootstrap_already_in_another_continue_rule() {
        let dir = tempdir().unwrap();
        let rules = continue_rules_dir(dir.path());
        std::fs::create_dir_all(&rules).unwrap();
        std::fs::write(
            rules.join("custom.md"),
            b"---\nalwaysApply: true\n---\nCall ax_preflight exactly once per turn.\n",
        )
        .unwrap();
        let result = seed_ide_agent_workflow(dir.path()).unwrap();
        assert!(!continue_rule_path(dir.path()).exists());
        assert!(result
            .skipped
            .iter()
            .any(|p| p.contains(".continue/rules/ax.md")));
    }

    #[test]
    fn seeds_claude_rule_and_claude_md_marker() {
        let dir = tempdir().unwrap();
        let result = seed_ide_agent_workflow(dir.path()).unwrap();
        assert!(result.created.iter().any(|p| p == ".claude/rules/ax.md"));
        assert!(result.created.iter().any(|p| p == ".claude/CLAUDE.md"));
        let claude_md = std::fs::read_to_string(dir.path().join(".claude/CLAUDE.md")).unwrap();
        assert!(claude_md.contains(AX_SECTION_START));
        assert!(claude_md.contains(".claude/rules/ax.md"));
    }

    #[test]
    fn upserts_agents_and_gemini_markers() {
        let dir = tempdir().unwrap();
        let result = seed_ide_agent_workflow(dir.path()).unwrap();
        assert!(result.created.iter().any(|p| p == "AGENTS.md"));
        assert!(result.created.iter().any(|p| p == "GEMINI.md"));
        let agents = std::fs::read_to_string(dir.path().join("AGENTS.md")).unwrap();
        assert!(agents.contains("ax_preflight"));
    }

    #[test]
    fn seeds_copilot_instructions() {
        let dir = tempdir().unwrap();
        let result = seed_ide_agent_workflow(dir.path()).unwrap();
        assert!(result.created.iter().any(|p| p == ".github/copilot-instructions.md"));
        let content = std::fs::read_to_string(dir.path().join(".github/copilot-instructions.md")).unwrap();
        assert!(content.contains("ax_preflight"));
        assert!(content.contains(AX_SECTION_START));
    }

    #[test]
    fn seeds_windsurf_rules() {
        let dir = tempdir().unwrap();
        let result = seed_ide_agent_workflow(dir.path()).unwrap();
        assert!(result.created.iter().any(|p| p == ".windsurfrules"));
        let content = std::fs::read_to_string(dir.path().join(".windsurfrules")).unwrap();
        assert!(content.contains("ax_preflight"));
        assert!(content.contains(AX_SECTION_START));
    }

    #[test]
    fn seeds_cline_rules() {
        let dir = tempdir().unwrap();
        let result = seed_ide_agent_workflow(dir.path()).unwrap();
        assert!(result.created.iter().any(|p| p == ".clinerules"));
        let content = std::fs::read_to_string(dir.path().join(".clinerules")).unwrap();
        assert!(content.contains("ax_preflight"));
        assert!(content.contains(AX_SECTION_START));
    }

    #[test]
    fn verify_ide_bootstrap_fails_before_seed() {
        let dir = tempdir().unwrap();
        let checks = verify_ide_bootstrap(dir.path());
        let fails: Vec<_> = checks.iter().filter(|c| !c.ok && !c.optional).collect();
        assert!(fails.len() >= 9);
    }

    #[test]
    fn sync_ide_bootstrap_fix_creates_all_targets() {
        let dir = tempdir().unwrap();
        let synced = sync_ide_bootstrap(dir.path(), true).unwrap();
        assert_eq!(synced.fail_count, 0);
        assert!(synced.fixed.iter().any(|p| p.contains(".continue/rules/ax.md")));
        assert!(synced
            .fixed
            .iter()
            .any(|p| p.contains(".continue/mcpServers/ax.json")));
        assert!(synced.fixed.iter().any(|p| p.contains("AGENTS.md")));
        assert!(synced.fixed.iter().any(|p| p.contains("GEMINI.md")));
        assert!(synced.fixed.iter().any(|p| p.contains(".claude/rules/ax.md")));
        assert!(synced.fixed.iter().any(|p| p.contains("copilot-instructions.md")));
        assert!(synced.fixed.iter().any(|p| p.contains(".windsurfrules")));
        assert!(synced.fixed.iter().any(|p| p.contains(".clinerules")));
        assert!(synced
            .fixed
            .iter()
            .any(|p| p.contains("mcp-callmcp-shape.mdc")));
    }
}
