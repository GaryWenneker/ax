//! Remove ax sidecar data for one project. Never deletes the project tree itself.
//! Callers pass group ids; paths are recomputed here so a client cannot name arbitrary files.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PurgeGroup {
    pub id: &'static str,
    pub label: &'static str,
    pub detail: &'static str,
    pub bytes: u64,
    pub file_count: u32,
    /// Checked in the modal unless the user turns it off.
    pub default_on: bool,
    /// Nothing on disk for this group.
    pub empty: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PurgeReport {
    pub removed: Vec<String>,
    pub errors: Vec<String>,
}

const DATABASE: &str = "database";
const LOGGING: &str = "logging";
const RUNTIME: &str = "runtime";
const LOCAL_POLICY: &str = "localPolicy";
const SHIP: &str = "ship";
const AGENTS: &str = "agents";

/// Groups the modal can toggle. `recent` and `globalIndex` are applied by the HTTP handler.
pub fn file_groups(project_root: &Path) -> Vec<PurgeGroup> {
    let ax = project_root.join(".ax");
    vec![
        group(
            DATABASE,
            "Graph database",
            "ax.db plus its write-ahead journal. The code graph for this project.",
            true,
            named_files(&ax, &["ax.db", "ax.db-wal", "ax.db-shm"]),
        ),
        group(
            LOGGING,
            "MCP logging",
            "Dated verbose logs under .ax (mcp-verbose-*.log).",
            true,
            log_files(&ax),
        ),
        group(
            RUNTIME,
            "Daemon and local cache",
            "Daemon socket, audit reports, and captured turns.",
            true,
            runtime_paths(&ax),
        ),
        group(
            LOCAL_POLICY,
            "Local policy cache",
            "Cached policy under .ax/policy. Team files in .agents are separate.",
            true,
            existing_dir(ax.join("policy")),
        ),
        group(
            SHIP,
            "Ship configuration",
            "ship.toml and the last ship run record.",
            true,
            named_files(&ax, &["ship.toml", "ship-last-run.json"]),
        ),
        group(
            AGENTS,
            "Team rules and skills",
            ".agents in the working tree. Often committed — leave unchecked to keep it.",
            false,
            existing_dir(project_root.join(".agents")),
        ),
    ]
}

/// Switcher and global-index rows. These are not files inside the project folder.
pub fn leftover_groups() -> Vec<PurgeGroup> {
    vec![
        PurgeGroup {
            id: "recent",
            label: "Recent projects list",
            detail: "Remove this project from the header switcher.",
            bytes: 0,
            file_count: 1,
            default_on: true,
            empty: false,
        },
        PurgeGroup {
            id: "globalIndex",
            label: "Global project index",
            detail: "Delete this project's row in ~/.ax/global.db. Other projects stay.",
            bytes: 0,
            file_count: 1,
            default_on: true,
            empty: false,
        },
    ]
}

pub const MISSING_FOLDER_NOTE: &str = "This folder is already gone. Nothing on disk will be deleted. You can still drop the leftover entries below.";

pub fn known_file_group(id: &str) -> bool {
    matches!(
        id,
        DATABASE | LOGGING | RUNTIME | LOCAL_POLICY | SHIP | AGENTS
    )
}

/// Delete only the selected file groups. Ignores unknown ids. Refuses symlinks.
pub fn remove_selected(project_root: &Path, groups: &[String]) -> PurgeReport {
    let mut report = PurgeReport {
        removed: Vec::new(),
        errors: Vec::new(),
    };
    let planned = file_groups(project_root);
    for id in groups {
        if !known_file_group(id) {
            continue;
        }
        let Some(group) = planned.iter().find(|g| g.id == *id) else {
            continue;
        };
        let paths = paths_for(project_root, id);
        if paths.is_empty() && group.empty {
            continue;
        }
        for path in paths {
            if !is_inside_project(project_root, &path) {
                report
                    .errors
                    .push(format!("refused path outside project: {}", path.display()));
                continue;
            }
            match remove_one(&path) {
                Ok(Some(shown)) => report.removed.push(shown),
                Ok(None) => {}
                Err(err) => report.errors.push(err),
            }
        }
    }
    report
}

