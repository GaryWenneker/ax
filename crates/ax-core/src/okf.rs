//! Open Knowledge Format (OKF) bundle export, validate, and optional wiki publish.
//!
//! An OKF bundle is a deterministic tree of Markdown files with YAML frontmatter,
//! one page per code concept, cross-linked via relative links. SQLite (`.ax/ax.db`)
//! remains the source of truth; OKF is a portable projection.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

use ax_types::{Edge, EdgeKind, Node, NodeKind};
use serde::Deserialize;

const DEFAULT_OUT_DIR: &str = ".ax/knowledge";
const DEFAULT_WIKI_LOCAL: &str = ".ax/wiki-okf";
const DEFAULT_WIKI_SUBDIR: &str = "okf";
const DEFAULT_COMMIT_MSG: &str = "chore: refresh Open Knowledge Format (OKF) bundle";

/// Project `ax.json` → `okf` section for Open Knowledge Format (OKF) export.
#[derive(Debug, Clone)]
pub struct OkfConfig {
    pub enabled: bool,
    /// Relative path from project root (stored as relative string).
    pub out_dir: String,
    pub auto_export_on_sync: bool,
    /// Empty = all non-File/Doc kinds. Otherwise kind strings (`function`, …).
    pub kinds: Vec<String>,
    pub azdo_wiki: OkfWikiConfig,
}

#[derive(Debug, Clone)]
pub struct OkfWikiConfig {
    pub enabled: bool,
    pub remote: String,
    /// Relative path for local wiki clone.
    pub local: String,
    pub subdir: String,
    pub commit_message: String,
}

impl Default for OkfConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            out_dir: DEFAULT_OUT_DIR.into(),
            auto_export_on_sync: false,
            kinds: Vec::new(),
            azdo_wiki: OkfWikiConfig::default(),
        }
    }
}

impl Default for OkfWikiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            remote: String::new(),
            local: DEFAULT_WIKI_LOCAL.into(),
            subdir: DEFAULT_WIKI_SUBDIR.into(),
            commit_message: DEFAULT_COMMIT_MSG.into(),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
