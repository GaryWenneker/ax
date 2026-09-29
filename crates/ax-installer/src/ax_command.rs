//! Which `ax` path the installer writes into agent configs and hooks.

use std::ffi::OsStr;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxCommand {
    /// Written into configs.
    pub path: String,
    /// The binary doing the install, when known.
    pub running: Option<String>,
    pub on_path: bool,
}

const EXE_NAME: &str = if cfg!(windows) { "ax.exe" } else { "ax" };

/// The first `ax` on PATH as written there (a shim stays a shim); else the running binary.
pub fn resolve(path_env: Option<&OsStr>, running: Option<PathBuf>) -> AxCommand {
    let running_str = running.as_ref().map(|p| p.to_string_lossy().into_owned());
    match path_env.and_then(first_on_path) {
        Some(found) => AxCommand {
            path: found.to_string_lossy().into_owned(),
            running: running_str,
            on_path: true,
        },
        None => AxCommand {
            path: running_str.clone().unwrap_or_else(|| "ax".to_string()),
            running: running_str,
            on_path: false,
        },
    }
}

fn first_on_path(path_env: &OsStr) -> Option<PathBuf> {
    std::env::split_paths(path_env)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(EXE_NAME))
        .find(|candidate| is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &std::path::Path) -> bool {
    path.is_file()
}

fn same_file(a: &str, b: &str) -> bool {
    a == b
        || matches!(
            (std::fs::canonicalize(a), std::fs::canonicalize(b)),
            (Ok(x), Ok(y)) if x == y
        )
}

pub fn current() -> AxCommand {
    resolve(
        std::env::var_os("PATH").as_deref(),
        std::env::current_exe().ok(),
    )
}

impl AxCommand {
    /// Install-report line when the written path is not simply the running binary on PATH.
    pub fn note(&self) -> Option<String> {
        if !self.on_path {
            return Some(format!(
                "No ax on PATH; agents run this binary: {}",
                self.path
            ));
        }
        match &self.running {
            Some(running) if !same_file(&self.path, running) => Some(format!(
                "Agents run ax from PATH: {} (this binary: {running})",
                self.path
            )),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn exe(dir: &Path) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(if cfg!(windows) { "ax.exe" } else { "ax" });
        std::fs::write(&path, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }

    fn path_env(dirs: &[&Path]) -> std::ffi::OsString {
        std::env::join_paths(dirs).unwrap()
    }

    #[test]
    fn c1_first_ax_on_path_wins_as_found() {
        let t = tempfile::tempdir().unwrap();
        let running = exe(&t.path().join("build"));
        let empty = t.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        let first = exe(&t.path().join("shim"));
        exe(&t.path().join("later"));
        let env = path_env(&[&empty, &t.path().join("shim"), &t.path().join("later")]);
        let cmd = resolve(Some(&env), Some(running.clone()));
        assert_eq!(cmd.path, first.to_string_lossy());
        assert!(cmd.on_path);
    }

    #[cfg(unix)]
    #[test]
    fn c1_a_symlinked_shim_is_not_resolved() {
        let t = tempfile::tempdir().unwrap();
        let running = exe(&t.path().join("build"));
        let shim_dir = t.path().join("shim");
        std::fs::create_dir_all(&shim_dir).unwrap();
        std::os::unix::fs::symlink(&running, shim_dir.join("ax")).unwrap();
        let cmd = resolve(Some(&path_env(&[&shim_dir])), Some(running));
        assert_eq!(cmd.path, shim_dir.join("ax").to_string_lossy());
    }

    #[cfg(unix)]
    #[test]
    fn c1_skips_a_file_that_is_not_executable_and_relative_entries() {
        use std::os::unix::fs::PermissionsExt;
        let t = tempfile::tempdir().unwrap();
        let running = exe(&t.path().join("build"));
        let plain = exe(&t.path().join("plain"));
        std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o644)).unwrap();
        let good = exe(&t.path().join("good"));
        exe(&t.path().join("rel"));
        let relative = relative_from_cwd(&t.path().join("rel"));
        assert!(
            relative.join("ax").is_file(),
            "the relative entry holds an executable ax"
        );
        let env = path_env(&[&relative, &t.path().join("plain"), &t.path().join("good")]);
        let cmd = resolve(Some(&env), Some(running));
        assert_eq!(cmd.path, good.to_string_lossy());
    }

    /// `dir` as a path relative to the current directory (`../../…/dir`).
    #[cfg(unix)]
    fn relative_from_cwd(dir: &Path) -> PathBuf {
        let cwd = std::env::current_dir().unwrap().canonicalize().unwrap();
        let dir = dir.canonicalize().unwrap();
        let up = cwd.components().count() - 1;
        let mut rel: PathBuf = std::iter::repeat_n("..", up).collect();
        rel.push(dir.strip_prefix("/").unwrap());
        rel
    }

    #[test]
    fn c2_without_ax_on_path_the_running_binary_is_used_and_said() {
        let t = tempfile::tempdir().unwrap();
        let running = exe(&t.path().join("build"));
        let empty = t.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        let cmd = resolve(Some(&path_env(&[&empty])), Some(running.clone()));
        assert_eq!(cmd.path, running.to_string_lossy());
        assert!(!cmd.on_path);
        let note = cmd.note().expect("a note");
        assert!(note.contains("No ax on PATH"), "{note}");
        assert!(note.contains(&*running.to_string_lossy()), "{note}");
    }

    #[test]
    fn c3_the_note_names_both_paths_when_they_differ() {
        let t = tempfile::tempdir().unwrap();
        let running = exe(&t.path().join("build"));
        let shim = exe(&t.path().join("shim"));
        let cmd = resolve(
            Some(&path_env(&[&t.path().join("shim")])),
            Some(running.clone()),
        );
        let note = cmd.note().expect("a note");
        assert!(note.contains(&*shim.to_string_lossy()), "{note}");
        assert!(note.contains(&*running.to_string_lossy()), "{note}");
    }

    #[test]
    fn c3_no_note_when_the_path_ax_is_the_running_binary() {
        let t = tempfile::tempdir().unwrap();
        let running = exe(&t.path().join("bin"));
        let cmd = resolve(Some(&path_env(&[&t.path().join("bin")])), Some(running));
        assert_eq!(cmd.note(), None);
    }
}
