//! Composable language and CMS stacks. Core policy stays on `ax init`; stacks are opt-in.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::hierarchy::ensure_scope_dirs;
use crate::paths::{rule_file, skill_file};
use crate::revisions::content_hash;
use crate::stack_catalog::{StackDef, STACKS};
use crate::types::PolicyScope;

const LOCK_REL: &str = ".ax/stacks.lock.json";
const MAX_DETECT_DEPTH: usize = 8;
const IGNORE_DIRS: &[&str] = &[
    "node_modules", "bin", "obj", "target", "vendor", ".git", ".ax", "dist",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackInfo {
    pub id: String,
    pub version: String,
    pub description: String,
    pub depends_on: Vec<String>,
    pub files: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedStack {
    pub id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReport {
    pub stacks: Vec<String>,
    pub created: Vec<String>,
    pub updated: Vec<String>,
    pub unchanged: Vec<String>,
    pub skipped_user_edit: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StackStatus {
    pub id: String,
    pub installed_version: Option<String>,
    pub catalog_version: String,
    pub drifted_files: Vec<String>,
    pub upgrade_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct LockFile {
    #[serde(default)]
    applied_at: String,
    #[serde(default)]
    stacks: BTreeMap<String, LockStack>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct LockStack {
    template_version: String,
    files: BTreeMap<String, String>,
}

pub fn catalog() -> Vec<StackInfo> {
    STACKS.iter().map(info_of).collect()
}

fn info_of(def: &StackDef) -> StackInfo {
    StackInfo {
        id: def.id.into(),
        version: def.version.into(),
        description: def.description.into(),
        depends_on: def.depends_on.iter().map(|s| (*s).to_string()).collect(),
        files: def.files.len(),
    }
}

pub fn find_stack(id: &str) -> Option<&'static StackDef> {
    let key = id.trim().to_ascii_lowercase();
    STACKS.iter().find(|s| s.id == key)
}

/// Ordered unique ids with dependencies prepended. Unknown ids error with the catalog.
pub fn resolve(ids: &[String]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for id in ids {
        let key = id.trim().to_ascii_lowercase();
        if key.is_empty() {
            continue;
        }
        push_resolved(&key, &mut out, &mut seen)?;
    }
    Ok(out)
}

fn push_resolved(id: &str, out: &mut Vec<String>, seen: &mut BTreeSet<String>) -> Result<(), String> {
    if seen.contains(id) {
        return Ok(());
    }
    let def = find_stack(id).ok_or_else(|| {
        let known = STACKS.iter().map(|s| s.id).collect::<Vec<_>>().join(", ");
        format!("unknown stack '{id}'. Available: {known}")
    })?;
    for dep in def.depends_on {
        push_resolved(dep, out, seen)?;
    }
    seen.insert(id.to_string());
    out.push(id.to_string());
    Ok(())
}

/// Proposal only. A direct `react` dependency is reported even when Next.js is present.
pub fn detect(root: &Path) -> Vec<DetectedStack> {
    let mut found: BTreeMap<String, String> = BTreeMap::new();
    let mut saw_next_config = false;
    let mut saw_react_dep = false;
    let mut saw_composer = false;
    let mut saw_artisan = false;
    let mut saw_drupal = false;

    let walk = WalkDir::new(root).max_depth(MAX_DETECT_DEPTH).into_iter().filter_entry(|entry| {
        let name = entry.file_name().to_str().unwrap_or("");
        !IGNORE_DIRS.contains(&name)
    });
    for entry in walk.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        if path.ancestors().any(|a| {
            a.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| IGNORE_DIRS.contains(&n))
        }) {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".csproj") || lower.ends_with(".sln") {
            found.entry("dotnet".into()).or_insert_with(|| name.to_string());
            if let Ok(text) = fs::read_to_string(path) {
                let l = text.to_ascii_lowercase();
                if l.contains("optimizely") || l.contains("episerver") {
                    found.entry("optimizely".into()).or_insert_with(|| name.to_string());
                }
            }
        }
        if lower.ends_with(".scproj") || lower.contains("unicorn") {
            found.entry("sitecore".into()).or_insert_with(|| name.to_string());
        }
        if lower == "pom.xml" || lower == "build.gradle" || lower == "build.gradle.kts" {
            found.entry("java".into()).or_insert_with(|| name.to_string());
        }
        if lower == "angular.json" {
            found.entry("angular".into()).or_insert_with(|| name.to_string());
        }
        if lower.starts_with("next.config.") {
            saw_next_config = true;
        }
        if lower == "artisan" {
            saw_artisan = true;
        }
        if lower == "composer.json" {
            saw_composer = true;
            if let Ok(text) = fs::read_to_string(path) {
                let l = text.to_ascii_lowercase();
                if l.contains("drupal") {
                    saw_drupal = true;
                }
            }
        }
        if lower == "package.json" {
            if let Ok(text) = fs::read_to_string(path) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                    let mut deps = Vec::new();
                    for key in ["dependencies", "devDependencies"] {
                        if let Some(obj) = v.get(key).and_then(|x| x.as_object()) {
                            deps.extend(obj.keys().cloned());
                        }
                    }
                    if deps.iter().any(|d| d == "next") {
                        saw_next_config = true;
                    }
                    if deps.iter().any(|d| d == "react" || d == "react-dom") {
                        saw_react_dep = true;
                    }
                    if deps.iter().any(|d| d == "vue") {
                        found.entry("vue".into()).or_insert_with(|| "package.json vue".into());
                    }
                }
            }
        }
        if path.components().any(|c| c.as_os_str() == "serialization") && lower.ends_with(".yml") {
            found.entry("sitecore".into()).or_insert_with(|| "serialization".into());
        }
        note_language(&mut found, &lower, name);
    }

    if saw_next_config {
        found.entry("nextjs".into()).or_insert_with(|| "package.json next".into());
    }
    if saw_react_dep {
        found.entry("react".into()).or_insert_with(|| "package.json react".into());
    }
    if saw_drupal {
        found.entry("drupal".into()).or_insert_with(|| "composer.json drupal".into());
    } else if saw_artisan && saw_composer {
        found.entry("laravel".into()).or_insert_with(|| "artisan".into());
    } else if saw_composer {
        found.entry("php".into()).or_insert_with(|| "composer.json".into());
    }

    found
        .into_iter()
        .map(|(id, reason)| DetectedStack { id, reason })
        .collect()
}

pub fn apply(root: &Path, ids: &[String], force: bool) -> Result<ApplyReport, String> {
    let resolved = resolve(ids)?;
    let policy_dir = ensure_scope_dirs(root, PolicyScope::Project).map_err(|e| e.to_string())?;
    let mut lock = read_lock(root);
    let mut report = ApplyReport {
        stacks: resolved.clone(),
        ..Default::default()
    };

    for id in &resolved {
        let def = find_stack(id).expect("resolved");
        let mut files = BTreeMap::new();
        for file in def.files {
            let dest = dest_for_rel(&policy_dir, file.rel);
            let template_hash = content_hash(file.body);
            let label = format!("{id}:{rel}", rel = file.rel);
            let existing = fs::read_to_string(&dest).ok();
            let existing_hash = existing.as_deref().map(content_hash);
            let locked = lock.stacks.get(id).and_then(|s| s.files.get(file.rel)).cloned();

            let user_edit = match (&existing_hash, &locked) {
                (Some(disk), Some(prev)) => disk != prev && disk != &template_hash,
                (Some(disk), None) => disk != &template_hash,
                _ => false,
            };
            if user_edit && !force {
                report.skipped_user_edit.push(label);
                if let Some(prev) = locked {
                    files.insert(file.rel.to_string(), prev);
                } else if let Some(disk) = existing_hash {
                    files.insert(file.rel.to_string(), disk);
                }
                continue;
            }
            if existing_hash.as_deref() == Some(template_hash.as_str()) {
                report.unchanged.push(label);
                files.insert(file.rel.to_string(), template_hash);
                continue;
            }
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::write(&dest, file.body.as_bytes()).map_err(|e| e.to_string())?;
            if existing.is_some() {
                report.updated.push(label);
            } else {
                report.created.push(label);
            }
            files.insert(file.rel.to_string(), template_hash);
        }
        lock.stacks.insert(
            id.clone(),
            LockStack {
                template_version: def.version.into(),
                files,
            },
        );
    }
    lock.applied_at = now_rfc3339();
    write_lock(root, &lock)?;
    crate::config::write_project_stacks(root, &resolved, "off")?;
    Ok(report)
}

pub fn remove(root: &Path, id: &str) -> Result<Vec<String>, String> {
    let key = id.trim().to_ascii_lowercase();
    if find_stack(&key).is_none() {
        return Err(format!("unknown stack '{key}'"));
    }
    let mut lock = read_lock(root);
    let dependents: Vec<_> = lock
        .stacks
        .keys()
        .filter_map(|other| {
            find_stack(other).and_then(|d| {
                if d.depends_on.contains(&key.as_str()) {
                    Some(other.clone())
                } else {
                    None
                }
            })
        })
        .collect();
    if !dependents.is_empty() {
        return Err(format!(
            "cannot remove '{key}' while these stacks still depend on it: {}",
            dependents.join(", ")
        ));
    }
    let Some(entry) = lock.stacks.remove(&key) else {
        return Ok(Vec::new());
    };
    let policy_dir = ensure_scope_dirs(root, PolicyScope::Project).map_err(|e| e.to_string())?;
    let mut removed = Vec::new();
    for (rel, hash) in entry.files {
        let dest = dest_for_rel(&policy_dir, &rel);
        if let Ok(text) = fs::read_to_string(&dest) {
            if content_hash(&text) == hash {
                fs::remove_file(&dest).map_err(|e| e.to_string())?;
                removed.push(rel);
            }
        }
    }
    write_lock(root, &lock)?;
    let remaining: Vec<String> = lock.stacks.keys().cloned().collect();
    crate::config::write_project_stacks(root, &remaining, "off")?;
    Ok(removed)
}

pub fn status(root: &Path) -> Vec<StackStatus> {
    let lock = read_lock(root);
    STACKS
        .iter()
        .filter(|def| lock.stacks.contains_key(def.id))
        .map(|def| {
            let installed = lock.stacks.get(def.id).unwrap();
            let policy_dir = ensure_scope_dirs(root, PolicyScope::Project).ok();
            let mut drifted = Vec::new();
            let mut upgrade = installed.template_version != def.version;
            for file in def.files {
                let locked = installed.files.get(file.rel);
                let disk = policy_dir.as_ref().and_then(|dir| {
                    fs::read_to_string(dest_for_rel(dir, file.rel)).ok()
                });
                let disk_hash = disk.as_deref().map(content_hash);
                let template_hash = content_hash(file.body);
                if locked.map(|h| h != &template_hash).unwrap_or(true) {
                    upgrade = true;
                }
                if disk_hash.as_ref() != locked || disk_hash.as_deref() != Some(template_hash.as_str()) {
                    if disk_hash.as_deref() != Some(template_hash.as_str()) {
                        drifted.push(file.rel.to_string());
                    }
                }
            }
            StackStatus {
                id: def.id.into(),
                installed_version: Some(installed.template_version.clone()),
                catalog_version: def.version.into(),
                drifted_files: drifted,
                upgrade_available: upgrade,
            }
        })
        .collect()
}

/// Refresh managed files that still match the lock hash.
pub fn upgrade(root: &Path) -> Result<ApplyReport, String> {
    let lock = read_lock(root);
    let ids: Vec<String> = lock.stacks.keys().cloned().collect();
    if ids.is_empty() {
        return Ok(ApplyReport::default());
    }
    apply(root, &ids, false)
}

/// Interpret one line from the init stack prompt.
/// Empty keeps the current selection, or the detection when nothing is saved.
/// `none` clears stacks. Other text is a list of ids separated by spaces or commas.
pub fn parse_stack_choice(line: &str, current: &[String], detected: &[String]) -> Result<Vec<String>, String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        if !current.is_empty() {
            return Ok(current.to_vec());
        }
        return Ok(detected.to_vec());
    }
    if trimmed.eq_ignore_ascii_case("none") {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = trimmed
        .split(|c: char| c.is_whitespace() || c == ',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_lowercase())
        .collect();
    resolve(&ids)
}

/// Install `ids` and remove previously installed stacks that are no longer selected.
pub fn replace_selection(root: &Path, ids: &[String], force: bool) -> Result<ApplyReport, String> {
    let resolved = resolve(ids)?;
    let installed: Vec<String> = status(root).into_iter().map(|row| row.id).collect();
    let mut pending: Vec<String> = installed
        .into_iter()
        .filter(|id| !resolved.iter().any(|keep| keep == id))
        .collect();
    while !pending.is_empty() {
        let leaf = pending.iter().position(|id| {
            !pending.iter().any(|other| {
                find_stack(other)
                    .map(|def| def.depends_on.iter().any(|dep| *dep == id))
                    .unwrap_or(false)
            })
        });
        let Some(index) = leaf else {
            return Err(format!(
                "cannot drop stacks with a dependency cycle: {}",
                pending.join(", ")
            ));
        };
        let id = pending.remove(index);
        remove(root, &id)?;
    }
    if resolved.is_empty() {
        crate::config::write_project_stacks(root, &[], "off")?;
        return Ok(ApplyReport::default());
    }
    apply(root, &resolved, force)
}

pub fn read_configured_stacks(root: &Path) -> Vec<String> {
    crate::config::read_project_stack_ids(root)
}

fn note_language(found: &mut BTreeMap<String, String>, lower: &str, name: &str) {
    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    let id = match (lower, ext) {
        ("cargo.toml", _) => "rust",
        ("go.mod", _) => "go",
        ("pyproject.toml" | "requirements.txt", _) => "python",
        ("tsconfig.json", _) => "typescript",
        ("gemfile", _) => "ruby",
        ("package.swift", _) => "swift",
        ("pubspec.yaml", _) => "dart",
        ("build.sbt", _) => "scala",
        _ => match ext {
            "rs" => "rust",
            "go" => "go",
            "py" => "python",
            "ts" | "tsx" | "mts" | "cts" => "typescript",
            "js" | "jsx" | "mjs" | "cjs" => "javascript",
            "c" => "c",
            "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => "cpp",
            "rb" => "ruby",
            "swift" => "swift",
            "kt" | "kts" => "kotlin",
            "dart" => "dart",
            "svelte" => "svelte",
            "astro" => "astro",
            "scala" => "scala",
            "lua" => "lua",
            "luau" => "luau",
            "m" | "mm" => "objc",
            "r" => "r",
            "pas" | "pp" => "pascal",
            _ => return,
        },
    };
    found.entry(id.into()).or_insert_with(|| name.to_string());
}

fn dest_for_rel(policy_dir: &Path, rel: &str) -> PathBuf {
    if let Some(id) = rel.strip_prefix("rules/").and_then(|s| s.strip_suffix(".mdc")) {
        return rule_file(&policy_dir.join("rules"), id);
    }
    if let Some(rest) = rel.strip_prefix("skills/") {
        if let Some(name) = rest.strip_suffix("/SKILL.md") {
            return skill_file(&policy_dir.join("skills"), name);
        }
    }
    policy_dir.join(rel)
}

fn lock_path(root: &Path) -> PathBuf {
    root.join(LOCK_REL)
}

fn read_lock(root: &Path) -> LockFile {
    fs::read_to_string(lock_path(root))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_lock(root: &Path, lock: &LockFile) -> Result<(), String> {
    let path = lock_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(lock).map_err(|e| e.to_string())? + "\n";
    fs::write(path, text.as_bytes()).map_err(|e| e.to_string())
}

fn now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn touch(dir: &Path, rel: &str, body: &str) {
        let p = dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }

    #[test]
    fn detect_dotnet_and_react() {
        let dir = tempdir().unwrap();
        touch(dir.path(), "src/App.csproj", "<Project />");
        touch(
            dir.path(),
            "web/package.json",
            r#"{"dependencies":{"react":"18.0.0","react-dom":"18.0.0"}}"#,
        );
        let ids: Vec<_> = detect(dir.path()).into_iter().map(|d| d.id).collect();
        assert!(ids.contains(&"dotnet".into()));
        assert!(ids.contains(&"react".into()));
        assert!(!ids.contains(&"nextjs".into()));
    }

    #[test]
    fn detect_next_and_react_from_package_json() {
        let dir = tempdir().unwrap();
        touch(dir.path(), "next.config.js", "module.exports = {};");
        touch(
            dir.path(),
            "package.json",
            r#"{"dependencies":{"next":"14.0.0","react":"18.0.0"}}"#,
        );
        let ids: Vec<_> = detect(dir.path()).into_iter().map(|d| d.id).collect();
        assert!(ids.contains(&"nextjs".into()));
        assert!(ids.contains(&"react".into()));
    }

    #[test]
    fn detect_next_and_react_in_nested_client_app() {
        let dir = tempdir().unwrap();
        touch(
            dir.path(),
            "AdviseurPortaal/src/WebApp/ClientApp/package.json",
            r#"{"dependencies":{"next":"16.3.4","react":"19.2.8","react-dom":"19.2.8"}}"#,
        );
        touch(
            dir.path(),
            "node_modules/next/package.json",
            r#"{"dependencies":{"left-pad":"1.0.0"}}"#,
        );
        let ids: Vec<_> = detect(dir.path()).into_iter().map(|d| d.id).collect();
        assert!(ids.contains(&"nextjs".into()));
        assert!(ids.contains(&"react".into()));
        assert!(!ids.contains(&"javascript".into()));
    }

    #[test]
    fn detect_drupal_and_optimizely() {
        let dir = tempdir().unwrap();
        touch(dir.path(), "composer.json", r#"{"name":"drupal/recommended-project"}"#);
        let ids: Vec<_> = detect(dir.path()).into_iter().map(|d| d.id).collect();
        assert!(ids.contains(&"drupal".into()));
        assert!(!ids.contains(&"php".into()));

        let dir = tempdir().unwrap();
        touch(
            dir.path(),
            "Site.csproj",
            r#"<PackageReference Include="EPiServer.CMS" />"#,
        );
        let ids: Vec<_> = detect(dir.path()).into_iter().map(|d| d.id).collect();
        assert!(ids.contains(&"optimizely".into()));
        assert!(ids.contains(&"dotnet".into()));
    }

    #[test]
    fn detect_indexed_languages() {
        let dir = tempdir().unwrap();
        touch(dir.path(), "Cargo.toml", "[package]\nname = \"app\"\n");
        touch(dir.path(), "pkg/go.mod", "module example.com/app\n");
        touch(dir.path(), "app.py", "print('hi')\n");
        touch(dir.path(), "main.kt", "fun main() {}\n");
        let ids: Vec<_> = detect(dir.path()).into_iter().map(|d| d.id).collect();
        for id in ["rust", "go", "python", "kotlin"] {
            assert!(ids.contains(&id.into()), "{id} missing from {ids:?}");
        }
    }

    #[test]
    fn stack_choice_keeps_current_or_detected() {
        let current = vec!["dotnet".into()];
        let detected = vec!["rust".into()];
        assert_eq!(parse_stack_choice("", &current, &detected).unwrap(), current);
        assert_eq!(
            parse_stack_choice("  ", &[], &detected).unwrap(),
            detected
        );
        assert!(parse_stack_choice("none", &current, &detected).unwrap().is_empty());
        assert_eq!(
            parse_stack_choice("nextjs, rust", &[], &[]).unwrap(),
            vec!["react".to_string(), "nextjs".to_string(), "rust".to_string()]
        );
    }

    #[test]
    fn detect_empty_repo() {
        let dir = tempdir().unwrap();
        assert!(detect(dir.path()).is_empty());
    }

    #[test]
    fn resolve_deps_and_unknown() {
        let ids = resolve(&["nextjs".into(), "nextjs".into()]).unwrap();
        assert_eq!(ids, vec!["react".to_string(), "nextjs".to_string()]);
        let err = resolve(&["cobol".into()]).unwrap_err();
        assert!(err.contains("unknown stack"));
    }

    #[test]
    fn apply_two_stacks_is_idempotent_and_respects_edits() {
        let dir = tempdir().unwrap();
        let first = apply(dir.path(), &["dotnet".into(), "react".into()], false).unwrap();
        assert!(first.created.iter().any(|c| c.contains("dotnet-code-review")));
        assert!(first.created.iter().any(|c| c.contains("react-review")));
        assert!(dir.path().join(".agents/skills/dotnet-code-review/SKILL.md").is_file());
        assert!(dir.path().join(".agents/skills/react-review/SKILL.md").is_file());
        assert!(!dir.path().join(".agents/skills/noti/SKILL.md").exists());

        let second = apply(dir.path(), &["dotnet".into(), "react".into()], false).unwrap();
        assert!(second.created.is_empty());
        assert!(second.updated.is_empty());

        let skill = dir.path().join(".agents/skills/react-review/SKILL.md");
        fs::write(&skill, "user edit\n").unwrap();
        let third = apply(dir.path(), &["react".into()], false).unwrap();
        assert!(third.skipped_user_edit.iter().any(|s| s.contains("react-review")));
        assert_eq!(fs::read_to_string(&skill).unwrap(), "user edit\n");

        let forced = apply(dir.path(), &["react".into()], true).unwrap();
        assert!(forced.updated.iter().any(|s| s.contains("react-review")));
        assert!(fs::read_to_string(&skill).unwrap().contains("name: react-review"));
    }

    #[test]
    fn remove_drops_stack_files_only() {
        let dir = tempdir().unwrap();
        apply(dir.path(), &["dotnet".into(), "react".into()], false).unwrap();
        fs::create_dir_all(dir.path().join(".agents/skills/startup")).unwrap();
        fs::write(dir.path().join(".agents/skills/startup/SKILL.md"), "core\n").unwrap();
        let removed = remove(dir.path(), "react").unwrap();
        assert!(removed.iter().any(|r| r.contains("react-review")));
        assert!(dir.path().join(".agents/skills/dotnet-code-review/SKILL.md").is_file());
        assert!(!dir.path().join(".agents/skills/react-review/SKILL.md").exists());
        assert!(dir.path().join(".agents/skills/startup/SKILL.md").is_file());
    }

    #[test]
    fn upgrade_skips_user_edit() {
        let dir = tempdir().unwrap();
        apply(dir.path(), &["php".into()], false).unwrap();
        let skill = dir.path().join(".agents/skills/php-review/SKILL.md");
        fs::write(&skill, "local change\n").unwrap();
        let report = upgrade(dir.path()).unwrap();
        assert!(report.skipped_user_edit.iter().any(|s| s.contains("php-review")));
        assert_eq!(fs::read_to_string(&skill).unwrap(), "local change\n");
    }

    const DOTNET_REVIEW_REL: &str = "skills/dotnet-code-review/SKILL.md";

    fn stack_file_body(stack: &str, rel: &str) -> &'static str {
        find_stack(stack)
            .unwrap()
            .files
            .iter()
            .find(|f| f.rel == rel)
            .unwrap()
            .body
    }

    fn dotnet_review_body() -> &'static str {
        stack_file_body("dotnet", DOTNET_REVIEW_REL)
    }

    fn bullets(body: &str) -> Vec<String> {
        body.lines()
            .filter_map(|line| line.trim().strip_prefix("- "))
            .map(str::to_lowercase)
            .collect()
    }

    fn assert_no_duplicate_bullets(body: &str) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        for bullet in bullets(body) {
            assert!(seen.insert(bullet.clone()), "duplicate bullet: {bullet}");
        }
        seen
    }

    /// Apply `stack`, pretend the project still has an older unedited copy of `rel`,
    /// then upgrade. The temp dir is returned so the project outlives the caller's asserts.
    fn upgrade_from_older_copy(stack: &str, rel: &str) -> (tempfile::TempDir, ApplyReport, PathBuf) {
        let dir = tempdir().unwrap();
        apply(dir.path(), &[stack.into()], false).unwrap();
        let file = dest_for_rel(&dir.path().join(".agents"), rel);
        let old = "---\nname: old\n---\nversion 1.1.0 body\n";
        fs::write(&file, old).unwrap();
        let mut lock = read_lock(dir.path());
        lock.stacks.get_mut(stack).unwrap().files.insert(rel.into(), content_hash(old));
        write_lock(dir.path(), &lock).unwrap();
        let report = upgrade(dir.path()).unwrap();
        (dir, report, file)
    }

    #[test]
    fn dotnet_review_skill_covers_every_section() {
        let body = dotnet_review_body();
        for phrase in [
            "## 1. Naming and casing",
            "## 2. Layout and syntax",
            "## 3. Design",
            "## 4. Dependency injection",
            "## 5. CLR and memory",
            "## 6. Async and concurrency",
            "## 7. EF Core",
            "## 8. ASP.NET Core",
            "## 9. Security",
            "## 10. Observability and errors",
            "## 11. Resilience and testability",
            "## 12. Output format",
            "_camelCase",
            "SCREAMING_CAPS",
            "`Attribute`",
            "[Flags]",
            "FrozenDictionary",
            "System.Threading.Lock",
            "AsSplitQuery",
            "HybridCache",
            "ProblemDetails",
            "[LoggerMessage]",
            "TimeProvider",
            "IHttpClientFactory",
            "APPROVED WITH WARNINGS",
            "Captive",
            "service locator",
            "No `async void`",
            "No sync-over-async",
            "No hardcoded secrets",
            "never `throw ex;`",
        ] {
            assert!(body.contains(phrase), "dotnet-code-review lost {phrase:?}");
        }
    }

    #[test]
    fn dotnet_review_skill_has_no_duplicate_bullets() {
        let seen = assert_no_duplicate_bullets(dotnet_review_body());
        assert!(seen.len() > 50, "expected the full rule set, got {} bullets", seen.len());
    }

    #[test]
    fn upgrade_rewrites_an_unedited_older_dotnet_review() {
        let (dir, report, skill) = upgrade_from_older_copy("dotnet", DOTNET_REVIEW_REL);
        assert!(report.updated.iter().any(|s| s.contains("dotnet-code-review")), "{report:?}");
        assert_eq!(fs::read_to_string(&skill).unwrap(), dotnet_review_body());
        assert_eq!(read_lock(dir.path()).stacks["dotnet"].template_version, "1.2.0");
    }

    const NEXTJS_REVIEW_REL: &str = "skills/nextjs-review/SKILL.md";

    fn nextjs_review_body() -> &'static str {
        stack_file_body("nextjs", NEXTJS_REVIEW_REL)
    }

    #[test]
    fn nextjs_review_skill_covers_every_section() {
        let body = nextjs_review_body();
        for phrase in [
            "## 1. Structure and naming",
            "## 2. TypeScript",
            "## 3. Server and client components",
            "## 4. Server Actions",
            "## 5. Data fetching and caching",
            "## 6. Client state and React 19",
            "## 7. Performance and assets",
            "## 8. Styling and UI",
            "## 9. Security",
            "## 10. Errors and observability",
            "## 11. Testing",
            "## 12. Output format",
            "'use client'",
            "import 'server-only'",
            "useActionState",
            "revalidateTag",
            "outside `try/catch`",
            "Promise.all",
            "next/image",
            "NEXT_PUBLIC_",
            "DOMPurify",
            "global-error.tsx",
            "MSW",
            "APPROVED WITH WARNINGS",
            "No `any`",
            "validates its input with Zod",
            "Every Server Action checks the session",
            "Every Route Handler checks the session",
        ] {
            assert!(body.contains(phrase), "nextjs-review lost {phrase:?}");
        }
    }

    #[test]
    fn nextjs_review_skill_has_no_duplicate_bullets() {
        let seen = assert_no_duplicate_bullets(nextjs_review_body());
        assert!(seen.len() > 50, "expected the full rule set, got {} bullets", seen.len());
        let react = bullets(stack_file_body("react", "skills/react-review/SKILL.md"));
        for bullet in react {
            assert!(!seen.contains(&bullet), "already in react-review: {bullet}");
        }
    }

    #[test]
    fn upgrade_rewrites_an_unedited_older_nextjs_review() {
        let (dir, report, skill) = upgrade_from_older_copy("nextjs", NEXTJS_REVIEW_REL);
        assert!(report.updated.iter().any(|s| s.contains("nextjs-review")), "{report:?}");
        assert_eq!(fs::read_to_string(&skill).unwrap(), nextjs_review_body());
        assert_eq!(read_lock(dir.path()).stacks["nextjs"].template_version, "1.2.0");
    }

    /// Building skill, base skill: the building skill loads the base and must not repeat it.
    const BUILDS_ON: &[(&str, &str)] = &[
        ("laravel", "php"),
        ("drupal", "php"),
        ("sitecore", "dotnet"),
        ("optimizely", "dotnet"),
        ("typescript", "javascript"),
        ("luau", "lua"),
        ("cpp", "c"),
        ("objc", "c"),
        ("nextjs", "react"),
    ];

    fn review_rel(stack: &StackDef) -> &'static str {
        stack
            .files
            .iter()
            .map(|f| f.rel)
            .find(|rel| rel.starts_with("skills/") && rel.contains("review"))
            .unwrap_or_else(|| panic!("{} has no review skill", stack.id))
    }

    fn review_body(id: &str) -> &'static str {
        stack_file_body(id, review_rel(find_stack(id).unwrap()))
    }

    fn review_skill_name(id: &str) -> &'static str {
        review_rel(find_stack(id).unwrap())
            .trim_start_matches("skills/")
            .trim_end_matches("/SKILL.md")
    }

    fn numbered_sections(body: &str) -> Vec<&str> {
        body.lines()
            .filter(|line| {
                line.strip_prefix("## ")
                    .and_then(|rest| rest.split_once(". "))
                    .is_some_and(|(n, _)| n.parse::<u32>().is_ok())
            })
            .collect()
    }

    #[test]
    fn every_stack_review_skill_is_complete() {
        for stack in STACKS {
            let body = review_body(stack.id);
            let sections = numbered_sections(body);
            assert!(sections.len() >= 11, "{}: {} numbered sections", stack.id, sections.len());
            let last = sections.last().unwrap();
            assert!(last.ends_with(". Output format"), "{}: last section is {last:?}", stack.id);
            let output = &body[body.find(last).unwrap()..];
            for phrase in ["**Verdict:**", "**Location:**", "**Section:**", "**Impact:**", "**Suggested code:**"] {
                assert!(output.contains(phrase), "{}: output format lacks {phrase}", stack.id);
            }
            let count = bullets(body).len();
            assert!(count >= 50, "{}: {count} bullets", stack.id);
        }
    }

    #[test]
    fn every_stack_review_skill_has_no_duplicate_bullets() {
        for stack in STACKS {
            let mut seen = BTreeSet::new();
            for bullet in bullets(review_body(stack.id)) {
                assert!(seen.insert(bullet.clone()), "{}: duplicate bullet: {bullet}", stack.id);
            }
        }
    }

    #[test]
    fn building_skills_do_not_repeat_their_base() {
        for (building, base) in BUILDS_ON {
            let body = review_body(building);
            let base_name = review_skill_name(base);
            let intro = &body[..body.find("\n## ").unwrap_or_else(|| panic!("{building} has no sections"))];
            assert!(intro.contains(&format!("`{base_name}`")), "{building} intro does not name {base_name}");
            let base_bullets: BTreeSet<String> = bullets(review_body(base)).into_iter().collect();
            for bullet in bullets(body) {
                assert!(!base_bullets.contains(&bullet), "{building} repeats {base_name}: {bullet}");
            }
        }
    }

    #[test]
    fn upgrade_rewrites_every_unedited_older_review_skill() {
        for stack in STACKS {
            let rel = review_rel(stack);
            let (dir, report, skill) = upgrade_from_older_copy(stack.id, rel);
            let name = review_skill_name(stack.id);
            assert!(report.updated.iter().any(|s| s.contains(name)), "{}: {report:?}", stack.id);
            assert_eq!(fs::read_to_string(&skill).unwrap(), review_body(stack.id));
            assert_eq!(read_lock(dir.path()).stacks[stack.id].template_version, "1.2.0");
        }
    }

    #[test]
    fn every_stack_pack_is_version_1_2_0() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/stacks");
        for stack in STACKS {
            let toml = fs::read_to_string(root.join(stack.id).join("pack.toml")).unwrap();
            assert!(toml.contains("version = \"1.2.0\""), "{} pack.toml is not 1.2.0", stack.id);
            assert_eq!(stack.version, "1.2.0", "{} catalog version", stack.id);
        }
    }
}