struct AxJsonRoot {
    okf: Option<OkfJson>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct OkfJson {
    enabled: Option<bool>,
    out_dir: Option<String>,
    auto_export_on_sync: Option<bool>,
    kinds: Option<Vec<String>>,
    azdo_wiki: Option<OkfWikiJson>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct OkfWikiJson {
    enabled: Option<bool>,
    remote: Option<String>,
    local: Option<String>,
    subdir: Option<String>,
    commit_message: Option<String>,
}

impl OkfConfig {
    pub fn load(project_root: &Path) -> Self {
        let mut cfg = Self::default();
        let Some(raw) = read_okf_section(project_root) else {
            return cfg;
        };
        if let Some(v) = raw.enabled {
            cfg.enabled = v;
        }
        if let Some(v) = raw.out_dir.filter(|s| !s.trim().is_empty()) {
            cfg.out_dir = normalize_rel(&v);
        }
        if let Some(v) = raw.auto_export_on_sync {
            cfg.auto_export_on_sync = v;
        }
        if let Some(v) = raw.kinds {
            cfg.kinds = v
                .into_iter()
                .map(|s| s.trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .collect();
        }
        if let Some(w) = raw.azdo_wiki {
            if let Some(v) = w.enabled {
                cfg.azdo_wiki.enabled = v;
            }
            if let Some(v) = w.remote {
                cfg.azdo_wiki.remote = v;
            }
            if let Some(v) = w.local.filter(|s| !s.trim().is_empty()) {
                cfg.azdo_wiki.local = normalize_rel(&v);
            }
            if let Some(v) = w.subdir.filter(|s| !s.trim().is_empty()) {
                cfg.azdo_wiki.subdir = normalize_rel(&v);
            }
            if let Some(v) = w.commit_message.filter(|s| !s.trim().is_empty()) {
                cfg.azdo_wiki.commit_message = v;
            }
        }
        cfg
    }

    pub fn out_dir_abs(&self, project_root: &Path) -> PathBuf {
        project_root.join(&self.out_dir)
    }

    pub fn wiki_local_abs(&self, project_root: &Path) -> PathBuf {
        project_root.join(&self.azdo_wiki.local)
    }
}

fn read_okf_section(project_root: &Path) -> Option<OkfJson> {
    for name in ["ax.json", ".ax.json"] {
        let path = project_root.join(name);
        let content = std::fs::read_to_string(&path).ok()?;
        let root: AxJsonRoot = serde_json::from_str(&content).ok()?;
        if root.okf.is_some() {
            return root.okf;
        }
    }
    None
}

fn normalize_rel(s: &str) -> String {
    s.replace('\\', "/").trim_matches('/').to_string()
}

/// Result of writing an Open Knowledge Format (OKF) bundle.
#[derive(Debug, Clone)]
pub struct OkfExportReport {
    pub out_dir: PathBuf,
    pub exported: usize,
    pub by_kind: BTreeMap<String, usize>,
}

/// Options for OKF export.
#[derive(Debug, Clone, Default)]
pub struct OkfExportOptions {
    /// Override config `outDir` (absolute or relative to project root).
    pub out: Option<PathBuf>,
    /// Max concepts (0 = all).
    pub limit: usize,
}

pub fn export_okf_bundle(
    project_root: &Path,
    nodes: &[Node],
    edges: &[Edge],
    opts: &OkfExportOptions,
) -> Result<OkfExportReport, String> {
    let cfg = OkfConfig::load(project_root);
    if !cfg.enabled {
        return Err(
            "Open Knowledge Format (OKF) export is disabled in ax.json (okf.enabled=false)"
                .into(),
        );
    }

    let out_dir = match &opts.out {
        Some(p) if p.is_absolute() => p.clone(),
        Some(p) => project_root.join(p),
        None => cfg.out_dir_abs(project_root),
    };

    let by_id: HashMap<&str, &Node> = nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let mut calls: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut called_by: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in edges {
        if e.kind != EdgeKind::Calls {
            continue;
        }
        calls.entry(e.source.as_str()).or_default().push(e.target.as_str());
        called_by
            .entry(e.target.as_str())
            .or_default()
            .push(e.source.as_str());
    }

    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;

    let kind_filter: HashSet<String> = cfg.kinds.iter().cloned().collect();
    let mut symbols: Vec<&Node> = nodes
        .iter()
        .filter(|n| !matches!(n.kind, NodeKind::File | NodeKind::Doc))
        .filter(|n| {
            kind_filter.is_empty() || kind_filter.contains(&n.kind.as_str().to_ascii_lowercase())
        })
        .collect();
    symbols.sort_by(|a, b| {
        a.kind
            .as_str()
            .cmp(b.kind.as_str())
            .then_with(|| a.qualified_name.cmp(&b.qualified_name))
    });
    if opts.limit > 0 {
        symbols.truncate(opts.limit);
    }

    let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
    let mut exported = 0usize;

    for n in &symbols {
        let rel = concept_rel_path(n);
        let dest = out_dir.join(&rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!("create OKF dir {}: {e}", parent.display())
            })?;
        }
        let body = render_concept(
            n,
            &by_id,
            calls.get(n.id.as_str()),
            called_by.get(n.id.as_str()),
        );
        std::fs::write(&dest, body).map_err(|e| {
            format!("write OKF page {}: {e}", dest.display())
        })?;
        *by_kind.entry(n.kind.as_str().to_string()).or_default() += 1;
        exported += 1;
    }

    let index = render_index(exported, &by_kind, &cfg.out_dir);
    std::fs::write(out_dir.join("index.md"), &index).map_err(|e| e.to_string())?;
    std::fs::write(out_dir.join("README.md"), &index).map_err(|e| e.to_string())?;

    Ok(OkfExportReport {
        out_dir,
        exported,
        by_kind,
    })
}

fn render_index(exported: usize, by_kind: &BTreeMap<String, usize>, out_rel: &str) -> String {
    let mut out = String::from("# Open Knowledge Format (OKF) bundle\n\n");
    out.push_str(&format!(
        "Exported **{exported}** concepts into `{out_rel}`.\n\n"
    ));
    out.push_str("Generated by ax as a portable OKF Markdown projection of the indexed graph.\n\n");
    if !by_kind.is_empty() {
        out.push_str("## By kind\n\n");
        for (kind, count) in by_kind {
            out.push_str(&format!("- `{kind}`: {count}\n"));
        }
        out.push('\n');
    }
    out
}

fn concept_rel_path(n: &Node) -> PathBuf {
    PathBuf::from(n.kind.as_str()).join(format!("{}.md", sanitize(&n.qualified_name)))
}

/// Max stem length for concept filenames (Windows component limit is 255).
const MAX_CONCEPT_STEM: usize = 180;

fn sanitize(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '<' | '>' | '|' | '?' | '*' | '"' => '_',
            c if c.is_control() || c.is_whitespace() => '_',
            c => c,
        })
        .collect();
    while out.ends_with(['.', ' ', '_']) {
        out.pop();
    }
    if out.is_empty() {
        out = "unnamed".into();
    }
    if is_windows_reserved_stem(&out) {
        out = format!("_{out}");
    }
    if out.chars().count() > MAX_CONCEPT_STEM {
        let hash = short_hash(s);
        let keep = MAX_CONCEPT_STEM.saturating_sub(1 + hash.len());
        let prefix: String = out.chars().take(keep).collect();
        out = format!("{prefix}_{hash}");
    }
    out
}

