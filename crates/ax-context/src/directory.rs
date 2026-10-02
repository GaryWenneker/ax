//! Directory and project root utilities.

use std::fs;
use std::path::{Path, PathBuf};

pub const AX_DIR: &str = ".ax";
pub const DB_FILENAME: &str = "ax.db";
pub const CONFIG_FILENAME: &str = "ax.json";

const WORKSPACE_ROOT_MANIFESTS: &[&str] = &[
    "package.json", "pnpm-workspace.yaml", "lerna.json", "nx.json", "turbo.json",
    "go.work", "go.mod", "Cargo.toml", "pom.xml", "build.gradle", "build.gradle.kts",
    "settings.gradle", "pyproject.toml", "composer.json", "Gemfile", "rush.json",
    "WORKSPACE", "WORKSPACE.bazel",
];

const SUBPROJECT_SCAN_SKIP: &[&str] = &[
    "node_modules", "target", "dist", "build", ".git", "vendor", "tmp", "temp",
];

#[derive(Debug, Clone, Default)]
pub struct FrontloadPlan {
    pub explore_root: Option<PathBuf>,
    pub nudge_projects: Vec<PathBuf>,
    pub via_sub_scan: bool,
}

pub fn get_ax_dir(root: &Path) -> PathBuf {
    root.join(AX_DIR)
}

pub fn is_initialized(root: &Path) -> bool {
    get_ax_dir(root).join(DB_FILENAME).exists()
}

pub fn find_nearest_ax_root(cwd: &Path) -> Option<PathBuf> {
    let mut current = cwd.canonicalize().ok();
    while let Some(mut dir) = current {
        if is_initialized(&dir) {
            return Some(dir);
        }
        if !dir.pop() {
            break;
        }
        current = Some(dir);
    }
    None
}

pub fn unsafe_index_root_reason(path: &Path) -> Option<String> {
    let canonical = path.canonicalize().ok();
    let path_str = canonical
        .as_ref()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());

    let unsafe_roots = ["/", "C:\\", "C:/"];
    for root in unsafe_roots {
        if path_str == root || path_str == root.trim_end_matches('\\') {
            return Some(format!("refusing to index unsafe root: {}", path_str));
        }
    }

    if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        if path_str == home {
            return Some("refusing to index home directory".to_string());
        }
    }

    None
}

pub fn has_structural_keyword(prompt: &str) -> bool {
    if prompt.is_empty() {
        return false;
    }
    let lower = prompt.to_lowercase();
    let keywords = [
        "call graph", "impact", "callers", "callees", "dependency", "flow",
        "architecture", "who calls", "what calls", "affected", "blast radius",
        "trace", "path", "structure", "how does", "where is", "wired", "implement",
    ];
    if keywords.iter().any(|k| lower.contains(k)) {
        return true;
    }
    let structural_en = regex::Regex::new(
        r"\b(how|where|trace|flow|path|reach(?:es|ed)?|call(?:s|ed|er|ers|ee)?|depend|impact|affect|wired?|connect|implement|architect|structure|breaks?|what calls|why does)\b",
    )
    .unwrap();
    let structural_cjk = regex::Regex::new(
        r"如何|怎么|在哪|哪里|追踪|跟踪|流程|流向|路径|调用|依赖|影响|实现|架构|结构|介绍|解析|分析|原理|机制",
    )
    .unwrap();
    structural_en.is_match(prompt) || structural_cjk.is_match(prompt)
}

pub fn extract_code_tokens(prompt: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let re_camel =
        regex::Regex::new(r"\b([A-Z][a-z]+(?:[A-Z][a-z]*)*|[a-z]+(?:[A-Z][a-z]*)+)\b").unwrap();
    for cap in re_camel.captures_iter(prompt) {
        if let Some(m) = cap.get(1) {
            tokens.push(m.as_str().to_string());
        }
    }
    let re_snake = regex::Regex::new(r"\b([a-z][a-z0-9]*(?:_[a-z0-9]+)+)\b").unwrap();
    for cap in re_snake.captures_iter(prompt) {
        if let Some(m) = cap.get(1) {
            tokens.push(m.as_str().to_string());
        }
    }
    tokens
}

pub fn looks_like_project_root(dir: &Path) -> bool {
    WORKSPACE_ROOT_MANIFESTS
        .iter()
        .any(|m| dir.join(m).exists())
}

/// How many levels below the home directory install-time discovery still enters.
pub const INSTALL_DISCOVERY_DEPTH: usize = 4;

