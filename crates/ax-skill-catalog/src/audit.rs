use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::validate::{read_skill_meta, validate_skill_name};

const SKILL_MD_CAP: usize = 24_000;
const FILE_LIST_CAP: usize = 200;
const TRIGGER_MIN_CHARS: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MechanicalFinding {
    pub severity: String,
    pub check: String,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditFile {
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentFinding {
    pub severity: String,
    pub check: String,
    pub evidence: String,
    pub suggestion: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditVerdict {
    pub verdict: String,
    pub clarity: u8,
    pub trigger: u8,
    pub steps: u8,
    pub files: u8,
    pub safety: u8,
    pub findings: Vec<AgentFinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillAuditReport {
    pub version: u32,
    pub name: String,
    pub candidate: bool,
    pub audited_at: String,
    pub source: String,
    pub sha: String,
    pub hash: String,
    pub directory: String,
    pub skill_md: String,
    pub truncated: bool,
    pub files: Vec<AuditFile>,
    pub mechanical: Vec<MechanicalFinding>,
    #[serde(default)]
    pub verdict: Option<AuditVerdict>,
    /// File the agent should edit. Empty on older skill reports.
    #[serde(default)]
    pub edit_file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditSummary {
    pub name: String,
    pub candidate: bool,
    pub audited_at: String,
    pub verdict: Option<String>,
    pub mechanical_fails: usize,
    pub mechanical_warns: usize,
}

pub fn require_one_skill(name: &str) -> Result<(), String> {
    if name.is_empty() || name == "*" || name.contains(',') || name.contains(' ') {
        return Err("audit one skill at a time".into());
    }
    validate_skill_name(name)
}

/// Where a referenced skill may already live. A path like
/// `.ax/policy/skills/<name>/SKILL.md` is present when that file exists, when
/// `<name>/SKILL.md` exists under a policy layer, or when `<name>` is stored
/// in the policy database.
pub struct SkillPresence<'a> {
    pub project: &'a Path,
    pub roots: &'a [PathBuf],
    pub database_names: &'a [String],
}

pub fn audit_directory(dir: &Path) -> Result<SkillAuditReport, String> {
    audit_directory_present(
        dir,
        &SkillPresence {
            project: dir,
            roots: &[],
            database_names: &[],
        },
    )
}

pub fn audit_directory_present(
    dir: &Path,
    presence: &SkillPresence<'_>,
) -> Result<SkillAuditReport, String> {
    let skill_md_path = dir.join("SKILL.md");
    if !skill_md_path.is_file() {
        return Err(format!("SKILL.md is missing in {}", dir.display()));
    }
    let raw = fs::read_to_string(&skill_md_path).map_err(|e| e.to_string())?;
    let dir_name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    require_one_skill(&dir_name)?;

    let mut mechanical = Vec::new();
    let (truncated, skill_md) = cap_text(&raw);
    if truncated {
        mechanical.push(finding(
            "info",
            "truncated",
            "SKILL.md is longer than 24000 bytes",
        ));
    }

    match read_skill_meta(&raw) {
        Ok(meta) => {
            if meta.name != dir_name {
                mechanical.push(finding(
                    "fail",
                    "name",
                    &format!("frontmatter name is {}; directory is {dir_name}", meta.name),
                ));
            }
            if meta.description.chars().count() < TRIGGER_MIN_CHARS {
                mechanical.push(finding("warn", "trigger", &meta.description));
            }
            let body = body_after_frontmatter(&raw);
            if body.trim().is_empty() {
                mechanical.push(finding("fail", "body", "(empty)"));
            } else {
                mechanical.extend(reference_findings(dir, &body, presence));
            }
        }
        Err(err) => mechanical.push(finding("fail", "frontmatter", &err)),
    }

    let files = list_files(dir, &mut mechanical)?;
    Ok(SkillAuditReport {
        version: 1,
        name: dir_name,
        candidate: false,
        audited_at: utc_stamp(),
        source: String::new(),
        sha: String::new(),
        hash: String::new(),
        directory: dir.display().to_string(),
        skill_md,
        truncated,
        files,
        mechanical,
        verdict: None,
        edit_file: dir.join("SKILL.md").display().to_string(),
    })
}

pub fn audit_skill_text(
    name: &str,
    raw: &str,
    presence: &SkillPresence<'_>,
) -> Result<SkillAuditReport, String> {
    require_one_skill(name)?;
    let mut mechanical = Vec::new();
    let (truncated, skill_md) = cap_text(raw);
    if truncated {
        mechanical.push(finding(
            "info",
            "truncated",
            "SKILL.md is longer than 24000 bytes",
        ));
    }
    match read_skill_meta(raw) {
        Ok(meta) => {
            if meta.name != name {
                mechanical.push(finding(
                    "fail",
                    "name",
                    &format!("frontmatter name is {}; skill is {name}", meta.name),
                ));
            }
            if meta.description.chars().count() < TRIGGER_MIN_CHARS {
                mechanical.push(finding("warn", "trigger", &meta.description));
            }
            let body = body_after_frontmatter(raw);
            if body.trim().is_empty() {
                mechanical.push(finding("fail", "body", "(empty)"));
            } else {
                mechanical.extend(reference_findings(presence.project, &body, presence));
            }
        }
        Err(err) => mechanical.push(finding("fail", "frontmatter", &err)),
    }
    Ok(SkillAuditReport {
        version: 1,
        name: name.to_string(),
        candidate: false,
        audited_at: utc_stamp(),
        source: String::new(),
        sha: String::new(),
        hash: String::new(),
        directory: format!("database:{name}"),
        skill_md,
        truncated,
        files: Vec::new(),
        mechanical,
        verdict: None,
        edit_file: format!("database:{name}"),
    })
}

pub fn audit_rule_file(file: &Path, project: &Path) -> Result<SkillAuditReport, String> {
    audit_rule_present(
        file,
        &SkillPresence {
            project,
            roots: &[],
            database_names: &[],
        },
    )
}

pub fn audit_rule_present(
    file: &Path,
    presence: &SkillPresence<'_>,
) -> Result<SkillAuditReport, String> {
    let project = presence.project;
    let name = file
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    require_one_skill(&name)?;
    let meta = fs::symlink_metadata(file).map_err(|e| e.to_string())?;
    let directory = file.parent().unwrap_or(project).display().to_string();
    let edit_file = file.display().to_string();
    if meta.file_type().is_symlink() {
        return Ok(rule_report(
            name,
            directory,
            edit_file,
            String::new(),
            false,
            vec![finding("fail", "symlink", &file.display().to_string())],
            Vec::new(),
        ));
    }
    let raw = fs::read_to_string(file).map_err(|e| e.to_string())?;
    let (truncated, skill_md) = cap_text(&raw);
    let mut mechanical = Vec::new();
    if truncated {
        mechanical.push(finding(
            "info",
            "truncated",
            "rule is longer than 24000 bytes",
        ));
    }
    match rule_fields(&raw) {
        Ok(fields) => {
            if fields.id != name {
                mechanical.push(finding(
                    "fail",
                    "id",
                    &format!("frontmatter id is {}; file is {name}.mdc", fields.id),
                ));
            }
            if !matches!(fields.level.as_str(), "CRITICAL" | "WARNING" | "INFO") {
                mechanical.push(finding(
                    "fail",
                    "frontmatter",
                    &format!("level is {}", fields.level),
                ));
            }
            if fields.body.trim().is_empty() {
                mechanical.push(finding("fail", "body", "(empty)"));
            } else {
                mechanical.extend(reference_findings(project, &fields.body, presence));
            }
            if !fields.always_apply && fields.triggers.is_empty() && fields.globs.is_empty() {
                mechanical.push(finding("fail", "applies", "no trigger or glob"));
            } else if !fields.always_apply && fields.globs.is_empty() {
                let text = fields.triggers.join(" ");
                if text.chars().count() < TRIGGER_MIN_CHARS {
                    mechanical.push(finding("warn", "applies", &text));
                }
            }
        }
        Err(err) => mechanical.push(finding("fail", "frontmatter", &err)),
    }
    let files = vec![AuditFile {
        path: file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("rule.mdc")
            .to_string(),
        bytes: meta.len(),
    }];
    Ok(rule_report(
        name, directory, edit_file, skill_md, truncated, mechanical, files,
    ))
}

pub fn find_rule_file(roots: &[PathBuf], id: &str) -> Result<PathBuf, String> {
    require_one_skill(id)?;
    let target = format!("{id}.mdc");
    let mut chosen: Option<PathBuf> = None;
    for root in roots {
        if !root.exists() {
            continue;
        }
        let mut found = Vec::new();
        let walker = WalkDir::new(root).max_depth(6).follow_links(false);
        for entry in walker {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_name() != target.as_str() {
                continue;
            }
            if entry.file_type().is_file() || entry.file_type().is_symlink() {
                let path = entry.path().to_path_buf();
                if !found.contains(&path) {
                    found.push(path);
                }
            }
        }
        if let Some(path) = choose_rule_copy(root, &target, found)? {
            chosen = Some(path);
        }
    }
    chosen.ok_or_else(|| format!("rule is not on disk: {id}"))
}

fn choose_rule_copy(root: &Path, target: &str, found: Vec<PathBuf>) -> Result<Option<PathBuf>, String> {
    if found.is_empty() {
        return Ok(None);
    }
    if found.len() == 1 {
        return Ok(Some(found.into_iter().next().unwrap()));
    }
    let direct = root.join("rules").join(target);
    if found.iter().any(|path| path == &direct) {
        return Ok(Some(direct));
    }
    let mut rules: Vec<PathBuf> = found
        .into_iter()
        .filter(|path| path.ends_with(Path::new("rules").join(target)))
        .collect();
    if rules.is_empty() {
        return Err(format!("more than one file named {target}"));
    }
    rules.sort_by_key(|path| path.components().count());
    Ok(rules.into_iter().next())
}

struct RuleFields {
    id: String,
    level: String,
    always_apply: bool,
    triggers: Vec<String>,
    globs: Vec<String>,
    body: String,
}

fn rule_fields(raw: &str) -> Result<RuleFields, String> {
    let trimmed = raw.trim_start();
    if !trimmed.starts_with("---") {
        return Err("rule must start with YAML frontmatter".into());
    }
    let rest = trimmed.trim_start_matches("---").trim_start();
    let end = rest
        .find("\n---")
        .ok_or_else(|| "rule is missing the closing ---".to_string())?;
    let yaml = rest[..end].trim();
    let body = rest[end + 4..].trim().to_string();
    let value: serde_yaml::Value =
        serde_yaml::from_str(yaml).map_err(|e| format!("rule frontmatter: {e}"))?;
    let id = value
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if id.is_empty() {
        return Err("id is required".into());
    }
    let level = value
        .get("level")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let always_apply = value
        .get("alwaysApply")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    Ok(RuleFields {
        id,
        level,
        always_apply,
        triggers: yaml_strings(value.get("triggers")),
        globs: yaml_strings(value.get("globs")),
        body,
    })
}

fn yaml_strings(value: Option<&serde_yaml::Value>) -> Vec<String> {
    match value {
        Some(serde_yaml::Value::Sequence(items)) => items
            .iter()
            .filter_map(|item| item.as_str().map(|s| s.to_string()))
            .collect(),
        Some(serde_yaml::Value::String(text)) => vec![text.clone()],
        _ => Vec::new(),
    }
}

fn rule_report(
    name: String,
    directory: String,
    edit_file: String,
    skill_md: String,
    truncated: bool,
    mechanical: Vec<MechanicalFinding>,
    files: Vec<AuditFile>,
) -> SkillAuditReport {
    SkillAuditReport {
        version: 1,
        name,
        candidate: false,
        audited_at: utc_stamp(),
        source: String::new(),
        sha: String::new(),
        hash: String::new(),
        directory,
        skill_md,
        truncated,
        files,
        mechanical,
        verdict: None,
        edit_file,
    }
}

pub fn parse_verdict(json: &str, skill_md: &str) -> Result<AuditVerdict, String> {
    let mut verdict: AuditVerdict = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if !matches!(verdict.verdict.as_str(), "pass" | "revise" | "reject") {
        return Err("verdict must be pass, revise, or reject".into());
    }
    for score in [
        verdict.clarity,
        verdict.trigger,
        verdict.steps,
        verdict.files,
        verdict.safety,
    ] {
        if !(1..=5).contains(&score) {
            return Err("score must be 1-5".into());
        }
    }
    for finding in &mut verdict.findings {
        if !matches!(finding.severity.as_str(), "info" | "warn" | "fail") {
            return Err("finding severity must be info, warn, or fail".into());
        }
        let evidence = finding.evidence.trim().to_string();
        if evidence.is_empty() || !skill_md.contains(&evidence) {
            return Err("evidence must quote the skill".into());
        }
        finding.evidence = evidence;
    }
    Ok(verdict)
}

pub fn store_report(root: &Path, report: &SkillAuditReport) -> Result<(), String> {
    require_one_skill(&report.name)?;
    let base = if report.candidate {
        root.join("_candidates").join(&report.name)
    } else {
        root.join(&report.name)
    };
    fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(report).map_err(|e| e.to_string())?;
    let stamp = report.audited_at.replace(':', "");
    fs::write(base.join(format!("{stamp}.json")), &json).map_err(|e| e.to_string())?;
    fs::write(base.join("latest.json"), json).map_err(|e| e.to_string())
}

pub fn load_latest(root: &Path, name: &str) -> Result<Option<SkillAuditReport>, String> {
    require_one_skill(name)?;
    let path = root.join(name).join("latest.json");
    if !path.is_file() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let report = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    Ok(Some(report))
}

pub fn list_latest(root: &Path) -> Result<Vec<AuditSummary>, String> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == "_candidates" || name.starts_with('.') {
            continue;
        }
        if !entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            continue;
        }
        let Some(report) = load_latest(root, &name)? else {
            continue;
        };
        out.push(summarize(&report));
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub fn find_skill_dir(root: &Path, name: &str) -> Result<PathBuf, String> {
    require_one_skill(name)?;
    for candidate in [root.join(name), root.join("skills").join(name)] {
        if candidate.join("SKILL.md").is_file() {
            return Ok(candidate);
        }
    }
    let walker = WalkDir::new(root).max_depth(6).follow_links(false);
    for entry in walker {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().is_symlink() {
            continue;
        }
        if entry.file_name() == "SKILL.md" {
            if let Some(dir) = entry.path().parent() {
                if dir.file_name().and_then(|n| n.to_str()) == Some(name) {
                    return Ok(dir.to_path_buf());
                }
            }
        }
    }
    Err(format!("skill not in source: {name}"))
}

fn summarize(report: &SkillAuditReport) -> AuditSummary {
    AuditSummary {
        name: report.name.clone(),
        candidate: report.candidate,
        audited_at: report.audited_at.clone(),
        verdict: report.verdict.as_ref().map(|v| v.verdict.clone()),
        mechanical_fails: report
            .mechanical
            .iter()
            .filter(|f| f.severity == "fail")
            .count(),
        mechanical_warns: report
            .mechanical
            .iter()
            .filter(|f| f.severity == "warn")
            .count(),
    }
}

fn finding(severity: &str, check: &str, evidence: &str) -> MechanicalFinding {
    MechanicalFinding {
        severity: severity.to_string(),
        check: check.to_string(),
        evidence: evidence.to_string(),
    }
}

fn cap_text(raw: &str) -> (bool, String) {
    if raw.len() <= SKILL_MD_CAP {
        return (false, raw.to_string());
    }
    let mut end = SKILL_MD_CAP;
    while !raw.is_char_boundary(end) {
        end -= 1;
    }
    (true, raw[..end].to_string())
}

fn body_after_frontmatter(markdown: &str) -> String {
    let trimmed = markdown.trim_start();
    let Some(rest) = trimmed.strip_prefix("---") else {
        return trimmed.to_string();
    };
    match rest.find("\n---") {
        Some(end) => rest[end + 4..].trim().to_string(),
        None => String::new(),
    }
}

fn reference_findings(dir: &Path, body: &str, presence: &SkillPresence<'_>) -> Vec<MechanicalFinding> {
    let mut findings = Vec::new();
    let mut seen = Vec::new();
    for raw in referenced_paths(body) {
        if seen.contains(&raw) {
            continue;
        }
        seen.push(raw.clone());
        if raw.split(['/', '\\']).any(|part| part == "..") {
            findings.push(finding("fail", "path", &raw));
            continue;
        }
        if !referenced_item_exists(dir, &raw, presence) {
            findings.push(finding("warn", "missing-file", &raw));
        }
    }
    findings
}

fn referenced_item_exists(dir: &Path, raw: &str, presence: &SkillPresence<'_>) -> bool {
    let at_dir = dir.join(raw);
    if at_dir.is_file() || at_dir.is_dir() {
        return true;
    }
    let at_project = presence.project.join(raw);
    if at_project.is_file() || at_project.is_dir() {
        return true;
    }
    let Some(name) = skill_reference_name(raw) else {
        return false;
    };
    if presence
        .database_names
        .iter()
        .any(|known| known.eq_ignore_ascii_case(&name))
    {
        return true;
    }
    presence.roots.iter().any(|root| {
        root.join("skills")
            .join(&name)
            .join("SKILL.md")
            .is_file()
    })
}

/// `skills/<name>` or `skills/<name>/SKILL.md`, with any prefix such as
/// `.ax/policy/` or `.agents/`.
fn skill_reference_name(raw: &str) -> Option<String> {
    let parts: Vec<&str> = raw
        .split(['/', '\\'])
        .filter(|part| !part.is_empty())
        .collect();
    let index = parts.iter().position(|part| *part == "skills")?;
    let name = *parts.get(index + 1)?;
    if name == "SKILL.md" || name == "." || name == ".." || name.contains('<') || name.contains('>')
    {
        return None;
    }
    let rest = &parts[index + 2..];
    if rest.is_empty() || rest == ["SKILL.md"] {
        Some(name.to_string())
    } else {
        None
    }
}

fn referenced_paths(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    collect_between(body, "](", ")", &mut out);
    collect_between(body, "`", "`", &mut out);
    out
}

fn collect_between(body: &str, open: &str, close: &str, out: &mut Vec<String>) {
    let mut rest = body;
    while let Some(start) = rest.find(open) {
        rest = &rest[start + open.len()..];
        let Some(end) = rest.find(close) else {
            break;
        };
        push_ref(out, &rest[..end]);
        rest = &rest[end + close.len()..];
    }
}

fn push_ref(out: &mut Vec<String>, raw: &str) {
    let raw = raw.trim();
    if raw.is_empty()
        || raw.len() > 200
        || raw.contains("://")
        || raw.starts_with('#')
        || raw.starts_with('/')
    {
        return;
    }
    let path = raw.split(['?', '#']).next().unwrap_or(raw).trim();
    if path.is_empty() || path.contains(' ') || path.contains('<') || path.contains('>') {
        return;
    }
    let looks = path.contains('/')
        || path.contains('\\')
        || path.ends_with(".md")
        || path.ends_with(".sh")
        || path.ends_with(".py")
        || path.ends_with(".js")
        || path.ends_with(".ts")
        || path.ends_with(".ps1")
        || path.ends_with(".json")
        || path.ends_with(".txt")
        || path.ends_with(".yml")
        || path.ends_with(".yaml");
    if looks {
        out.push(path.to_string());
    }
}

fn list_files(
    dir: &Path,
    mechanical: &mut Vec<MechanicalFinding>,
) -> Result<Vec<AuditFile>, String> {
    let mut files = Vec::new();
    let walker = WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            name != ".git" && name != "node_modules"
        });
    for entry in walker {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().is_symlink() {
            let rel = rel_path(dir, entry.path());
            mechanical.push(finding("fail", "symlink", &rel));
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        if files.len() >= FILE_LIST_CAP {
            mechanical.push(finding(
                "info",
                "file-list-capped",
                "file list stopped at 200",
            ));
            break;
        }
        let bytes = entry.metadata().map_err(|e| e.to_string())?.len();
        files.push(AuditFile {
            path: rel_path(dir, entry.path()),
            bytes,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn rel_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .filter_map(|c| match c {
            Component::Normal(part) => Some(part.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

pub fn needs_edit(report: &SkillAuditReport) -> bool {
    if report
        .mechanical
        .iter()
        .any(|finding| finding.severity == "warn" || finding.severity == "fail")
    {
        return true;
    }
    report
        .verdict
        .as_ref()
        .is_some_and(|verdict| verdict.verdict != "pass" || !verdict.findings.is_empty())
}

pub fn agent_brief(report: &SkillAuditReport, file: &str) -> String {
    let kind = if file.ends_with(".mdc") {
        "rule"
    } else {
        "skill"
    };
    let mut body = format!(
        "<ax_audit kind=\"{kind}\" name=\"{}\" file=\"{file}\">\n",
        report.name
    );
    if report.candidate {
        body.push_str("Candidate only. Nothing was installed.\n");
    }
    if !needs_edit(report) {
        body.push_str("No edits.\n</ax_audit>\n");
        return body;
    }
    body.push_str("Edit this file. Leave it enabled.\n");
    for finding in &report.mechanical {
        if finding.severity == "info" && finding.check != "truncated" {
            continue;
        }
        body.push_str(&format!(
            "- {} {}: {} {}\n",
            finding.severity,
            finding.check,
            finding.evidence,
            edit_instruction(&finding.check)
        ));
    }
    if let Some(verdict) = &report.verdict {
        for finding in &verdict.findings {
            if finding.suggestion.is_empty() {
                continue;
            }
            body.push_str(&format!(
                "- suggestion {}: {}\n",
                finding.check, finding.suggestion
            ));
        }
    }
    body.push_str("</ax_audit>\n");
    body
}

pub fn open_briefs(root: &Path) -> Result<String, String> {
    let mut blocks = Vec::new();
    let mut extra = Vec::new();
    push_open(root, &mut blocks, &mut extra)?;
    finish_briefs(blocks, extra)
}

pub fn project_briefs(project: &Path) -> Result<String, String> {
    let mut blocks = Vec::new();
    let mut extra = Vec::new();
    push_open(
        &project.join(".ax").join("skill-audits"),
        &mut blocks,
        &mut extra,
    )?;
    push_open(
        &project.join(".ax").join("rule-audits"),
        &mut blocks,
        &mut extra,
    )?;
    finish_briefs(blocks, extra)
}

fn push_open(root: &Path, blocks: &mut Vec<String>, extra: &mut Vec<String>) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    for summary in list_latest(root)? {
        let Some(report) = load_latest(root, &summary.name)? else {
            continue;
        };
        if report.candidate || !needs_edit(&report) {
            continue;
        }
        if blocks.len() >= 8 {
            extra.push(summary.name);
            continue;
        }
        let file = if report.edit_file.is_empty() {
            format!("{}/SKILL.md", report.directory)
        } else {
            report.edit_file.clone()
        };
        blocks.push(agent_brief(&report, &file));
    }
    Ok(())
}

fn finish_briefs(blocks: Vec<String>, extra: Vec<String>) -> Result<String, String> {
    if blocks.is_empty() {
        return Ok(String::new());
    }
    let mut out =
        String::from("<ax_audits note=\"Edit these files. Leave each skill and rule enabled.\">\n");
    for block in blocks {
        out.push_str(&block);
        out.push('\n');
    }
    if !extra.is_empty() {
        out.push_str(&format!(
            "More: {}. Run `ax skill brief` for each.\n",
            extra.join(", ")
        ));
    }
    out.push_str("</ax_audits>\n");
    Ok(out)
}

fn edit_instruction(check: &str) -> &'static str {
    match check {
        "trigger" => "Rewrite the description so it says when to use this.",
        "applies" => "Turn on alwaysApply, or add a trigger or glob.",
        "id" => "Set the frontmatter id to the file name.",
        "body" => "Add the steps the agent should follow after the frontmatter.",
        "missing-file" => "Add that file or remove the reference.",
        "path" => "Remove the parent-directory path.",
        "symlink" => "Replace the symlink with a real file.",
        "name" => "Set the frontmatter name to the directory name.",
        "frontmatter" => "Fix the frontmatter so name and description are valid.",
        "truncated" => "Move extra detail into references/.",
        _ => "Apply this finding in the file.",
    }
}

fn utc_stamp() -> String {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format_unix_utc(elapsed.as_secs(), elapsed.subsec_millis())
}

fn format_unix_utc(secs: u64, millis: u32) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}.{millis:03}Z",
        hour = rem / 3600,
        minute = (rem % 3600) / 60,
        second = rem % 60,
    )
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = y + if month <= 2 { 1 } else { 0 };
    (year as i32, month as u32, day as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_skill(dir: &Path, body: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("SKILL.md"), body).unwrap();
    }

    const SHORT: &str = "---\nname: pdf\ndescription: Read PDFs.\n---\n\n";

    #[test]
    fn refuses_more_than_one_name() {
        assert_eq!(
            require_one_skill("pdf,review").unwrap_err(),
            "audit one skill at a time"
        );
        assert_eq!(
            require_one_skill("*").unwrap_err(),
            "audit one skill at a time"
        );
    }

    #[test]
    fn short_description_and_empty_body_are_findings() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("pdf");
        write_skill(&dir, SHORT);
        let report = audit_directory(&dir).unwrap();
        assert!(report
            .mechanical
            .iter()
            .any(|f| f.check == "trigger" && f.evidence == "Read PDFs."));
        assert!(report
            .mechanical
            .iter()
            .any(|f| f.check == "body" && f.severity == "fail"));
    }

    #[test]
    fn missing_file_and_parent_path_are_findings() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("pdf");
        let description = "Use this when the user asks to extract text from a PDF file.";
        let body = format!(
            "---\nname: pdf\ndescription: {description}\n---\n\nRead `scripts/run.sh` and `../secret.txt`.\n"
        );
        write_skill(&dir, &body);
        let report = audit_directory(&dir).unwrap();
        assert!(report
            .mechanical
            .iter()
            .any(|f| f.check == "missing-file" && f.evidence == "scripts/run.sh"));
        assert!(report
            .mechanical
            .iter()
            .any(|f| f.check == "path" && f.evidence == "../secret.txt"));
        assert!(!dir.join("secret.txt").exists());
    }

    #[test]
    fn placeholders_and_existing_directories_are_not_missing_files() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(".agents").join("skills")).unwrap();
        let file = root.path().join("author-skill-rule.mdc");
        fs::write(
            &file,
            "---\nid: author-skill-rule\nlevel: WARNING\ntriggers: [\"author a skill\"]\n---\n\nSee `.agents/skills/<name>/SKILL.md` and `.agents/skills`.\n",
        )
        .unwrap();
        let report = audit_rule_file(&file, root.path()).unwrap();
        assert!(
            !report.mechanical.iter().any(|f| f.check == "missing-file"),
            "{:?}",
            report.mechanical
        );
    }

    #[test]
    fn a_skill_on_another_policy_layer_is_not_missing() {
        let root = tempfile::tempdir().unwrap();
        let layer = root.path().join(".agents");
        let skill = layer.join("skills").join("subagents");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "---\nname: subagents\ndescription: Run subagents.\n---\n\nBody.\n").unwrap();
        let file = root.path().join("subagents.mdc");
        fs::write(
            &file,
            "---\nid: subagents\nlevel: CRITICAL\nalwaysApply: true\n---\n\nRead `.ax/policy/skills/subagents/SKILL.md`.\n",
        )
        .unwrap();
        let roots = vec![layer];
        let report = audit_rule_present(
            &file,
            &SkillPresence {
                project: root.path(),
                roots: &roots,
                database_names: &[],
            },
        )
        .unwrap();
        assert!(
            !report.mechanical.iter().any(|f| f.check == "missing-file"),
            "{:?}",
            report.mechanical
        );
    }

    #[test]
    fn a_skill_stored_in_the_database_is_not_missing() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("subagents.mdc");
        fs::write(
            &file,
            "---\nid: subagents\nlevel: CRITICAL\nalwaysApply: true\n---\n\nRead `.ax/policy/skills/subagents/SKILL.md`.\n",
        )
        .unwrap();
        let names = vec!["subagents".to_string()];
        let report = audit_rule_present(
            &file,
            &SkillPresence {
                project: root.path(),
                roots: &[],
                database_names: &names,
            },
        )
        .unwrap();
        assert!(
            !report.mechanical.iter().any(|f| f.check == "missing-file"),
            "{:?}",
            report.mechanical
        );
    }

    #[test]
    fn an_unknown_skill_path_is_still_missing() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("subagents.mdc");
        fs::write(
            &file,
            "---\nid: subagents\nlevel: CRITICAL\nalwaysApply: true\n---\n\nRead `.ax/policy/skills/missing/SKILL.md` and `scripts/run.sh`.\n",
        )
        .unwrap();
        let names = vec!["subagents".to_string()];
        let report = audit_rule_present(
            &file,
            &SkillPresence {
                project: root.path(),
                roots: &[],
                database_names: &names,
            },
        )
        .unwrap();
        let missing: Vec<_> = report
            .mechanical
            .iter()
            .filter(|f| f.check == "missing-file")
            .map(|f| f.evidence.as_str())
            .collect();
        assert_eq!(
            missing,
            vec![".ax/policy/skills/missing/SKILL.md", "scripts/run.sh"]
        );
    }

    #[test]
    fn name_mismatch_is_a_failure() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("pdf");
        write_skill(
            &dir,
            "---\nname: other\ndescription: Use this when the user asks to extract text from a PDF file.\n---\n\nDo the work.\n",
        );
        let report = audit_directory(&dir).unwrap();
        assert!(report
            .mechanical
            .iter()
            .any(|f| f.check == "name" && f.severity == "fail"));
        assert_eq!(report.name, "pdf");
    }

    #[test]
    fn does_not_execute_a_script() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("pdf");
        write_skill(
            &dir,
            "---\nname: pdf\ndescription: Use this when the user asks to extract text from a PDF file.\n---\n\nRun scripts/run.sh now.\n",
        );
        fs::create_dir(dir.join("scripts")).unwrap();
        fs::write(dir.join("scripts/run.sh"), "touch ran\n").unwrap();
        let report = audit_directory(&dir).unwrap();
        assert!(!dir.join("ran").exists());
        assert!(report.files.iter().any(|f| f.path == "scripts/run.sh"));
        assert!(!report.mechanical.iter().any(|f| f.check == "missing-file"));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_is_not_followed() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("pdf");
        write_skill(
            &dir,
            "---\nname: pdf\ndescription: Use this when the user asks to extract text from a PDF file.\n---\n\nRead the notes.\n",
        );
        let nested = root.path().join("nested");
        fs::create_dir(&nested).unwrap();
        fs::write(nested.join("hidden.sh"), "echo hi\n").unwrap();
        std::os::unix::fs::symlink(&nested, dir.join("scripts")).unwrap();
        let report = audit_directory(&dir).unwrap();
        assert!(report
            .mechanical
            .iter()
            .any(|f| f.check == "symlink" && f.evidence == "scripts"));
        assert!(!report.files.iter().any(|f| f.path.contains("hidden.sh")));
        assert!(!report.skill_md.contains("echo hi"));
    }

    #[test]
    fn truncates_a_long_skill() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("pdf");
        let description = "Use this when the user asks to extract text from a PDF file.";
        let mut body = format!("---\nname: pdf\ndescription: {description}\n---\n\n");
        body.push_str(&"x".repeat(25_000));
        write_skill(&dir, &body);
        let report = audit_directory(&dir).unwrap();
        assert!(report.truncated);
        assert!(report.skill_md.len() <= SKILL_MD_CAP);
        assert!(report.mechanical.iter().any(|f| f.check == "truncated"));
    }

    #[test]
    fn verdict_must_quote_the_skill_and_score_in_range() {
        let skill = "Run scripts/run.sh now.";
        let ok = r#"{"verdict":"revise","clarity":3,"trigger":2,"steps":4,"files":5,"safety":5,"findings":[{"severity":"warn","check":"steps","evidence":"Run scripts/run.sh now.","suggestion":"Say when."}]}"#;
        let verdict = parse_verdict(ok, skill).unwrap();
        assert_eq!(verdict.verdict, "revise");
        assert!(parse_verdict(
            r#"{"verdict":"maybe","clarity":3,"trigger":2,"steps":4,"files":5,"safety":5,"findings":[]}"#,
            skill
        )
        .unwrap_err()
        .contains("verdict must be"));
        assert_eq!(
            parse_verdict(
                r#"{"verdict":"pass","clarity":0,"trigger":2,"steps":4,"files":5,"safety":5,"findings":[]}"#,
                skill
            )
            .unwrap_err(),
            "score must be 1-5"
        );
        assert_eq!(
            parse_verdict(
                r#"{"verdict":"pass","clarity":3,"trigger":2,"steps":4,"files":5,"safety":5,"findings":[{"severity":"warn","check":"steps","evidence":"not in the skill","suggestion":""}]}"#,
                skill
            )
            .unwrap_err(),
            "evidence must quote the skill"
        );
    }

    #[test]
    fn latest_report_round_trips() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("pdf");
        write_skill(&dir, SHORT);
        let report = audit_directory(&dir).unwrap();
        let audits = root.path().join("audits");
        store_report(&audits, &report).unwrap();
        let loaded = load_latest(&audits, "pdf").unwrap().unwrap();
        assert_eq!(loaded.mechanical, report.mechanical);
        let listed = list_latest(&audits).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "pdf");
        assert!(listed[0].verdict.is_none());
    }

    #[test]
    fn brief_names_the_file_and_the_edit() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("pdf");
        write_skill(&dir, SHORT);
        let report = audit_directory(&dir).unwrap();
        let file = dir.join("SKILL.md");
        let brief = agent_brief(&report, &file.display().to_string());
        assert!(brief.contains("kind=\"skill\""));
        assert!(brief.contains(&file.display().to_string()));
        assert!(brief.contains("Leave it enabled."));
        assert!(brief.contains("Rewrite the description so it says when to use this."));
        let audits = root.path().join("audits");
        store_report(&audits, &report).unwrap();
        let open = open_briefs(&audits).unwrap();
        assert!(open.contains("name=\"pdf\""));
    }

    #[test]
    fn a_clean_report_asks_for_no_edits() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("pdf");
        write_skill(
            &dir,
            "---\nname: pdf\ndescription: Use this when the user asks to extract text from a PDF file.\n---\n\nRead the file.\n",
        );
        let mut report = audit_directory(&dir).unwrap();
        assert!(!needs_edit(&report));
        report.verdict = Some(AuditVerdict {
            verdict: "pass".into(),
            clarity: 5,
            trigger: 5,
            steps: 5,
            files: 5,
            safety: 5,
            findings: Vec::new(),
        });
        let brief = agent_brief(&report, "rules/pdf.mdc");
        assert!(brief.contains("kind=\"rule\""));
        assert!(brief.contains("No edits."));
        let audits = root.path().join("audits");
        store_report(&audits, &report).unwrap();
        assert!(open_briefs(&audits).unwrap().is_empty());
    }

    #[test]
    fn rule_without_a_trigger_or_body_needs_an_edit() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("pdf.mdc");
        fs::write(
            &file,
            "---\nid: pdf\nlevel: WARNING\nalwaysApply: false\n---\n",
        )
        .unwrap();
        let report = audit_rule_file(&file, root.path()).unwrap();
        assert!(report
            .mechanical
            .iter()
            .any(|f| f.check == "body" && f.severity == "fail"));
        assert!(report
            .mechanical
            .iter()
            .any(|f| f.check == "applies" && f.severity == "fail"));
        let brief = agent_brief(&report, &report.edit_file);
        assert!(brief.contains("kind=\"rule\""));
        assert!(brief.contains("Turn on alwaysApply, or add a trigger or glob."));
        assert!(brief.contains(&file.display().to_string()));
        let audits = root.path().join(".ax").join("rule-audits");
        store_report(&audits, &report).unwrap();
        let open = project_briefs(root.path()).unwrap();
        assert!(open.contains("kind=\"rule\""));
    }

    #[test]
    fn always_apply_rule_with_a_body_needs_no_edit() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("pdf.mdc");
        fs::write(
            &file,
            "---\nid: pdf\nlevel: CRITICAL\nalwaysApply: true\n---\n\nKeep the file UTF-8.\n",
        )
        .unwrap();
        let report = audit_rule_file(&file, root.path()).unwrap();
        assert!(!needs_edit(&report));
        assert!(agent_brief(&report, &report.edit_file).contains("No edits."));
    }

    #[test]
    fn rule_id_must_match_the_file_name() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("pdf.mdc");
        fs::write(
            &file,
            "---\nid: other\nlevel: INFO\nalwaysApply: true\n---\n\nDo the thing.\n",
        )
        .unwrap();
        let report = audit_rule_file(&file, root.path()).unwrap();
        assert!(report
            .mechanical
            .iter()
            .any(|f| f.check == "id" && f.severity == "fail"));
    }

    #[test]
    fn epoch_formats_as_utc() {
        assert_eq!(format_unix_utc(0, 0), "19700101T000000.000Z");
        assert_eq!(format_unix_utc(86_400, 5), "19700102T000000.005Z");
    }

    #[test]
    fn layered_copies_of_a_rule_use_the_later_layer() {
        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join("io").join(".agents").join("rules");
        let project = root.path().join("io").join("ax").join(".agents").join("rules");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::write(workspace.join("subagents.mdc"), "workspace").unwrap();
        fs::write(project.join("subagents.mdc"), "project").unwrap();
        let roots = vec![
            root.path().join("io").join(".agents"),
            root.path().join("io").join("ax").join(".agents"),
        ];
        let found = find_rule_file(&roots, "subagents").unwrap();
        assert_eq!(found, project.join("subagents.mdc"));
    }

    #[test]
    fn one_tree_with_two_loose_copies_is_still_an_error() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("a")).unwrap();
        fs::create_dir_all(root.path().join("b")).unwrap();
        fs::write(root.path().join("a").join("subagents.mdc"), "a").unwrap();
        fs::write(root.path().join("b").join("subagents.mdc"), "b").unwrap();
        let err = find_rule_file(&[root.path().to_path_buf()], "subagents").unwrap_err();
        assert!(err.contains("more than one file named subagents.mdc"));
    }
}
