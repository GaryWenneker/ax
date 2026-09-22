//! Portable Sitecore-style zip of selected `.agents` rules and skills.

use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read, Write};
use std::path::Path;

use serde::de::{self, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::agents_share::{agents_dir, is_git_export_candidate};
use crate::parse::{parse_rule_file, parse_skill_file};
use crate::paths::{RULES_DIR, SKILLS_DIR, SKILL_FILENAME};
use crate::types::PolicyScope;

pub const ZIP_PACKAGE_MAX_BYTES: usize = 8 * 1024 * 1024;
const KIND: &str = "ax-policy-package";
const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct PackSpec {
    pub name: String,
    pub description: String,
    pub rule_ids: Vec<String>,
    pub skill_names: Vec<String>,
    pub ax_version: String,
    pub package_version: Option<String>,
    pub author: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HunkTake {
    Local,
    Package,
    Both,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HunkPick {
    pub index: usize,
    pub take: HunkTake,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreAction {
    Overwrite,
    Skip,
    Merge {
        accept_hunks: Vec<usize>,
        hunks: Vec<HunkPick>,
    },
}

impl Serialize for RestoreAction {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            RestoreAction::Overwrite => serializer.serialize_str("overwrite"),
            RestoreAction::Skip => serializer.serialize_str("skip"),
            RestoreAction::Merge {
                accept_hunks,
                hunks,
            } => {
                #[derive(Serialize)]
                struct Obj<'a> {
                    action: &'static str,
                    #[serde(rename = "acceptHunks", skip_serializing_if = "<[usize]>::is_empty")]
                    accept_hunks: &'a [usize],
                    #[serde(skip_serializing_if = "<[HunkPick]>::is_empty")]
                    hunks: &'a [HunkPick],
                }
                Obj {
                    action: "merge",
                    accept_hunks,
                    hunks,
                }
                .serialize(serializer)
            }
        }
    }
}

impl<'de> Deserialize<'de> for RestoreAction {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Named(String),
            Obj {
                action: String,
                #[serde(default, rename = "acceptHunks")]
                accept_hunks: Vec<usize>,
                #[serde(default)]
                hunks: Vec<HunkPick>,
            },
        }
        match Raw::deserialize(deserializer)? {
            Raw::Named(s) if s == "overwrite" => Ok(Self::Overwrite),
            Raw::Named(s) if s == "skip" => Ok(Self::Skip),
            Raw::Named(s) => Err(de::Error::custom(format!("unknown restore action {s}"))),
            Raw::Obj {
                action,
                accept_hunks,
                hunks,
            } if action == "merge" => Ok(Self::Merge {
                accept_hunks,
                hunks,
            }),
            Raw::Obj { action, .. } => Err(de::Error::custom(format!("unknown restore action {action}"))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub kind: String,
    pub format_version: u32,
    pub name: String,
    pub description: String,
    pub created_at: String,
    pub ax_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub rules: Vec<ManifestPath>,
    pub skills: Vec<ManifestPath>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestPath {
    pub id: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtime: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "contentHash")]
    pub content_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewItem {
    pub kind: String,
    pub id: String,
    pub status: String,
    pub compare: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub newer: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZipPreview {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub items: Vec<PreviewItem>,
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    pub written: Vec<String>,
    pub skipped: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunk {
    pub index: usize,
    pub local: Vec<String>,
    pub package: Vec<String>,
    /// 1-based first local line in this hunk, or 0 when the hunk has no local lines.
    pub local_start: usize,
    /// 1-based first package line in this hunk, or 0 when the hunk has no package lines.
    pub package_start: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDiff {
    pub kind: String,
    pub id: String,
    pub compare: String,
    pub unified: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hunks: Vec<DiffHunk>,
}

#[derive(Debug)]
pub enum ZipPkgError {
    Empty,
    Unknown(Vec<String>),
    BadZip(String),
    TooLarge,
    Io(String),
}

impl std::fmt::Display for ZipPkgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "select at least one shareable rule or skill"),
            Self::Unknown(ids) => write!(f, "unknown or not shareable: {}", ids.join(", ")),
            Self::BadZip(m) => write!(f, "{m}"),
            Self::TooLarge => write!(f, "package exceeds {ZIP_PACKAGE_MAX_BYTES} bytes"),
            Self::Io(m) => write!(f, "{m}"),
        }
    }
}

impl From<std::io::Error> for ZipPkgError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

pub fn slug_package_filename(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    while s.contains("--") {
        s = s.replace("--", "-");
    }
    let s = s.trim_matches('-');
    let stem = if s.is_empty() { "package" } else { s };
    format!("{stem}.ax-policy.zip")
}

fn safe_item_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        && !id.contains("..")
}

fn shareable_scope_enabled(scope: &str, enabled: bool) -> bool {
    let scope = PolicyScope::parse(scope).unwrap_or(PolicyScope::Project);
    is_git_export_candidate(scope, enabled)
}

fn zip_opts() -> SimpleFileOptions {
    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated)
}