const INSTALL_DISCOVERY_SKIP: &[&str] = &[
    "node_modules",
    "target",
    "target-dev",
    "dist",
    "build",
    "vendor",
    "Library",
    "Applications",
    "Movies",
    "Music",
    "Pictures",
    "Downloads",
    "AppData",
    "Caches",
    "Pods",
    "coverage",
];

/// Project roots under `root` for an unattended `ax init`.
///
/// A directory counts when it already has `.ax/ax.db`, a `.git` directory or file,
/// or a workspace manifest. The walk stops at that directory, skips hidden and
/// dependency folders, does not follow symlinks, and never returns the home
/// directory or a filesystem root.
pub fn discover_projects_for_init(root: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if root.is_dir() {
        visit_install_projects(root, 0, max_depth, &mut out);
    }
    out.sort();
    out.dedup();
    out
}

fn install_project(dir: &Path) -> bool {
    if unsafe_index_root_reason(dir).is_some() {
        return false;
    }
    is_initialized(dir) || dir.join(".git").exists() || looks_like_project_root(dir)
}

fn visit_install_projects(dir: &Path, depth: usize, max_depth: usize, out: &mut Vec<PathBuf>) {
    if install_project(dir) {
        out.push(dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf()));
        return;
    }
    if depth >= max_depth {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_dir() || kind.is_symlink() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || INSTALL_DISCOVERY_SKIP.iter().any(|skip| *skip == name) {
            continue;
        }
        visit_install_projects(&entry.path(), depth + 1, max_depth, out);
    }
}

pub fn find_indexed_subproject_roots(root: &Path, max_depth: u32, max: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn walk(dir: &Path, depth: u32, max_depth: u32, max: usize, out: &mut Vec<PathBuf>) {
        if out.len() >= max || depth > max_depth {
            return;
        }
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            if out.len() >= max {
                return;
            }
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || SUBPROJECT_SCAN_SKIP.iter().any(|s| s == &name) {
                continue;
            }
            if is_initialized(&path) {
                out.push(path);
                continue;
            }
            walk(&path, depth + 1, max_depth, max, out);
        }
    }
    walk(root, 1, max_depth, max, &mut out);
    out
}

fn escape_regexp(s: &str) -> String {
    regex::escape(s)
}

/// Decide what the front-load hook injects for a prompt from `cwd`.
pub fn plan_frontload_full(cwd: &Path, prompt: &str) -> FrontloadPlan {
    let none = FrontloadPlan::default();
    let mut dir = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    for _ in 0..6 {
        if is_initialized(&dir) {
            return FrontloadPlan {
                explore_root: Some(dir),
                nudge_projects: vec![],
                via_sub_scan: false,
            };
        }
        if !dir.pop() {
            break;
        }
    }

    let base = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    if !looks_like_project_root(&base) {
        return none;
    }
    let subs = find_indexed_subproject_roots(&base, 4, 64);
    if subs.is_empty() {
        return none;
    }
    if subs.len() == 1 {
        return FrontloadPlan {
            explore_root: Some(subs[0].clone()),
            nudge_projects: vec![],
            via_sub_scan: true,
        };
    }

    let p = prompt.to_lowercase();
    let mut best: Option<(PathBuf, i32, usize)> = None;
    for s in &subs {
        let rel = s.strip_prefix(&base).unwrap_or(s).to_string_lossy();
        let rel_lc = rel.replace('\\', "/").to_lowercase();
        let name = s.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
        let mut score = 0;
        if !rel_lc.is_empty() && p.contains(&rel_lc) {
            score = 10;
        } else if name.len() >= 3 {
            let re = regex::Regex::new(&format!(r"\b{}\b", escape_regexp(&name))).unwrap();
            if re.is_match(&p) {
                score = 5;
            }
        }
        if score > 0 {
            let rel_len = rel.len();
            if best.is_none()
                || score > best.as_ref().unwrap().1
                || (score == best.as_ref().unwrap().1 && rel_len < best.as_ref().unwrap().2)
            {
                best = Some((s.clone(), score, rel_len));
            }
        }
    }
    if let Some((root, _, _)) = best {
        let nudge = subs.into_iter().filter(|s| s != &root).collect();
        return FrontloadPlan {
            explore_root: Some(root),
            nudge_projects: nudge,
            via_sub_scan: true,
        };
    }
    FrontloadPlan {
        explore_root: None,
        nudge_projects: subs,
        via_sub_scan: true,
    }
}