fn paths_for(project_root: &Path, id: &str) -> Vec<PathBuf> {
    let ax = project_root.join(".ax");
    match id {
        DATABASE => named_files(&ax, &["ax.db", "ax.db-wal", "ax.db-shm"]),
        LOGGING => log_files(&ax),
        RUNTIME => runtime_paths(&ax),
        LOCAL_POLICY => existing_dir(ax.join("policy")),
        SHIP => named_files(&ax, &["ship.toml", "ship-last-run.json"]),
        AGENTS => existing_dir(project_root.join(".agents")),
        _ => Vec::new(),
    }
}

fn group(
    id: &'static str,
    label: &'static str,
    detail: &'static str,
    default_on: bool,
    paths: Vec<PathBuf>,
) -> PurgeGroup {
    let mut bytes = 0u64;
    let mut file_count = 0u32;
    for path in &paths {
        let (b, n) = measure(path);
        bytes += b;
        file_count += n;
    }
    let empty = paths.is_empty();
    PurgeGroup {
        id,
        label,
        detail,
        bytes,
        file_count,
        default_on: default_on && !empty,
        empty,
    }
}

fn named_files(dir: &Path, names: &[&str]) -> Vec<PathBuf> {
    names
        .iter()
        .map(|name| dir.join(name))
        .filter(|path| path.symlink_metadata().is_ok())
        .collect()
}

fn existing_dir(path: PathBuf) -> Vec<PathBuf> {
    match path.symlink_metadata() {
        Ok(meta) if meta.is_dir() => vec![path],
        _ => Vec::new(),
    }
}

fn log_files(ax: &Path) -> Vec<PathBuf> {
    let Ok(read) = fs::read_dir(ax) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in read.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("mcp-verbose") && name.ends_with(".log") {
            out.push(entry.path());
        }
    }
    out.sort();
    out
}

fn runtime_paths(ax: &Path) -> Vec<PathBuf> {
    let mut paths = named_files(ax, &["daemon.json", "daemon.pid", "daemon.sock"]);
    paths.extend(existing_dir(ax.join("audit")));
    paths.extend(existing_dir(ax.join("turns")));
    paths
}

fn measure(path: &Path) -> (u64, u32) {
    let Ok(meta) = path.symlink_metadata() else {
        return (0, 0);
    };
    if meta.file_type().is_symlink() {
        return (0, 1);
    }
    if meta.is_file() {
        return (meta.len(), 1);
    }
    if !meta.is_dir() {
        return (meta.len(), 1);
    }
    let mut bytes = 0u64;
    let mut count = 0u32;
    let Ok(read) = fs::read_dir(path) else {
        return (0, 1);
    };
    for entry in read.flatten() {
        let (b, n) = measure(&entry.path());
        bytes += b;
        count += n;
    }
    (bytes, count.max(1))
}

fn is_inside_project(project_root: &Path, path: &Path) -> bool {
    let root = canonical_dir(project_root);
    let parent = match path.parent() {
        Some(parent) => canonical_dir(parent),
        None => return false,
    };
    if parent == root && path.file_name().and_then(|n| n.to_str()) == Some(".agents") {
        return true;
    }
    let ax = root.join(".ax");
    parent.starts_with(&ax) || path == ax
}