fn path_mtime_unix(path: &Path) -> Option<u64> {
    path.metadata()
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

fn content_hash_bytes(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn content_hash_mismatch(expected: &Option<String>, packaged: &[u8]) -> bool {
    expected
        .as_deref()
        .is_some_and(|h| h != content_hash_bytes(packaged))
}

pub fn default_restore_action(status: &str, newer: &str) -> Option<RestoreAction> {
    if status == "invalid" {
        return None;
    }
    if status == "new" {
        return Some(RestoreAction::Overwrite);
    }
    if newer == "local" {
        return Some(RestoreAction::Skip);
    }
    Some(RestoreAction::Skip)
}

pub fn build_policy_zip(project_root: &Path, spec: &PackSpec) -> Result<Vec<u8>, ZipPkgError> {
    if spec.name.trim().is_empty() {
        return Err(ZipPkgError::BadZip("package name is required".into()));
    }
    if spec.rule_ids.is_empty() && spec.skill_names.is_empty() {
        return Err(ZipPkgError::Empty);
    }
    let agents = agents_dir(project_root);
    let mut unknown = Vec::new();
    let mut rule_files: Vec<(String, Vec<u8>, Option<u64>)> = Vec::new();
    let mut skill_files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut seen_rules = HashSet::new();
    let mut seen_skills = HashSet::new();

    for id in &spec.rule_ids {
        let id = id.trim();
        if !safe_item_id(id) || !seen_rules.insert(id.to_string()) {
            unknown.push(id.to_string());
            continue;
        }
        let path = agents.join(RULES_DIR).join(format!("{id}.mdc"));
        match read_shareable_rule(&path) {
            Ok(bytes) => rule_files.push((id.to_string(), bytes, path_mtime_unix(&path))),
            Err(_) => unknown.push(id.to_string()),
        }
    }
    for name in &spec.skill_names {
        let name = name.trim();
        if !safe_item_id(name) || !seen_skills.insert(name.to_string()) {
            unknown.push(name.to_string());
            continue;
        }
        let dir = agents.join(SKILLS_DIR).join(name);
        match read_shareable_skill(&dir, name) {
            Ok(files) => skill_files.extend(files),
            Err(_) => unknown.push(name.to_string()),
        }
    }
    if !unknown.is_empty() {
        return Err(ZipPkgError::Unknown(unknown));
    }
    if rule_files.is_empty() && skill_files.is_empty() {
        return Err(ZipPkgError::Empty);
    }

    let mut rules_meta = Vec::new();
    for (id, bytes, mtime) in &rule_files {
        rules_meta.push(ManifestPath {
            id: id.clone(),
            path: format!("rules/{id}.mdc"),
            mtime: *mtime,
            content_hash: Some(content_hash_bytes(bytes)),
        });
    }
    let mut skill_ids: Vec<String> = skill_files
        .iter()
        .filter_map(|(p, _)| {
            let rest = p.strip_prefix("skills/")?;
            let name = rest.split('/').next()?;
            Some(name.to_string())
        })
        .collect();
    skill_ids.sort();
    skill_ids.dedup();
    let skills_meta: Vec<ManifestPath> = skill_ids
        .iter()
        .map(|n| {
            let skill_md = agents.join(SKILLS_DIR).join(n).join(SKILL_FILENAME);
            ManifestPath {
                id: n.clone(),
                path: format!("skills/{n}/{SKILL_FILENAME}"),
                mtime: path_mtime_unix(&skill_md),
                content_hash: skill_files
                    .iter()
                    .find(|(p, _)| *p == format!("skills/{n}/{SKILL_FILENAME}"))
                    .map(|(_, b)| content_hash_bytes(b)),
            }
        })
        .collect();

    let manifest = Manifest {
        kind: KIND.into(),
        format_version: FORMAT_VERSION,
        name: spec.name.trim().to_string(),
        description: spec.description.clone(),
        created_at: unix_secs_to_rfc3339(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        ),
        ax_version: spec.ax_version.clone(),
        package_version: spec.package_version.clone(),
        author: spec.author.clone(),
        rules: rules_meta,
        skills: skills_meta,
    };
    let manifest_json = serde_json::to_string_pretty(&manifest).map_err(|e| ZipPkgError::Io(e.to_string()))?;

    let mut cursor = Cursor::new(Vec::new());
    {
        let mut zw = ZipWriter::new(&mut cursor);
        zw.start_file("ax-package.json", zip_opts())
            .map_err(|e| ZipPkgError::Io(e.to_string()))?;
        zw.write_all(manifest_json.as_bytes())?;
        for (id, bytes, _) in &rule_files {
            zw.start_file(format!("rules/{id}.mdc"), zip_opts())
                .map_err(|e| ZipPkgError::Io(e.to_string()))?;
            zw.write_all(bytes)?;
        }
        for (rel, bytes) in &skill_files {
            zw.start_file(rel, zip_opts())
                .map_err(|e| ZipPkgError::Io(e.to_string()))?;
            zw.write_all(bytes)?;
        }
        zw.finish().map_err(|e| ZipPkgError::Io(e.to_string()))?;
    }
    Ok(cursor.into_inner())
}

fn read_shareable_rule(path: &Path) -> Result<Vec<u8>, ()> {
    let raw = std::fs::read_to_string(path).map_err(|_| ())?;
    let doc = parse_rule_file(path, &raw).map_err(|_| ())?;
    if !shareable_scope_enabled(&doc.frontmatter.scope, doc.frontmatter.enabled) {
        return Err(());
    }
    Ok(raw.into_bytes())
}

fn read_shareable_skill(dir: &Path, name: &str) -> Result<Vec<(String, Vec<u8>)>, ()> {
    let skill_md = dir.join(SKILL_FILENAME);
    let raw = std::fs::read_to_string(&skill_md).map_err(|_| ())?;
    let doc = parse_skill_file(&skill_md, &raw).map_err(|_| ())?;
    if doc.frontmatter.name != name {
        return Err(());
    }
    if !shareable_scope_enabled(&doc.frontmatter.scope, doc.frontmatter.enabled) {
        return Err(());
    }
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Err(());
    }
    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry.path().strip_prefix(dir).map_err(|_| ())?;
        let rel_s = rel.to_string_lossy().replace('\\', "/");
        if rel_s.contains("..") {
            continue;
        }
        let bytes = std::fs::read(entry.path()).map_err(|_| ())?;
        out.push((format!("skills/{name}/{rel_s}"), bytes));
    }
    if out.is_empty() {
        return Err(());
    }
    Ok(out)
}

fn zip_entry_ok(name: &str) -> bool {
    let n = name.replace('\\', "/");
    if n.contains("..") || n.starts_with('/') || n.starts_with("./") {
        return false;
    }
    n == "ax-package.json" || n.starts_with("rules/") || n.starts_with("skills/")
}

pub fn preview_policy_zip(project_root: &Path, bytes: &[u8]) -> Result<ZipPreview, ZipPkgError> {
    if bytes.len() > ZIP_PACKAGE_MAX_BYTES {
        return Err(ZipPkgError::TooLarge);
    }
    let files = read_zip_map(bytes)?;
    let manifest = parse_manifest(&files)?;
    let agents = agents_dir(project_root);
    let mut items = Vec::new();
    for r in &manifest.rules {
        items.push(preview_rule(&agents, &files, r));
    }
    for s in &manifest.skills {
        items.push(preview_skill(&agents, &files, s));
    }
    Ok(ZipPreview {
        name: manifest.name,
        package_version: manifest.package_version,
        author: manifest.author,
        items,
    })
}

fn preview_rule(agents: &Path, files: &HashMap<String, Vec<u8>>, meta: &ManifestPath) -> PreviewItem {
    if !safe_item_id(&meta.id) || !zip_entry_ok(&meta.path) {
        return invalid_item("rule", &meta.id, "unsafe path");
    }
    let Some(packaged) = files.get(&meta.path) else {
        return invalid_item("rule", &meta.id, "missing file in zip");
    };
    if content_hash_mismatch(&meta.content_hash, packaged) {
        return invalid_item("rule", &meta.id, "contentHash mismatch");
    }
    let dest = agents.join(RULES_DIR).join(format!("{}.mdc", meta.id));
    let (status, compare, newer) = compare_local(&dest, packaged, meta.mtime);
    let summary = std::str::from_utf8(packaged).ok().map(|raw| match parse_rule_file(&dest, raw) {
        Ok(d) => summarize_item_description(&d.frontmatter.id, None, &d.body),
        Err(_) => summarize_item_description(&meta.id, None, raw),
    });
    PreviewItem {
        kind: "rule".into(),
        id: meta.id.clone(),
        status,
        compare,
        summary,
        reason: None,
        newer,
    }
}