pub fn plan_frontload(cwd: &Path, prompt: &str) -> Option<PathBuf> {
    plan_frontload_full(cwd, prompt).explore_root
}

pub fn validate_directory(root: &Path) -> bool {
    let ax_dir = get_ax_dir(root);
    ax_dir.exists() && ax_dir.join(DB_FILENAME).exists()
}

#[cfg(test)]
mod escape_tests {
    use super::escape_regexp;

    #[test]
    fn escaped_text_matches_only_itself() {
        let re = regex::Regex::new(&format!("^{}$", escape_regexp("a.b*c"))).unwrap();
        assert!(re.is_match("a.b*c"));
        assert!(!re.is_match("axbbc"));
    }

    #[test]
    fn brackets_and_parens_are_literal() {
        let re = regex::Regex::new(&format!("^{}$", escape_regexp("x[1](y)"))).unwrap();
        assert!(re.is_match("x[1](y)"));
        assert!(!re.is_match("x1y"));
    }
}

#[cfg(test)]
mod discover_tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ax-discover-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names(paths: &[PathBuf]) -> Vec<String> {
        let mut out: Vec<String> = paths
            .iter()
            .map(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string()
            })
            .collect();
        out.sort();
        out
    }

    #[test]
    fn finds_a_git_repo_and_skips_a_plain_folder() {
        let root = scratch("git");
        let repo = root.join("alpha");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::create_dir_all(root.join("plain")).unwrap();
        let found = discover_projects_for_init(&root, 2);
        assert_eq!(names(&found), vec!["alpha".to_string()]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn finds_a_manifest_without_git() {
        let root = scratch("manifest");
        let app = root.join("app");
        fs::create_dir_all(&app).unwrap();
        fs::write(app.join("Cargo.toml"), b"[package]\nname = \"app\"\n").unwrap();
        let found = discover_projects_for_init(&root, 2);
        assert_eq!(names(&found), vec!["app".to_string()]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn does_not_descend_into_a_project() {
        let root = scratch("nested");
        let repo = root.join("mono");
        fs::create_dir_all(repo.join(".git")).unwrap();
        let nested = repo.join("crates").join("leaf");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("Cargo.toml"), b"[package]\nname = \"leaf\"\n").unwrap();
        let found = discover_projects_for_init(&root, 4);
        assert_eq!(names(&found), vec!["mono".to_string()]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn skips_node_modules_and_stops_at_max_depth() {
        let root = scratch("limits");
        let hidden = root.join("node_modules").join("pkg");
        fs::create_dir_all(hidden.join(".git")).unwrap();
        let deep = root.join("a").join("b").join("c");
        fs::create_dir_all(deep.join(".git")).unwrap();
        let found = discover_projects_for_init(&root, 2);
        assert!(found.is_empty(), "{found:?}");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn finds_an_initialized_directory_and_ignores_a_symlinked_directory() {
        let root = scratch("init");
        let ready = root.join("ready");
        fs::create_dir_all(ready.join(".ax")).unwrap();
        fs::write(ready.join(".ax").join("ax.db"), b"x").unwrap();
        let real = scratch("link-target");
        fs::create_dir_all(real.join(".git")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, root.join("linked")).unwrap();
        let found = discover_projects_for_init(&root, 2);
        assert_eq!(names(&found), vec!["ready".to_string()]);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(real);
    }

    #[test]
    fn skips_the_home_directory_itself() {
        let root = scratch("home");
        fs::write(root.join("package.json"), b"{}\n").unwrap();
        let child = root.join("app");
        fs::create_dir_all(&child).unwrap();
        fs::write(child.join("go.mod"), b"module app\n").unwrap();
        let canon = root.canonicalize().unwrap();
        let prev_home = std::env::var("HOME").ok();
        let prev_profile = std::env::var("USERPROFILE").ok();
        std::env::set_var("HOME", &canon);
        std::env::set_var("USERPROFILE", &canon);
        let found = discover_projects_for_init(&root, 2);
        if let Some(v) = prev_home {
            std::env::set_var("HOME", v);
        } else {
            std::env::remove_var("HOME");
        }
        if let Some(v) = prev_profile {
            std::env::set_var("USERPROFILE", v);
        } else {
            std::env::remove_var("USERPROFILE");
        }
        assert_eq!(names(&found), vec!["app".to_string()]);
        let _ = fs::remove_dir_all(root);
    }
}
