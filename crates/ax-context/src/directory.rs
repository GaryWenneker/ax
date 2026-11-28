//! Directory and project root utilities.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const AX_DIR: &str = ".ax";
pub const DB_FILENAME: &str = "ax.db";
pub const CONFIG_FILENAME: &str = "ax.json";

const WORKSPACE_ROOT_MANIFESTS: &[&str] = &[
    "package.json",
    "pnpm-workspace.yaml",
    "lerna.json",
    "nx.json",
    "turbo.json",
    "go.work",
    "go.mod",
    "Cargo.toml",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "settings.gradle",
    "pyproject.toml",
    "composer.json",
    "Gemfile",
    "rush.json",
    "WORKSPACE",
    "WORKSPACE.bazel",
];

const SUBPROJECT_SCAN_SKIP: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    ".git",
    "vendor",
    "tmp",
    "temp",
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

/// Why `path` must not be indexed: it is a filesystem root (`/`, `C:\`, any
/// drive) or the home directory. Paths are compared after canonicalizing both
/// sides, so a symlinked HOME, a trailing slash or a Windows `\\?\` prefix
/// does not hide a match.
pub fn unsafe_index_root_reason(path: &Path) -> Option<String> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    if canonical.has_root() && canonical.parent().is_none() {
        return Some(format!(
            "refusing to index unsafe root: {}",
            canonical.display()
        ));
    }

    let home = std::env::var_os("USERPROFILE")
        .filter(|h| !h.is_empty())
        .or_else(|| std::env::var_os("HOME").filter(|h| !h.is_empty()));
    if let Some(home) = home {
        let home = PathBuf::from(home);
        let home = home.canonicalize().unwrap_or(home);
        if canonical == home {
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
        "call graph",
        "impact",
        "callers",
        "callees",
        "dependency",
        "flow",
        "architecture",
        "who calls",
        "what calls",
        "affected",
        "blast radius",
        "trace",
        "path",
        "structure",
        "how does",
        "where is",
        "wired",
        "implement",
    ];
    if keywords.iter().any(|k| lower.contains(k)) {
        return true;
    }
    static STRUCTURAL_EN: OnceLock<regex::Regex> = OnceLock::new();
    static STRUCTURAL_CJK: OnceLock<regex::Regex> = OnceLock::new();
    let structural_en = STRUCTURAL_EN.get_or_init(|| {
        regex::Regex::new(
            r"\b(how|where|trace|flow|path|reach(?:es|ed)?|call(?:s|ed|er|ers|ee)?|depend|impact|affect|wired?|connect|implement|architect|structure|breaks?|what calls|why does)\b",
        )
        .expect("structural regex")
    });
    let structural_cjk = STRUCTURAL_CJK.get_or_init(|| {
        regex::Regex::new(
            r"如何|怎么|在哪|哪里|追踪|跟踪|流程|流向|路径|调用|依赖|影响|实现|架构|结构|介绍|解析|分析|原理|机制",
        )
        .expect("structural CJK regex")
    });
    structural_en.is_match(prompt) || structural_cjk.is_match(prompt)
}

const HARNESS_BLOCK_OPEN: &str = "<system-reminder>";
const HARNESS_BLOCK_CLOSE: &str = "</system-reminder>";
/// Prompts that are one harness message: a finished background task, or an event
/// the desktop app sends about a pull request or a side session.
const HARNESS_NOTIFICATIONS: &[&str] = &[
    "<task-notification>",
    "<ci-monitor-event>",
    "<side-sessions-event>",
];