fn preview_skill(agents: &Path, files: &HashMap<String, Vec<u8>>, meta: &ManifestPath) -> PreviewItem {
    if !safe_item_id(&meta.id) || !zip_entry_ok(&meta.path) {
        return invalid_item("skill", &meta.id, "unsafe path");
    }
    let Some(packaged) = files.get(&meta.path) else {
        return invalid_item("skill", &meta.id, "missing file in zip");
    };
    if content_hash_mismatch(&meta.content_hash, packaged) {
        return invalid_item("skill", &meta.id, "contentHash mismatch");
    }
    let dest = agents.join(SKILLS_DIR).join(&meta.id).join(SKILL_FILENAME);
    let (status, compare, newer) = compare_local(&dest, packaged, meta.mtime);
    let summary = std::str::from_utf8(packaged).ok().map(|raw| match parse_skill_file(&dest, raw) {
        Ok(d) => summarize_item_description(&meta.id, Some(d.frontmatter.description.as_str()), &d.body),
        Err(_) => summarize_item_description(&meta.id, None, raw),
    });
    PreviewItem {
        kind: "skill".into(),
        id: meta.id.clone(),
        status,
        compare,
        summary,
        reason: None,
        newer,
    }
}

pub(crate) fn summarize_item_description(id: &str, explicit: Option<&str>, body: &str) -> String {
    if let Some(s) = explicit {
        let t = s.trim();
        if !t.is_empty() {
            return clip_description(t);
        }
    }
    if let Some(from_body) = first_prose(body) {
        return from_body;
    }
    humanize_policy_id(id)
}

fn clip_description(text: &str) -> String {
    let t: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() <= 220 {
        t
    } else {
        let mut s: String = t.chars().take(217).collect();
        s = s.trim_end().to_string();
        s.push('…');
        s
    }
}

fn first_prose(body: &str) -> Option<String> {
    let mut para = String::new();
    for line in body.lines() {
        let mut t = line.trim();
        if t.is_empty() {
            if !para.is_empty() {
                break;
            }
            continue;
        }
        if t.starts_with("```") {
            continue;
        }
        t = t.trim_start_matches('#').trim();
        t = t.trim_start_matches('>').trim();
        if t.is_empty() {
            continue;
        }
        if !para.is_empty() {
            para.push(' ');
        }
        para.push_str(t);
        if para.len() > 220 {
            break;
        }
    }
    if para.is_empty() {
        None
    } else {
        Some(clip_description(&para))
    }
}

fn humanize_policy_id(id: &str) -> String {
    let t = id.replace('-', " ").replace('_', " ");
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() {
        id.to_string()
    } else {
        t
    }
}

fn invalid_item(kind: &str, id: &str, reason: &str) -> PreviewItem {
    PreviewItem {
        kind: kind.into(),
        id: id.into(),
        status: "invalid".into(),
        compare: "invalid".into(),
        summary: None,
        reason: Some(reason.into()),
        newer: "none".into(),
    }
}

fn compare_local(dest: &Path, packaged: &[u8], pack_mtime: Option<u64>) -> (String, String, String) {
    if !dest.is_file() {
        return ("new".into(), "new".into(), "none".into());
    }
    let local = std::fs::read(dest).unwrap_or_default();
    if local == packaged {
        return ("conflict".into(), "identical".into(), "equal".into());
    }
    let newer = match (path_mtime_unix(dest), pack_mtime) {
        (Some(local_m), Some(pack_m)) if local_m > pack_m => "local",
        (Some(local_m), Some(pack_m)) if pack_m > local_m => "package",
        (Some(_), Some(_)) => "equal",
        _ => "unknown",
    };
    ("conflict".into(), "changed".into(), newer.into())
}