fn short_hash(s: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    format!("{:08x}", hasher.finish() as u32)
}

fn is_windows_reserved_stem(stem: &str) -> bool {
    let upper = stem.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

fn render_concept(
    n: &Node,
    by_id: &HashMap<&str, &Node>,
    calls: Option<&Vec<&str>>,
    called_by: Option<&Vec<&str>>,
) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("id: {}\n", yaml_escape(&n.id)));
    out.push_str(&format!("type: {}\n", n.kind.as_str()));
    out.push_str(&format!("title: {}\n", yaml_escape(&n.name)));
    out.push_str(&format!(
        "resource: {}#L{}-L{}\n",
        yaml_escape(&n.file_path),
        n.start_line,
        n.end_line
    ));
    out.push_str("generated:\n");
    out.push_str(&format!("  by: ax/{}\n", env!("CARGO_PKG_VERSION")));
    out.push_str("  format: Open Knowledge Format (OKF)\n");

    let call_ids = unique_sorted_ids(calls);
    let caller_ids = unique_sorted_ids(called_by);
    if !call_ids.is_empty() || !caller_ids.is_empty() {
        out.push_str("relationships:\n");
        if !call_ids.is_empty() {
            out.push_str("  Calls:\n");
            for id in &call_ids {
                out.push_str(&format!("    - {}\n", yaml_escape(id)));
            }
        }
        if !caller_ids.is_empty() {
            out.push_str("  CalledBy:\n");
            for id in &caller_ids {
                out.push_str(&format!("    - {}\n", yaml_escape(id)));
            }
        }
    }
    out.push_str("---\n\n");

    if let Some(sig) = &n.signature {
        out.push_str("## Signature\n\n");
        out.push_str(&format!("`{sig}`\n\n"));
    }
    out.push_str("## Calls\n\n");
    append_link_list(&mut out, calls, by_id);
    out.push('\n');
    out.push_str("## Called by\n\n");
    append_link_list(&mut out, called_by, by_id);
    out.push('\n');
    out
}

fn unique_sorted_ids(ids: Option<&Vec<&str>>) -> Vec<String> {
    let Some(ids) = ids else {
        return Vec::new();
    };
    let mut v: Vec<String> = ids.iter().map(|s| (*s).to_string()).collect();
    v.sort();
    v.dedup();
    v
}

fn append_link_list(out: &mut String, ids: Option<&Vec<&str>>, by_id: &HashMap<&str, &Node>) {
    match ids {
        Some(ids) if !ids.is_empty() => {
            let mut names: Vec<(&str, &str)> = ids
                .iter()
                .filter_map(|id| {
                    by_id
                        .get(id)
                        .map(|n| (n.qualified_name.as_str(), n.kind.as_str()))
                })
                .collect();
            names.sort_by(|a, b| a.0.cmp(b.0));
            names.dedup_by(|a, b| a.0 == b.0);
            for (name, kind) in names {
                out.push_str(&format!(
                    "- [{name}](../{kind}/{}.md)\n",
                    sanitize(name)
                ));
            }
        }
        _ => out.push_str("_none_\n"),
    }
}