/// The part of a hook prompt the user wrote.
///
/// Agent harnesses append `<system-reminder>` blocks to the prompt and deliver
/// background-task results and app events as a prompt that is one block
/// (`<task-notification>`, `<ci-monitor-event>`, `<side-sessions-event>`).
/// None of these is a question about the code, so matching or exploring them injects
/// context for text nobody asked about. Reminder blocks are cut out (an unclosed
/// one hides the rest), and a prompt that is only such a block becomes empty.
pub fn user_prompt_text(prompt: &str) -> String {
    let mut text = String::with_capacity(prompt.len());
    let mut rest = prompt;
    while let Some(open) = rest.find(HARNESS_BLOCK_OPEN) {
        text.push_str(&rest[..open]);
        rest = match rest[open..].find(HARNESS_BLOCK_CLOSE) {
            Some(close) => &rest[open + close + HARNESS_BLOCK_CLOSE.len()..],
            None => "",
        };
    }
    text.push_str(rest);
    let text = text.trim();
    if HARNESS_NOTIFICATIONS
        .iter()
        .any(|tag| text.starts_with(tag))
    {
        return String::new();
    }
    text.to_string()
}

pub fn extract_code_tokens(prompt: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    // Two humps or more: `IndexPathFilter`, `getUser`. One capitalized word
    // ("Ok", "You") is ordinary language, and half the index has a node by that name.
    static RE_CAMEL: OnceLock<regex::Regex> = OnceLock::new();
    static RE_SNAKE: OnceLock<regex::Regex> = OnceLock::new();
    let re_camel = RE_CAMEL.get_or_init(|| {
        regex::Regex::new(
            r"\b([A-Z][a-z0-9]+(?:[A-Z][a-z0-9]*)+|[a-z][a-z0-9]*(?:[A-Z][a-z0-9]*)+)\b",
        )
        .expect("camel regex")
    });
    for cap in re_camel.captures_iter(prompt) {
        if let Some(m) = cap.get(1) {
            tokens.push(m.as_str().to_string());
        }
    }
    let re_snake = RE_SNAKE.get_or_init(|| {
        regex::Regex::new(r"\b([a-z][a-z0-9]*(?:_[a-z0-9]+)+)\b").expect("snake regex")
    });
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
        let scan_root = install_scan_root(root);
        visit_install_projects(root, &scan_root, 0, max_depth, &mut out);
    }
    out.sort();
    out.dedup();
    out
}

fn install_project(dir: &Path) -> bool {
    if unsafe_index_root_reason(dir).is_some() {
        return false;
    }
    is_initialized(dir)
}

fn install_scan_root(root: &Path) -> PathBuf {
    root.canonicalize().unwrap_or_else(|_| root.to_path_buf())
}

fn dir_within_install_scan(scan_root: &Path, path: &Path) -> bool {
    let Ok(canonical) = path.canonicalize() else {
        return false;
    };
    canonical.strip_prefix(scan_root).is_ok()
}