fn lcs_ops<'a>(a: &[&'a str], b: &[&'a str]) -> Vec<(char, &'a str)> {
    let n = a.len();
    let m = b.len();
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in 0..n {
        for j in 0..m {
            dp[i + 1][j + 1] = if a[i] == b[j] {
                dp[i][j] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut ops = Vec::new();
    let mut i = n;
    let mut j = m;
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && a[i - 1] == b[j - 1] {
            ops.push((' ', a[i - 1]));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
            ops.push(('+', b[j - 1]));
            j -= 1;
        } else {
            ops.push(('-', a[i - 1]));
            i -= 1;
        }
    }
    ops.reverse();
    ops
}

fn join_lines(lines: &[String], trailing_nl: bool) -> String {
    let mut s = lines.join("\n");
    if trailing_nl && (s.is_empty() || !s.ends_with('\n')) {
        s.push('\n');
    }
    s
}

/// Consecutive insert/delete ops from the same LCS walk as `unified_diff`.
pub fn split_diff_hunks(old: &str, new: &str) -> Vec<DiffHunk> {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    if a == b {
        return Vec::new();
    }
    let ops = lcs_ops(&a, &b);
    let mut hunks = Vec::new();
    let mut i = 0;
    let mut local_line = 1usize;
    let mut package_line = 1usize;
    while i < ops.len() {
        if ops[i].0 == ' ' {
            local_line += 1;
            package_line += 1;
            i += 1;
            continue;
        }
        let local_start = local_line;
        let package_start = package_line;
        let mut local = Vec::new();
        let mut package = Vec::new();
        while i < ops.len() && ops[i].0 != ' ' {
            match ops[i].0 {
                '-' => {
                    local.push(ops[i].1.to_string());
                    local_line += 1;
                }
                '+' => {
                    package.push(ops[i].1.to_string());
                    package_line += 1;
                }
                _ => {}
            }
            i += 1;
        }
        hunks.push(DiffHunk {
            index: hunks.len(),
            local_start: if local.is_empty() { 0 } else { local_start },
            package_start: if package.is_empty() { 0 } else { package_start },
            local,
            package,
        });
    }
    hunks
}

pub fn merge_selected_hunks(old: &str, new: &str, accept: &[usize]) -> Result<String, ZipPkgError> {
    let n = split_diff_hunks(old, new).len();
    let takes = resolve_hunk_takes(n, accept, &[])?;
    merge_hunk_takes(old, new, &takes)
}

fn resolve_hunk_takes(
    hunk_count: usize,
    accept_hunks: &[usize],
    picks: &[HunkPick],
) -> Result<Vec<HunkTake>, ZipPkgError> {
    let mut takes = vec![HunkTake::Local; hunk_count];
    for &idx in accept_hunks {
        if idx >= hunk_count {
            return Err(ZipPkgError::BadZip(format!("invalid hunk index {idx}")));
        }
        takes[idx] = HunkTake::Package;
    }
    for pick in picks {
        if pick.index >= hunk_count {
            return Err(ZipPkgError::BadZip(format!("invalid hunk index {}", pick.index)));
        }
        takes[pick.index] = pick.take;
    }
    Ok(takes)
}

pub fn merge_hunk_takes(old: &str, new: &str, takes: &[HunkTake]) -> Result<String, ZipPkgError> {
    let hunks = split_diff_hunks(old, new);
    if takes.len() != hunks.len() {
        return Err(ZipPkgError::BadZip("hunk take count mismatch".into()));
    }
    if hunks.is_empty() {
        return Ok(old.to_string());
    }
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let ops = lcs_ops(&a, &b);
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    let mut hunk_i = 0;
    while i < ops.len() {
        if ops[i].0 == ' ' {
            out.push(ops[i].1.to_string());
            i += 1;
            continue;
        }
        let mut local = Vec::new();
        let mut package = Vec::new();
        while i < ops.len() && ops[i].0 != ' ' {
            match ops[i].0 {
                '-' => local.push(ops[i].1.to_string()),
                '+' => package.push(ops[i].1.to_string()),
                _ => {}
            }
            i += 1;
        }
        match takes[hunk_i] {
            HunkTake::Local => out.extend(local),
            HunkTake::Package => out.extend(package),
            HunkTake::Both => {
                out.extend(local);
                out.extend(package);
            }
            HunkTake::None => {}
        }
        hunk_i += 1;
    }
    let all_package = takes.iter().all(|t| *t == HunkTake::Package);
    let trailing = if all_package {
        new.ends_with('\n')
    } else {
        old.ends_with('\n')
    };
    Ok(join_lines(&out, trailing))
}

enum WritePlan {
    Skip,
    Bytes(Vec<u8>),
}

fn write_plan(
    dest: &Path,
    packaged: &[u8],
    action: &RestoreAction,
    key: &str,
    is_new: bool,
) -> Result<WritePlan, ZipPkgError> {
    match action {
        RestoreAction::Skip => Ok(WritePlan::Skip),
        RestoreAction::Overwrite => Ok(WritePlan::Bytes(packaged.to_vec())),
        RestoreAction::Merge {
            accept_hunks,
            hunks: picks,
        } => {
            if is_new || !dest.is_file() {
                if accept_hunks.is_empty() && picks.is_empty() {
                    return Ok(WritePlan::Skip);
                }
                return Ok(WritePlan::Bytes(packaged.to_vec()));
            }
            let local = std::fs::read_to_string(dest).unwrap_or_default();
            let pkg = String::from_utf8_lossy(packaged).into_owned();
            let hunks = split_diff_hunks(&local, &pkg);
            let takes = resolve_hunk_takes(hunks.len(), accept_hunks, picks).map_err(|e| match e {
                ZipPkgError::BadZip(m) => ZipPkgError::BadZip(format!("{key}: {m}")),
                other => other,
            })?;
            if takes.iter().all(|t| *t == HunkTake::Local) {
                return Ok(WritePlan::Skip);
            }
            if !hunks.is_empty() && takes.iter().all(|t| *t == HunkTake::Package) {
                return Ok(WritePlan::Bytes(packaged.to_vec()));
            }
            let merged = merge_hunk_takes(&local, &pkg, &takes)?;
            Ok(WritePlan::Bytes(merged.into_bytes()))
        }
    }
}

pub fn unified_diff(old: &str, new: &str, old_label: &str, new_label: &str) -> String {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    if a == b {
        return String::new();
    }
    let n = a.len();
    let m = b.len();
    let ops = lcs_ops(&a, &b);
    let mut out = format!("--- {old_label}\n+++ {new_label}\n");
    out.push_str(&format!("@@ -1,{} +1,{} @@\n", n.max(1), m.max(1)));
    for (op, line) in ops {
        out.push(op);
        out.push_str(line);
        out.push('\n');
    }
    out
}

pub fn diff_policy_zip_item(
    project_root: &Path,
    bytes: &[u8],
    kind: &str,
    id: &str,
) -> Result<ItemDiff, ZipPkgError> {
    if bytes.len() > ZIP_PACKAGE_MAX_BYTES {
        return Err(ZipPkgError::TooLarge);
    }
    let files = read_zip_map(bytes)?;
    let manifest = parse_manifest(&files)?;
    let agents = agents_dir(project_root);
    let (dest, zip_path, pack_mtime) = match kind {
        "rule" => {
            let meta = manifest
                .rules
                .iter()
                .find(|r| r.id == id)
                .ok_or_else(|| ZipPkgError::Unknown(vec![id.into()]))?;
            (
                agents.join(RULES_DIR).join(format!("{id}.mdc")),
                meta.path.clone(),
                meta.mtime,
            )
        }
        "skill" => {
            let meta = manifest
                .skills
                .iter()
                .find(|s| s.id == id)
                .ok_or_else(|| ZipPkgError::Unknown(vec![id.into()]))?;
            (
                agents.join(SKILLS_DIR).join(id).join(SKILL_FILENAME),
                meta.path.clone(),
                meta.mtime,
            )
        }
        _ => return Err(ZipPkgError::BadZip("kind must be rule or skill".into())),
    };
    let packaged = files
        .get(&zip_path)
        .ok_or_else(|| ZipPkgError::BadZip("missing file in zip".into()))?;
    let packaged_text = String::from_utf8_lossy(packaged).into_owned();
    let (status, compare, _newer) = compare_local(&dest, packaged, pack_mtime);
    let local_text = if dest.is_file() {
        std::fs::read_to_string(&dest).unwrap_or_default()
    } else {
        String::new()
    };
    let mut unified = if compare == "identical" {
        String::new()
    } else {
        unified_diff(&local_text, &packaged_text, "local", "package")
    };
    if unified.is_empty() && compare == "changed" {
        unified = "Files differ only in line endings or encoding.\n".into();
    }
    let _ = status;
    Ok(ItemDiff {
        kind: kind.into(),
        id: id.into(),
        compare,
        unified,
        hunks: split_diff_hunks(&local_text, &packaged_text),
    })
}

pub fn decision_key(kind: &str, id: &str) -> String {
    format!("{kind}:{id}")
}

pub fn restore_policy_zip(
    project_root: &Path,
    bytes: &[u8],
    decisions: &HashMap<String, RestoreAction>,
) -> Result<RestoreResult, ZipPkgError> {
    if bytes.len() > ZIP_PACKAGE_MAX_BYTES {
        return Err(ZipPkgError::TooLarge);
    }
    let files = read_zip_map(bytes)?;
    let manifest = parse_manifest(&files)?;
    let preview = preview_policy_zip(project_root, bytes)?;
    let agents = agents_dir(project_root);
    let mut result = RestoreResult::default();

    for item in &preview.items {
        let key = decision_key(&item.kind, &item.id);
        if item.status == "invalid" {
            result.errors.push(format!("{key}: {}", item.reason.clone().unwrap_or_else(|| "invalid".into())));
            continue;
        }
        let action = decisions.get(&key).cloned().unwrap_or_else(|| {
            default_restore_action(&item.status, &item.newer).unwrap_or(RestoreAction::Skip)
        });
        let action = match action {
            RestoreAction::Merge {
                accept_hunks,
                hunks,
            } if accept_hunks.is_empty() && hunks.is_empty() => RestoreAction::Skip,
            other => other,
        };
        if action == RestoreAction::Skip {
            result.skipped.push(key);
            continue;
        }
        match item.kind.as_str() {
            "rule" => {
                let meta = manifest.rules.iter().find(|r| r.id == item.id);
                let Some(meta) = meta else {
                    result.errors.push(format!("{key}: missing manifest"));
                    continue;
                };
                let Some(content) = files.get(&meta.path) else {
                    result.errors.push(format!("{key}: missing zip member"));
                    continue;
                };
                let dest = agents.join(RULES_DIR).join(format!("{}.mdc", item.id));
                match write_plan(&dest, content, &action, &key, item.status == "new")? {
                    WritePlan::Skip => result.skipped.push(key),
                    WritePlan::Bytes(bytes) => {
                        if let Some(p) = dest.parent() {
                            std::fs::create_dir_all(p)?;
                        }
                        std::fs::write(&dest, bytes)?;
                        result.written.push(key);
                    }
                }
            }
            "skill" => {
                let prefix = format!("skills/{}/", item.id);
                let dest_root = agents.join(SKILLS_DIR).join(&item.id);
                let merge_skill_md_only = matches!(&action, RestoreAction::Merge { .. })
                    && item.status != "new";
                if merge_skill_md_only {
                    let zip_path = format!("{prefix}{SKILL_FILENAME}");
                    let Some(content) = files.get(&zip_path) else {
                        result.errors.push(format!("{key}: missing zip member"));
                        continue;
                    };
                    let dest = dest_root.join(SKILL_FILENAME);
                    match write_plan(&dest, content, &action, &key, false)? {
                        WritePlan::Skip => result.skipped.push(key),
                        WritePlan::Bytes(bytes) => {
                            std::fs::create_dir_all(&dest_root)?;
                            std::fs::write(&dest, bytes)?;
                            result.written.push(key);
                        }
                    }
                    continue;
                }
                std::fs::create_dir_all(&dest_root)?;
                for (path, content) in &files {
                    if !path.starts_with(&prefix) {
                        continue;
                    }
                    let rel = &path[prefix.len()..];
                    if rel.is_empty() || rel.contains("..") {
                        continue;
                    }
                    let dest = dest_root.join(rel);
                    if let Some(p) = dest.parent() {
                        std::fs::create_dir_all(p)?;
                    }
                    std::fs::write(&dest, content)?;
                }
                result.written.push(key);
            }
            _ => result.errors.push(format!("{key}: unknown kind")),
        }
    }
    Ok(result)
}

fn read_zip_map(bytes: &[u8]) -> Result<HashMap<String, Vec<u8>>, ZipPkgError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|e| ZipPkgError::BadZip(e.to_string()))?;
    let mut files = HashMap::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| ZipPkgError::BadZip(e.to_string()))?;
        let name = file.name().replace('\\', "/");
        if name.ends_with('/') {
            continue;
        }
        if !zip_entry_ok(&name) {
            return Err(ZipPkgError::BadZip(format!("refused zip path: {name}")));
        }
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        files.insert(name, buf);
    }
    Ok(files)
}