fn yaml_escape(s: &str) -> String {
    if s.chars().any(|c| {
        matches!(
            c,
            ':' | '#'
                | '{'
                | '}'
                | '['
                | ']'
                | ','
                | '&'
                | '*'
                | '?'
                | '|'
                | '>'
                | '!'
                | '%'
                | '@'
                | '`'
        )
    }) || s.contains('\n')
        || s.is_empty()
    {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

/// Validation report for an on-disk Open Knowledge Format (OKF) bundle.
#[derive(Debug, Clone, Default)]
pub struct OkfValidateReport {
    pub ok: bool,
    pub missing_index: bool,
    pub dangling_links: Vec<String>,
    pub pages: usize,
}

pub fn validate_okf_bundle(out_dir: &Path) -> Result<OkfValidateReport, String> {
    if !out_dir.is_dir() {
        return Err(format!(
            "OKF bundle directory missing: {}",
            out_dir.display()
        ));
    }
    let mut report = OkfValidateReport::default();
    report.missing_index = !out_dir.join("index.md").is_file();

    let mut pages = Vec::new();
    collect_md_files(out_dir, out_dir, &mut pages)?;
    report.pages = pages
        .iter()
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            name != "index.md" && name != "README.md"
        })
        .count();

    for page in &pages {
        let rel = page
            .strip_prefix(out_dir)
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let content = std::fs::read_to_string(page).map_err(|e| e.to_string())?;
        let from_dir = page.parent().unwrap_or(out_dir);
        for target in extract_md_links(&content) {
            if target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with('#')
                || target.starts_with("mailto:")
            {
                continue;
            }
            let clean = target.split('#').next().unwrap_or(target.as_str());
            if clean.is_empty() {
                continue;
            }
            let resolved = from_dir.join(clean);
            if !resolved.is_file() {
                report.dangling_links.push(format!("{rel} → {target}"));
            }
        }
    }

    report.dangling_links.sort();
    report.dangling_links.dedup();
    if report.dangling_links.len() > 100 {
        report.dangling_links.truncate(100);
    }
    report.ok = !report.missing_index && report.dangling_links.is_empty();
    Ok(report)
}

fn collect_md_files(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            collect_md_files(root, &path, out)?;
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        {
            out.push(path);
        }
    }
    Ok(())
}