fn visit_install_projects(
    dir: &Path,
    scan_root: &Path,
    depth: usize,
    max_depth: usize,
    out: &mut Vec<PathBuf>,
) {
    if install_project(dir) {
        if dir_within_install_scan(scan_root, dir) {
            out.push(dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf()));
        }
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
        let child = entry.path();
        if !dir_within_install_scan(scan_root, &child) {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || INSTALL_DISCOVERY_SKIP.iter().any(|skip| *skip == name) {
            continue;
        }
        visit_install_projects(&child, scan_root, depth + 1, max_depth, out);
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
        let name = s
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
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
    fn skips_a_git_repo_that_is_not_an_ax_project() {
        let root = scratch("git");
        let repo = root.join("alpha");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::create_dir_all(root.join("plain")).unwrap();
        let found = discover_projects_for_init(&root, 2);
        assert_eq!(names(&found), Vec::<String>::new());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn skips_a_manifest_that_is_not_an_ax_project() {
        let root = scratch("manifest");
        let app = root.join("app");
        fs::create_dir_all(&app).unwrap();
        fs::write(app.join("Cargo.toml"), b"[package]\nname = \"app\"\n").unwrap();
        let found = discover_projects_for_init(&root, 2);
        assert_eq!(names(&found), Vec::<String>::new());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn does_not_descend_into_a_project() {
        let root = scratch("nested");
        let repo = root.join("mono");
        fs::create_dir_all(repo.join(".ax")).unwrap();
        fs::write(repo.join(".ax").join("ax.db"), b"x").unwrap();
        let nested = repo.join("crates").join("leaf");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("Cargo.toml"), b"[package]\nname = \"leaf\"\n").unwrap();
        let found = discover_projects_for_init(&root, 4);
        assert_eq!(names(&found), vec!["mono".to_string()]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn finds_an_ax_project_nested_in_a_git_repo() {
        let root = scratch("wrap");
        let repo = root.join("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        let inner = repo.join("pkg");
        fs::create_dir_all(inner.join(".ax")).unwrap();
        fs::write(inner.join(".ax").join("ax.db"), b"x").unwrap();
        let found = discover_projects_for_init(&root, 4);
        assert_eq!(names(&found), vec!["pkg".to_string()]);
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
    fn ignores_a_nested_symlink_that_escapes_the_scan_root() {
        let root = scratch("nest-link");
        let ready = root.join("ready");
        fs::create_dir_all(ready.join(".ax")).unwrap();
        fs::write(ready.join(".ax").join("ax.db"), b"x").unwrap();
        let outside = scratch("nest-outside");
        fs::create_dir_all(outside.join(".ax")).unwrap();
        fs::write(outside.join(".ax").join("ax.db"), b"x").unwrap();
        let wrapper = root.join("wrapper");
        fs::create_dir_all(&wrapper).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, wrapper.join("escape")).unwrap();
        let found = discover_projects_for_init(&root, 3);
        assert_eq!(names(&found), vec!["ready".to_string()]);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }

    #[test]
    fn skips_the_home_directory_itself() {
        let root = scratch("home");
        fs::write(root.join("package.json"), b"{}\n").unwrap();
        let child = root.join("app");
        fs::create_dir_all(child.join(".ax")).unwrap();
        fs::write(child.join(".ax").join("ax.db"), b"x").unwrap();
        let other = root.join("plain-mod");
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join("go.mod"), b"module app\n").unwrap();
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

#[cfg(test)]
mod prompt_tests {
    use super::*;

    #[test]
    fn e1_harness_blocks_are_removed_from_the_prompt() {
        let prompt = "<system-reminder>how does the call graph work</system-reminder>\nYes";
        assert_eq!(user_prompt_text(prompt), "Yes");
        let two = "<system-reminder>a</system-reminder>fix getUser<system-reminder>\nb\n</system-reminder>";
        assert_eq!(user_prompt_text(two), "fix getUser");
        // An unclosed block hides everything after it: it is not the user's text.
        assert_eq!(user_prompt_text("go <system-reminder>trace the flow"), "go");
        assert_eq!(
            user_prompt_text("  how does sync work  "),
            "how does sync work"
        );
    }

    #[test]
    fn e2_an_app_event_only_prompt_is_empty() {
        for tag in ["ci-monitor-event", "side-sessions-event"] {
            let prompt = format!("<{tag}>how does the build flow</{tag}>");
            assert_eq!(user_prompt_text(&prompt), "", "{tag}");
        }
        assert_eq!(
            user_prompt_text("why did <ci-monitor-event> fire, how does it flow"),
            "why did <ci-monitor-event> fire, how does it flow"
        );
    }

    #[test]
    fn e2_a_notification_only_prompt_is_empty() {
        let prompt = "<system-reminder>x</system-reminder>\n<task-notification>\n<summary>how does the build flow</summary>\n</task-notification>";
        assert_eq!(user_prompt_text(prompt), "");
        assert_eq!(
            user_prompt_text("see <task-notification> in the docs, how does it flow"),
            "see <task-notification> in the docs, how does it flow"
        );
    }

    #[test]
    fn e3_a_plain_capitalized_word_is_not_a_code_token() {
        assert!(extract_code_tokens("Ok").is_empty());
        assert!(extract_code_tokens("You choose. It must work perfectly").is_empty());
        assert_eq!(
            extract_code_tokens("fix capture_affected_paths and IndexPathFilter and getUser"),
            vec!["IndexPathFilter", "getUser", "capture_affected_paths"]
        );
        assert_eq!(
            extract_code_tokens("the Sha256Hasher type"),
            vec!["Sha256Hasher"]
        );
    }
}