fn parse_manifest(files: &HashMap<String, Vec<u8>>) -> Result<Manifest, ZipPkgError> {
    let raw = files
        .get("ax-package.json")
        .ok_or_else(|| ZipPkgError::BadZip("missing ax-package.json".into()))?;
    let text = std::str::from_utf8(raw).map_err(|_| ZipPkgError::BadZip("manifest is not UTF-8".into()))?;
    if text.starts_with('\u{feff}') {
        return Err(ZipPkgError::BadZip("manifest must be UTF-8 without BOM".into()));
    }
    let manifest: Manifest =
        serde_json::from_str(text).map_err(|e| ZipPkgError::BadZip(format!("manifest: {e}")))?;
    if manifest.kind != KIND {
        return Err(ZipPkgError::BadZip("not an ax-policy-package".into()));
    }
    if manifest.format_version != FORMAT_VERSION {
        return Err(ZipPkgError::BadZip(format!(
            "unsupported formatVersion {}",
            manifest.format_version
        )));
    }
    Ok(manifest)
}

fn unix_secs_to_rfc3339(secs: u64) -> String {
    let z = secs as i64;
    let days = z.div_euclid(86400);
    let tod = z.rem_euclid(86400) as u32;
    let h = tod / 3600;
    let m = (tod % 3600) / 60;
    let s = tod % 60;
    let (y, mo, d) = civil_from_days(days);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Howard Hinnant civil-from-days (Unix epoch day 0 = 1970-01-01).
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

fn write_rule_file(root: &Path, id: &str, enabled: bool, scope: &str) {
    let path = agents_dir(root).join(RULES_DIR).join(format!("{id}.mdc"));
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).unwrap();
    }
    let enabled_s = if enabled { "true" } else { "false" };
    std::fs::write(
        path,
        format!(
            "---\nid: {id}\nlevel: INFO\nalwaysApply: true\nenabled: {enabled_s}\nscope: \"{scope}\"\n---\n\nbody\n"
        ),
    )
    .unwrap();
}