fn canonical_dir(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn remove_one(path: &Path) -> Result<Option<String>, String> {
    let meta = match path.symlink_metadata() {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(format!("{}: {err}", path.display())),
    };
    if meta.file_type().is_symlink() {
        return Err(format!("refused symlink {}", path.display()));
    }
    let shown = path.display().to_string();
    let result = if meta.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    result
        .map(|()| Some(shown))
        .map_err(|err| format!("{}: {err}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scaffold(root: &Path) {
        let ax = root.join(".ax");
        fs::create_dir_all(ax.join("audit")).unwrap();
        fs::create_dir_all(ax.join("turns")).unwrap();
        fs::create_dir_all(ax.join("policy")).unwrap();
        fs::create_dir_all(root.join(".agents").join("rules")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(ax.join("ax.db"), b"db").unwrap();
        fs::write(ax.join("ax.db-wal"), b"wal").unwrap();
        fs::write(ax.join("mcp-verbose-2026-09-25.log"), b"log").unwrap();
        fs::write(ax.join("daemon.json"), b"{}").unwrap();
        fs::write(ax.join("ship.toml"), b"ship").unwrap();
        fs::write(ax.join("audit").join("latest.json"), b"{}").unwrap();
        fs::write(
            root.join(".agents").join("rules").join("english-only.mdc"),
            b"rule",
        )
        .unwrap();
        fs::write(root.join("src").join("main.rs"), b"fn main() {}").unwrap();
    }

    #[test]
    fn missing_folder_only_offers_switcher_leftovers() {
        let groups = leftover_groups();
        let ids: Vec<_> = groups.iter().map(|g| g.id).collect();
        assert_eq!(ids, ["recent", "globalIndex"]);
        assert!(groups.iter().all(|g| g.default_on && !g.empty));
        let missing = std::env::temp_dir().join(format!("ax-purge-missing-{}", std::process::id()));
        let _ = fs::remove_dir_all(&missing);
        let report = remove_selected(&missing, &["database".into(), "logging".into()]);
        assert!(report.removed.is_empty(), "{:?}", report.removed);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(!missing.exists());
    }

    #[test]
    fn plan_defaults_keep_agents_and_ignore_source() {
        let dir = tempfile::tempdir().unwrap();
        scaffold(dir.path());
        let groups = file_groups(dir.path());
        let agents = groups.iter().find(|g| g.id == AGENTS).unwrap();
        assert!(!agents.default_on);
        assert!(!agents.empty);
        let database = groups.iter().find(|g| g.id == DATABASE).unwrap();
        assert!(database.default_on);
        assert!(database.bytes >= 2);
        assert!(groups.iter().all(|g| g.id != "src"));
    }

    #[test]
    fn logging_only_keeps_database_and_source() {
        let dir = tempfile::tempdir().unwrap();
        scaffold(dir.path());
        let report = remove_selected(dir.path(), &[LOGGING.to_string()]);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(!dir
            .path()
            .join(".ax")
            .join("mcp-verbose-2026-09-25.log")
            .exists());
        assert!(dir.path().join(".ax").join("ax.db").exists());
        assert!(dir.path().join("src").join("main.rs").exists());
        assert!(dir
            .path()
            .join(".agents")
            .join("rules")
            .join("english-only.mdc")
            .exists());
    }

    #[test]
    fn database_only_keeps_logs() {
        let dir = tempfile::tempdir().unwrap();
        scaffold(dir.path());
        let report = remove_selected(dir.path(), &[DATABASE.to_string()]);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(!dir.path().join(".ax").join("ax.db").exists());
        assert!(!dir.path().join(".ax").join("ax.db-wal").exists());
        assert!(dir
            .path()
            .join(".ax")
            .join("mcp-verbose-2026-09-25.log")
            .exists());
    }

    #[test]
    fn agents_group_removes_agents_dir_only_when_selected() {
        let dir = tempfile::tempdir().unwrap();
        scaffold(dir.path());
        let report = remove_selected(dir.path(), &[AGENTS.to_string()]);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(!dir.path().join(".agents").exists());
        assert!(dir.path().join(".ax").join("ship.toml").exists());
        assert!(dir.path().join("src").join("main.rs").exists());
    }

    #[test]
    fn unknown_group_deletes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        scaffold(dir.path());
        let report = remove_selected(dir.path(), &["../src".to_string(), "source".to_string()]);
        assert!(report.removed.is_empty());
        assert!(dir.path().join("src").join("main.rs").exists());
        assert!(dir.path().join(".ax").join("ax.db").exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        scaffold(dir.path());
        let outside = dir.path().join("src").join("main.rs");
        let link = dir.path().join(".ax").join("mcp-verbose-link.log");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let report = remove_selected(dir.path(), &[LOGGING.to_string()]);
        assert!(
            report.errors.iter().any(|e| e.contains("symlink")),
            "{:?}",
            report.errors
        );
        assert!(outside.exists());
        assert!(link.symlink_metadata().is_ok());
    }
}