fn extract_md_links(content: &str) -> Vec<String> {
    let mut links = Vec::new();
    let bytes = content.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            if let Some(close) = content[i..].find("](") {
                let start = i + close + 2;
                if let Some(end_rel) = content[start..].find(')') {
                    let target = &content[start..start + end_rel];
                    links.push(target.to_string());
                    i = start + end_rel + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    links
}

/// Publish an OKF bundle into a git wiki clone (Azure DevOps Wiki or any git remote).
#[derive(Debug, Clone, Default)]
pub struct OkfPublishOptions {
    pub dry_run: bool,
    /// Skip git push (still commit locally when not dry_run).
    pub no_push: bool,
}

#[derive(Debug, Clone)]
pub struct OkfPublishReport {
    pub wiki_action: String,
    pub subdir: PathBuf,
    pub files_copied: usize,
    pub committed: bool,
    pub pushed: bool,
    pub dry_run: bool,
}

pub fn publish_okf_wiki(
    project_root: &Path,
    bundle_dir: &Path,
    opts: &OkfPublishOptions,
) -> Result<OkfPublishReport, String> {
    let cfg = OkfConfig::load(project_root);
    if !cfg.azdo_wiki.enabled && opts.dry_run {
        // allow dry-run preview even when disabled, but warn via action string
    } else if !cfg.azdo_wiki.enabled {
        return Err(
            "OKF wiki publish is disabled. Set ax.json okf.azdoWiki.enabled=true and okf.azdoWiki.remote"
                .into(),
        );
    }
    if cfg.azdo_wiki.remote.trim().is_empty() {
        return Err(
            "okf.azdoWiki.remote is empty — set a git URL for the Azure DevOps Wiki (or any wiki remote)"
                .into(),
        );
    }
    if !bundle_dir.is_dir() {
        return Err(format!(
            "OKF bundle missing at {} — run ax export okf first",
            bundle_dir.display()
        ));
    }

    let wiki_local = cfg.wiki_local_abs(project_root);
    let dest = wiki_local.join(&cfg.azdo_wiki.subdir);

    if opts.dry_run {
        let mut files_copied = 0usize;
        count_md_files(bundle_dir, &mut files_copied)?;
        return Ok(OkfPublishReport {
            wiki_action: "dry-run".into(),
            subdir: dest,
            files_copied,
            committed: false,
            pushed: false,
            dry_run: true,
        });
    }

    let wiki_action = sync_wiki_repo(&cfg.azdo_wiki.remote, &wiki_local)?;
    if dest.exists() {
        std::fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    let files_copied = copy_dir_md(bundle_dir, &dest)?;

    let status = Command::new("git")
        .current_dir(&wiki_local)
        .args(["add", "-A", &cfg.azdo_wiki.subdir])
        .status()
        .map_err(|e| format!("git add failed: {e}"))?;
    if !status.success() {
        return Err("git add failed for OKF wiki publish".into());
    }

    let diff = Command::new("git")
        .current_dir(&wiki_local)
        .args(["diff", "--cached", "--quiet"])
        .status()
        .map_err(|e| format!("git diff failed: {e}"))?;
    let has_changes = !diff.success();

    let mut committed = false;
    if has_changes {
        let status = Command::new("git")
            .current_dir(&wiki_local)
            .args(["commit", "-m", &cfg.azdo_wiki.commit_message])
            .status()
            .map_err(|e| format!("git commit failed: {e}"))?;
        if !status.success() {
            return Err("git commit failed for OKF wiki publish".into());
        }
        committed = true;
    }

    let mut pushed = false;
    if committed && !opts.no_push {
        let status = Command::new("git")
            .current_dir(&wiki_local)
            .args(["push"])
            .status()
            .map_err(|e| format!("git push failed: {e}"))?;
        if !status.success() {
            return Err(
                "git push failed — check credentials for the wiki remote (no secrets stored in ax)"
                    .into(),
            );
        }
        pushed = true;
    }

    Ok(OkfPublishReport {
        wiki_action,
        subdir: dest,
        files_copied,
        committed,
        pushed,
        dry_run: false,
    })
}

fn sync_wiki_repo(remote: &str, local: &Path) -> Result<String, String> {
    if !local.exists() {
        if let Some(parent) = local.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let status = Command::new("git")
            .args(["clone", remote])
            .arg(local)
            .status()
            .map_err(|e| format!("git clone failed: {e}"))?;
        if !status.success() {
            return Err(format!("git clone failed for {remote}"));
        }
        return Ok("cloned".into());
    }
    let output = Command::new("git")
        .current_dir(local)
        .args(["pull", "--ff-only"])
        .output()
        .map_err(|e| format!("git pull failed: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git pull failed: {stderr}"));
    }
    Ok("pulled".into())
}

fn copy_dir_md(src: &Path, dest: &Path) -> Result<usize, String> {
    let mut count = 0usize;
    for entry in walkdir_simple(src)? {
        let rel = entry
            .strip_prefix(src)
            .map_err(|e| e.to_string())?
            .to_path_buf();
        let target = dest.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::copy(&entry, &target).map_err(|e| e.to_string())?;
            count += 1;
        }
    }
    Ok(count)
}

fn count_md_files(dir: &Path, count: &mut usize) -> Result<(), String> {
    for entry in walkdir_simple(dir)? {
        if entry.is_file() {
            *count += 1;
        }
    }
    Ok(())
}

fn walkdir_simple(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
        for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            out.push(path.clone());
            if path.is_dir() {
                walk(&path, out)?;
            }
        }
        Ok(())
    }
    walk(root, &mut out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ax_types::Language;

    fn sample_node(id: &str, name: &str, kind: NodeKind) -> Node {
        Node {
            id: id.into(),
            kind,
            name: name.into(),
            qualified_name: format!("demo::{name}"),
            file_path: "src/demo.rs".into(),
            language: Language::Rust,
            start_line: 1,
            end_line: 10,
            start_column: 0,
            end_column: 0,
            docstring: None,
            signature: Some(format!("fn {name}()")),
            visibility: None,
            is_exported: Some(true),
            is_async: None,
            is_static: None,
            is_abstract: None,
            decorators: None,
            type_parameters: None,
            return_type: None,
            updated_at: 0,
        }
    }

    #[test]
    fn export_and_validate_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::write(
            root.join("ax.json"),
            r#"{"okf":{"outDir":"knowledge","enabled":true}}"#,
        )
        .unwrap();

        let a = sample_node("fn:a", "alpha", NodeKind::Function);
        let b = sample_node("fn:b", "beta", NodeKind::Function);
        let nodes = vec![a.clone(), b.clone()];
        let edges = vec![Edge {
            source: a.id.clone(),
            target: b.id.clone(),
            kind: EdgeKind::Calls,
            metadata: None,
            line: Some(2),
            column: None,
            provenance: None,
            confidence: None,
        }];

        let report =
            export_okf_bundle(root, &nodes, &edges, &OkfExportOptions::default()).unwrap();
        assert_eq!(report.exported, 2);
        assert!(report.out_dir.join("index.md").is_file());

        let v = validate_okf_bundle(&report.out_dir).unwrap();
        assert!(v.ok, "dangling={:?}", v.dangling_links);
        assert_eq!(v.pages, 2);
    }

    #[test]
    fn config_loads_relative_out_dir() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("ax.json"),
            r#"{"okf":{"outDir":"docs/okf","azdoWiki":{"enabled":false,"remote":""}}}"#,
        )
        .unwrap();
        let cfg = OkfConfig::load(tmp.path());
        assert_eq!(cfg.out_dir, "docs/okf");
        assert!(!cfg.azdo_wiki.enabled);
    }

    #[test]
    fn publish_dry_run_requires_remote() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("ax.json"),
            r#"{"okf":{"azdoWiki":{"enabled":true,"remote":""}}}"#,
        )
        .unwrap();
        let bundle = tmp.path().join("knowledge");
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(bundle.join("index.md"), "# ok\n").unwrap();
        let err = publish_okf_wiki(
            tmp.path(),
            &bundle,
            &OkfPublishOptions {
                dry_run: true,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(err.contains("remote"));
    }

    #[test]
    fn sanitize_handles_multiline_and_long_names() {
        let long = format!("docs/examples/pipeline.yml::steps:\n{}\n", "x".repeat(400));
        let stem = sanitize(&long);
        assert!(stem.chars().count() <= MAX_CONCEPT_STEM);
        assert!(!stem.contains('\n'));
        assert!(!stem.contains('/'));
        assert_eq!(sanitize("CON"), "_CON");
        assert_eq!(sanitize("foo/bar:baz"), "foo_bar_baz");
        // Stable for the same input
        assert_eq!(sanitize(&long), stem);
    }

    #[test]
    fn export_survives_windows_hostile_qualified_names() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::write(
            root.join("ax.json"),
            r#"{"okf":{"outDir":"knowledge","enabled":true}}"#,
        )
        .unwrap();

        let mut n = sample_node("var:yaml", "steps", NodeKind::Variable);
        n.qualified_name = format!(
            "docs/examples/azure-pipelines-ship.yml::steps:\n{}\n- bash: |\n    ax ship --ci",
            "  - checkout: self\n".repeat(30)
        );
        n.name = n.qualified_name.clone();
        let report =
            export_okf_bundle(root, &[n], &[], &OkfExportOptions::default()).unwrap();
        assert_eq!(report.exported, 1);
        let pages: Vec<_> = std::fs::read_dir(report.out_dir.join("variable"))
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(pages.len(), 1);
        let name = pages[0].file_name().to_string_lossy().into_owned();
        assert!(name.ends_with(".md"));
        assert!(name.len() < 255);
    }
}