fn write_skill_file(root: &Path, name: &str, enabled: bool, scope: &str, extra: Option<(&str, &str)>) {
    let dir = agents_dir(root).join(SKILLS_DIR).join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let enabled_s = if enabled { "true" } else { "false" };
    std::fs::write(
        dir.join(SKILL_FILENAME),
        format!(
            "---\nname: {name}\ndescription: demo\nenabled: {enabled_s}\nscope: \"{scope}\"\n---\n\nskill body\n"
        ),
    )
    .unwrap();
    if let Some((rel, content)) = extra {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, content).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(rules: &[&str], skills: &[&str]) -> PackSpec {
        PackSpec {
            name: "Team pack".into(),
            description: "d".into(),
            rule_ids: rules.iter().map(|s| (*s).to_string()).collect(),
            skill_names: skills.iter().map(|s| (*s).to_string()).collect(),
            ax_version: "4.6.0".into(),
            package_version: None,
            author: None,
        }
    }

    #[test]
    fn pack_includes_selected_and_skill_extras() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        write_rule_file(p, "alpha", true, "project");
        write_skill_file(p, "startup", true, "project", Some(("notes.md", "extra")));
        let zip = build_policy_zip(p, &spec(&["alpha"], &["startup"])).unwrap();
        let files = read_zip_map(&zip).unwrap();
        assert!(files.contains_key("ax-package.json"));
        assert!(files.contains_key("rules/alpha.mdc"));
        assert!(files.contains_key("skills/startup/SKILL.md"));
        assert!(files.contains_key("skills/startup/notes.md"));
        let man: Manifest = serde_json::from_slice(&files["ax-package.json"]).unwrap();
        assert_eq!(man.kind, "ax-policy-package");
        assert_eq!(man.format_version, 1);
        let dest = tempfile::tempdir().unwrap();
        let preview = preview_policy_zip(dest.path(), &zip).unwrap();
        let skill = preview.items.iter().find(|i| i.kind == "skill").unwrap();
        assert_eq!(skill.summary.as_deref(), Some("demo"));
        assert_eq!(skill.compare, "new");
        let rule = preview.items.iter().find(|i| i.kind == "rule").unwrap();
        assert_eq!(rule.summary.as_deref(), Some("body"));
    }

    #[test]
    fn pack_rejects_private_and_disabled_and_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        write_rule_file(p, "pub", true, "project");
        write_rule_file(p, "hid", true, "private_project");
        write_rule_file(p, "off", false, "project");
        let err = build_policy_zip(p, &spec(&["hid"], &[])).unwrap_err();
        assert!(matches!(err, ZipPkgError::Unknown(_)));
        let err = build_policy_zip(p, &spec(&["off"], &[])).unwrap_err();
        assert!(matches!(err, ZipPkgError::Unknown(_)));
        let err = build_policy_zip(p, &spec(&["missing"], &[])).unwrap_err();
        assert!(matches!(err, ZipPkgError::Unknown(_)));
        let err = build_policy_zip(p, &spec(&[], &[])).unwrap_err();
        assert!(matches!(err, ZipPkgError::Empty));
        assert!(build_policy_zip(p, &spec(&["pub"], &[])).is_ok());
    }

    #[test]
    fn preview_new_conflict_and_restore_skip_overwrite() {
        let src = tempfile::tempdir().unwrap();
        write_rule_file(src.path(), "alpha", true, "project");
        let zip = build_policy_zip(src.path(), &spec(&["alpha"], &[])).unwrap();

        let dest = tempfile::tempdir().unwrap();
        let preview = preview_policy_zip(dest.path(), &zip).unwrap();
        assert_eq!(preview.items[0].status, "new");
        assert_eq!(preview.items[0].compare, "new");
        restore_policy_zip(dest.path(), &zip, &HashMap::new()).unwrap();
        let dest_file = agents_dir(dest.path()).join("rules/alpha.mdc");
        assert!(dest_file.is_file());
        let preview = preview_policy_zip(dest.path(), &zip).unwrap();
        assert_eq!(preview.items[0].compare, "identical");

        std::fs::write(&dest_file, "LOCAL\n").unwrap();
        let preview = preview_policy_zip(dest.path(), &zip).unwrap();
        assert_eq!(preview.items[0].status, "conflict");
        assert_eq!(preview.items[0].compare, "changed");
        let diff = diff_policy_zip_item(dest.path(), &zip, "rule", "alpha").unwrap();
        assert_eq!(diff.compare, "changed");
        assert!(diff.unified.contains("-LOCAL"));
        assert!(diff.unified.contains("+---") || diff.unified.contains("id: alpha"));
        restore_policy_zip(dest.path(), &zip, &HashMap::new()).unwrap();
        assert_eq!(std::fs::read_to_string(&dest_file).unwrap(), "LOCAL\n");

        let mut dec = HashMap::new();
        dec.insert("rule:alpha".into(), RestoreAction::Overwrite);
        restore_policy_zip(dest.path(), &zip, &dec).unwrap();
        assert!(std::fs::read_to_string(&dest_file).unwrap().contains("id: alpha"));
    }

    #[test]
    fn unified_diff_marks_changed_line() {
        let d = unified_diff("a\nb\n", "a\nc\n", "local", "package");
        assert!(d.contains("--- local"));
        assert!(d.contains("+++ package"));
        assert!(d.contains("-b"));
        assert!(d.contains("+c"));
        assert!(unified_diff("same\n", "same\n", "a", "b").is_empty());
    }

    #[test]
    fn summarize_item_description_prefers_explicit_then_body_then_id() {
        assert_eq!(
            summarize_item_description("utf8-no-bom", Some("  UTF-8 without BOM  "), "ignored"),
            "UTF-8 without BOM"
        );
        assert_eq!(
            summarize_item_description(
                "english-only",
                None,
                "# English only\n\nAll agent-facing text MUST be English."
            ),
            "English only"
        );
        assert_eq!(summarize_item_description("utf8-no-bom", None, ""), "utf8 no bom");
    }

    #[test]
    fn zip_slip_and_bad_kind_rejected() {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zw = ZipWriter::new(&mut cursor);
            zw.start_file("../evil.mdc", zip_opts()).unwrap();
            zw.write_all(b"x").unwrap();
            zw.finish().unwrap();
        }
        let dir = tempfile::tempdir().unwrap();
        assert!(preview_policy_zip(dir.path(), &cursor.into_inner()).is_err());

        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zw = ZipWriter::new(&mut cursor);
            zw.start_file("ax-package.json", zip_opts()).unwrap();
            zw.write_all(br#"{"kind":"nope","formatVersion":1,"name":"n","description":"","createdAt":"t","axVersion":"1","rules":[],"skills":[]}"#).unwrap();
            zw.finish().unwrap();
        }
        assert!(preview_policy_zip(dir.path(), &cursor.into_inner()).is_err());
    }

    #[test]
    fn slug_and_rfc3339() {
        assert_eq!(slug_package_filename("Team Pack!"), "team-pack.ax-policy.zip");
        assert!(unix_secs_to_rfc3339(0).starts_with("1970-01-01T00:00:00Z"));
    }

    #[test]
    fn preview_new_has_newer_none() {
        let src = tempfile::tempdir().unwrap();
        write_rule_file(src.path(), "alpha", true, "project");
        let zip = build_policy_zip(src.path(), &spec(&["alpha"], &[])).unwrap();
        let dest = tempfile::tempdir().unwrap();
        let preview = preview_policy_zip(dest.path(), &zip).unwrap();
        assert_eq!(preview.items[0].status, "new");
        assert_eq!(preview.items[0].newer, "none");
    }

    #[test]
    fn restore_new_honors_skip() {
        let src = tempfile::tempdir().unwrap();
        write_rule_file(src.path(), "alpha", true, "project");
        let zip = build_policy_zip(src.path(), &spec(&["alpha"], &[])).unwrap();
        let dest = tempfile::tempdir().unwrap();
        let mut dec = HashMap::new();
        dec.insert("rule:alpha".into(), RestoreAction::Skip);
        restore_policy_zip(dest.path(), &zip, &dec).unwrap();
        let dest_file = agents_dir(dest.path()).join("rules/alpha.mdc");
        assert!(!dest_file.is_file());
    }

    #[test]
    fn restore_new_default_installs() {
        let src = tempfile::tempdir().unwrap();
        write_rule_file(src.path(), "alpha", true, "project");
        let zip = build_policy_zip(src.path(), &spec(&["alpha"], &[])).unwrap();
        let dest = tempfile::tempdir().unwrap();
        restore_policy_zip(dest.path(), &zip, &HashMap::new()).unwrap();
        assert!(agents_dir(dest.path()).join("rules/alpha.mdc").is_file());
    }

    fn set_mtime(path: &Path, unix: u64) {
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(unix);
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(t)
            .unwrap();
    }

    #[test]
    fn preview_changed_local_newer() {
        let src = tempfile::tempdir().unwrap();
        write_rule_file(src.path(), "alpha", true, "project");
        let zip = build_policy_zip(src.path(), &spec(&["alpha"], &[])).unwrap();
        let dest = tempfile::tempdir().unwrap();
        restore_policy_zip(dest.path(), &zip, &HashMap::new()).unwrap();
        let dest_file = agents_dir(dest.path()).join("rules/alpha.mdc");
        std::fs::write(&dest_file, "LOCAL\n").unwrap();
        set_mtime(&dest_file, 2_000_000_000);
        let preview = preview_policy_zip(dest.path(), &zip).unwrap();
        assert_eq!(preview.items[0].compare, "changed");
        assert_eq!(preview.items[0].newer, "local");
        restore_policy_zip(dest.path(), &zip, &HashMap::new()).unwrap();
        assert_eq!(std::fs::read_to_string(&dest_file).unwrap(), "LOCAL\n");
    }

    #[test]
    fn preview_changed_package_newer() {
        let src = tempfile::tempdir().unwrap();
        write_rule_file(src.path(), "alpha", true, "project");
        let src_file = agents_dir(src.path()).join("rules/alpha.mdc");
        set_mtime(&src_file, 2_000_000_000);
        let zip = build_policy_zip(src.path(), &spec(&["alpha"], &[])).unwrap();
        let dest = tempfile::tempdir().unwrap();
        restore_policy_zip(dest.path(), &zip, &HashMap::new()).unwrap();
        let dest_file = agents_dir(dest.path()).join("rules/alpha.mdc");
        std::fs::write(&dest_file, "LOCAL\n").unwrap();
        set_mtime(&dest_file, 1_000);
        let preview = preview_policy_zip(dest.path(), &zip).unwrap();
        assert_eq!(preview.items[0].compare, "changed");
        assert_eq!(preview.items[0].newer, "package");
        restore_policy_zip(dest.path(), &zip, &HashMap::new()).unwrap();
        assert_eq!(std::fs::read_to_string(&dest_file).unwrap(), "LOCAL\n");
    }

    #[test]
    fn restore_explicit_overwrite_when_local_newer() {
        let src = tempfile::tempdir().unwrap();
        write_rule_file(src.path(), "alpha", true, "project");
        let zip = build_policy_zip(src.path(), &spec(&["alpha"], &[])).unwrap();
        let dest = tempfile::tempdir().unwrap();
        restore_policy_zip(dest.path(), &zip, &HashMap::new()).unwrap();
        let dest_file = agents_dir(dest.path()).join("rules/alpha.mdc");
        std::fs::write(&dest_file, "LOCAL\n").unwrap();
        set_mtime(&dest_file, 2_000_000_000);
        let mut dec = HashMap::new();
        dec.insert("rule:alpha".into(), RestoreAction::Overwrite);
        restore_policy_zip(dest.path(), &zip, &dec).unwrap();
        assert!(std::fs::read_to_string(&dest_file).unwrap().contains("id: alpha"));
    }

    #[test]
    fn preview_legacy_zip_without_mtime_is_unknown() {
        let src = tempfile::tempdir().unwrap();
        write_rule_file(src.path(), "alpha", true, "project");
        let zip = build_policy_zip(src.path(), &spec(&["alpha"], &[])).unwrap();
        let files = read_zip_map(&zip).unwrap();
        let mut man: Manifest = serde_json::from_slice(&files["ax-package.json"]).unwrap();
        for r in &mut man.rules {
            r.mtime = None;
        }
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zw = ZipWriter::new(&mut cursor);
            zw.start_file("ax-package.json", zip_opts()).unwrap();
            zw.write_all(serde_json::to_string(&man).unwrap().as_bytes()).unwrap();
            zw.start_file("rules/alpha.mdc", zip_opts()).unwrap();
            zw.write_all(&files["rules/alpha.mdc"]).unwrap();
            zw.finish().unwrap();
        }
        let legacy = cursor.into_inner();
        let dest = tempfile::tempdir().unwrap();
        restore_policy_zip(dest.path(), &legacy, &HashMap::new()).unwrap();
        let dest_file = agents_dir(dest.path()).join("rules/alpha.mdc");
        std::fs::write(&dest_file, "LOCAL\n").unwrap();
        let preview = preview_policy_zip(dest.path(), &legacy).unwrap();
        assert_eq!(preview.items[0].compare, "changed");
        assert_eq!(preview.items[0].newer, "unknown");
        restore_policy_zip(dest.path(), &legacy, &HashMap::new()).unwrap();
        assert_eq!(std::fs::read_to_string(&dest_file).unwrap(), "LOCAL\n");
    }

    fn rewrite_zip(man: &Manifest, files: &HashMap<String, Vec<u8>>) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zw = ZipWriter::new(&mut cursor);
            zw.start_file("ax-package.json", zip_opts()).unwrap();
            zw.write_all(serde_json::to_string(man).unwrap().as_bytes()).unwrap();
            for (name, bytes) in files {
                if name == "ax-package.json" {
                    continue;
                }
                zw.start_file(name, zip_opts()).unwrap();
                zw.write_all(bytes).unwrap();
            }
            zw.finish().unwrap();
        }
        cursor.into_inner()
    }

    #[test]
    fn pack_writes_blake3_content_hash() {
        let src = tempfile::tempdir().unwrap();
        write_rule_file(src.path(), "alpha", true, "project");
        let zip = build_policy_zip(src.path(), &spec(&["alpha"], &[])).unwrap();
        let files = read_zip_map(&zip).unwrap();
        let man: Manifest = serde_json::from_slice(&files["ax-package.json"]).unwrap();
        let expected = content_hash_bytes(&files["rules/alpha.mdc"]);
        assert_eq!(man.rules[0].content_hash.as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn preview_invalid_when_content_hash_mismatches() {
        let src = tempfile::tempdir().unwrap();
        write_rule_file(src.path(), "alpha", true, "project");
        let zip = build_policy_zip(src.path(), &spec(&["alpha"], &[])).unwrap();
        let files = read_zip_map(&zip).unwrap();
        let mut man: Manifest = serde_json::from_slice(&files["ax-package.json"]).unwrap();
        man.rules[0].content_hash = Some("0".repeat(64));
        let tampered = rewrite_zip(&man, &files);
        let dest = tempfile::tempdir().unwrap();
        let preview = preview_policy_zip(dest.path(), &tampered).unwrap();
        assert_eq!(preview.items[0].status, "invalid");
        assert_eq!(preview.items[0].reason.as_deref(), Some("contentHash mismatch"));
    }

    #[test]
    fn preview_legacy_zip_without_content_hash_still_previews() {
        let src = tempfile::tempdir().unwrap();
        write_rule_file(src.path(), "alpha", true, "project");
        let zip = build_policy_zip(src.path(), &spec(&["alpha"], &[])).unwrap();
        let files = read_zip_map(&zip).unwrap();
        let mut man: Manifest = serde_json::from_slice(&files["ax-package.json"]).unwrap();
        man.rules[0].content_hash = None;
        let legacy = rewrite_zip(&man, &files);
        let dest = tempfile::tempdir().unwrap();
        let preview = preview_policy_zip(dest.path(), &legacy).unwrap();
        assert_eq!(preview.items[0].status, "new");
        assert_eq!(preview.items[0].compare, "new");
        assert!(preview.items[0].reason.is_none());
    }

    #[test]
    fn diff_notes_line_endings_when_bytes_differ_but_lines_match() {
        let src = tempfile::tempdir().unwrap();
        write_rule_file(src.path(), "alpha", true, "project");
        let zip = build_policy_zip(src.path(), &spec(&["alpha"], &[])).unwrap();
        let dest = tempfile::tempdir().unwrap();
        restore_policy_zip(dest.path(), &zip, &HashMap::new()).unwrap();
        let dest_file = agents_dir(dest.path()).join("rules/alpha.mdc");
        let lf = std::fs::read_to_string(&dest_file).unwrap();
        std::fs::write(&dest_file, lf.replace('\n', "\r\n")).unwrap();
        let diff = diff_policy_zip_item(dest.path(), &zip, "rule", "alpha").unwrap();
        assert_eq!(diff.compare, "changed");
        assert!(diff.unified.contains("line endings or encoding"));
    }

    #[test]
    fn split_lcs_into_hunks_separates_distant_edits() {
        let old = "a\nb\nc\nd\ne\n";
        let new = "a\nX\nc\nY\ne\n";
        let hunks = split_diff_hunks(old, new);
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[0].index, 0);
        assert_eq!(hunks[0].local, vec!["b"]);
        assert_eq!(hunks[0].package, vec!["X"]);
        assert_eq!(hunks[0].local_start, 2);
        assert_eq!(hunks[0].package_start, 2);
        assert_eq!(hunks[1].index, 1);
        assert_eq!(hunks[1].local, vec!["d"]);
        assert_eq!(hunks[1].package, vec!["Y"]);
        assert_eq!(hunks[1].local_start, 4);
        assert_eq!(hunks[1].package_start, 4);
    }

    #[test]
    fn merge_accepts_first_hunk_keeps_second_local() {
        let old = "a\nb\nc\nd\ne\n";
        let new = "a\nX\nc\nY\ne\n";
        let merged = merge_selected_hunks(old, new, &[0]).unwrap();
        assert_eq!(merged, "a\nX\nc\nd\ne\n");
        let none = merge_selected_hunks(old, new, &[]).unwrap();
        assert_eq!(none, old);
        let all = merge_selected_hunks(old, new, &[0, 1]).unwrap();
        assert_eq!(all, new);
        let err = merge_selected_hunks(old, new, &[2]).unwrap_err();
        assert!(err.to_string().contains("hunk"));
        let both = merge_hunk_takes(
            old,
            new,
            &[HunkTake::Both, HunkTake::Local],
        )
        .unwrap();
        assert_eq!(both, "a\nb\nX\nc\nd\ne\n");
    }

    fn write_two_hunk_rule(root: &Path, id: &str, mid1: &str, mid2: &str) {
        let path = agents_dir(root).join(RULES_DIR).join(format!("{id}.mdc"));
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).unwrap();
        }
        std::fs::write(
            path,
            format!(
                "---\nid: {id}\nlevel: INFO\nalwaysApply: true\nenabled: true\nscope: \"project\"\n---\n\nkeep-a\n{mid1}\nkeep-b\n{mid2}\nkeep-c\n"
            ),
        )
        .unwrap();
    }

    #[test]
    fn restore_merge_accepts_first_hunk_keeps_second_local() {
        let src = tempfile::tempdir().unwrap();
        write_two_hunk_rule(src.path(), "alpha", "PACK-ONE", "PACK-TWO");
        let zip = build_policy_zip(src.path(), &spec(&["alpha"], &[])).unwrap();
        let dest = tempfile::tempdir().unwrap();
        restore_policy_zip(dest.path(), &zip, &HashMap::new()).unwrap();
        let dest_file = agents_dir(dest.path()).join("rules/alpha.mdc");
        write_two_hunk_rule(dest.path(), "alpha", "LOCAL-ONE", "LOCAL-TWO");
        let mut dec = HashMap::new();
        dec.insert(
            "rule:alpha".into(),
            RestoreAction::Merge {
                accept_hunks: vec![0],
                hunks: vec![],
            },
        );
        restore_policy_zip(dest.path(), &zip, &dec).unwrap();
        let body = std::fs::read_to_string(&dest_file).unwrap();
        assert!(body.contains("PACK-ONE"));
        assert!(body.contains("LOCAL-TWO"));
        assert!(!body.contains("PACK-TWO"));
        assert!(!body.contains("LOCAL-ONE"));
    }

    #[test]
    fn restore_merge_string_overwrite_still_full_file() {
        let src = tempfile::tempdir().unwrap();
        write_two_hunk_rule(src.path(), "alpha", "PACK-ONE", "PACK-TWO");
        let zip = build_policy_zip(src.path(), &spec(&["alpha"], &[])).unwrap();
        let dest = tempfile::tempdir().unwrap();
        restore_policy_zip(dest.path(), &zip, &HashMap::new()).unwrap();
        write_two_hunk_rule(dest.path(), "alpha", "LOCAL-ONE", "LOCAL-TWO");
        let parsed: HashMap<String, RestoreAction> =
            serde_json::from_str(r#"{"rule:alpha":"overwrite"}"#).unwrap();
        restore_policy_zip(dest.path(), &zip, &parsed).unwrap();
        let body = std::fs::read_to_string(agents_dir(dest.path()).join("rules/alpha.mdc")).unwrap();
        assert!(body.contains("PACK-ONE"));
        assert!(body.contains("PACK-TWO"));
    }

    #[test]
    fn restore_merge_invalid_hunk_index_errors() {
        let src = tempfile::tempdir().unwrap();
        write_two_hunk_rule(src.path(), "alpha", "PACK-ONE", "PACK-TWO");
        let zip = build_policy_zip(src.path(), &spec(&["alpha"], &[])).unwrap();
        let dest = tempfile::tempdir().unwrap();
        restore_policy_zip(dest.path(), &zip, &HashMap::new()).unwrap();
        write_two_hunk_rule(dest.path(), "alpha", "LOCAL-ONE", "LOCAL-TWO");
        let mut dec = HashMap::new();
        dec.insert(
            "rule:alpha".into(),
            RestoreAction::Merge {
                accept_hunks: vec![99],
                hunks: vec![],
            },
        );
        let err = restore_policy_zip(dest.path(), &zip, &dec).unwrap_err();
        assert!(err.to_string().contains("hunk"));
    }

    #[test]
    fn restore_merge_json_roundtrip() {
        let parsed: HashMap<String, RestoreAction> =
            serde_json::from_str(r#"{"rule:alpha":{"action":"merge","acceptHunks":[0,2]}}"#)
                .unwrap();
        assert_eq!(
            parsed.get("rule:alpha"),
            Some(&RestoreAction::Merge {
                accept_hunks: vec![0, 2],
                hunks: vec![],
            })
        );
        let json = serde_json::to_string(&parsed).unwrap();
        assert!(json.contains("acceptHunks"));
        assert!(json.contains("merge"));
    }
}
