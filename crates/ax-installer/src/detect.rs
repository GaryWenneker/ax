//! Whether an IDE app is installed on this machine. Config folders never count:
//! ax writes into them itself, so they exist after any connect.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Mac,
    Windows,
    Linux,
}

impl Os {
    pub fn current() -> Os {
        if cfg!(target_os = "macos") {
            Os::Mac
        } else if cfg!(target_os = "windows") {
            Os::Windows
        } else {
            Os::Linux
        }
    }
}

/// Where to look; `local_app_data` and `program_files` are Windows only.
pub struct Places {
    pub home: PathBuf,
    pub local_app_data: Option<PathBuf>,
    pub program_files: Option<PathBuf>,
}

/// What the filesystem and PATH look like, so detection can be tested with fakes.
pub trait Probe {
    fn exists(&self, path: &Path) -> bool;
    fn on_path(&self, bin: &str) -> bool;
    fn dir_names(&self, dir: &Path) -> Vec<String>;
}

/// Install names per app: (macOS bundle, Windows folder/exe, Linux binaries, Linux dirs).
fn app_names(target: &str) -> Option<(&'static str, &'static str, &'static [&'static str], &'static [&'static str])> {
    Some(match target {
        "cursor" => ("Cursor.app", "cursor/Cursor.exe", &["cursor"], &["/opt/Cursor", "/usr/share/cursor"]),
        "vscode" => ("Visual Studio Code.app", "Microsoft VS Code/Code.exe", &["code"], &["/usr/share/code"]),
        "windsurf" => ("Windsurf.app", "Windsurf/Windsurf.exe", &["windsurf"], &["/usr/share/windsurf"]),
        "zed" => ("Zed.app", "Zed/Zed.exe", &["zed", "zeditor"], &[]),
        "kiro" => ("Kiro.app", "Kiro/Kiro.exe", &["kiro"], &["/usr/share/kiro"]),
        "antigravity" => ("Antigravity.app", "Antigravity/Antigravity.exe", &["antigravity"], &[]),
        _ => return None,
    })
}

pub fn app_installed(target: &str, os: Os, places: &Places, probe: &dyn Probe) -> bool {
    if target == "continue" {
        return [".vscode", ".cursor"].iter().any(|ide| {
            probe
                .dir_names(&places.home.join(ide).join("extensions"))
                .iter()
                .any(|n| n.starts_with("continue.continue-"))
        });
    }
    let Some((mac, win, bins, dirs)) = app_names(target) else {
        return false;
    };
    match os {
        Os::Mac => [PathBuf::from("/Applications"), places.home.join("Applications")]
            .iter()
            .any(|root| probe.exists(&root.join(mac))),
        Os::Windows => {
            let local = places.local_app_data.as_ref().map(|d| d.join("Programs").join(win));
            let global = places.program_files.as_ref().map(|d| d.join(win));
            local.iter().chain(global.iter()).any(|p| probe.exists(p))
        }
        Os::Linux => bins.iter().any(|b| probe.on_path(b)) || dirs.iter().any(|d| probe.exists(Path::new(d))),
    }
}

/// The real filesystem and PATH.
pub struct System;

impl Probe for System {
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
    fn on_path(&self, bin: &str) -> bool {
        let Some(path) = std::env::var_os("PATH") else {
            return false;
        };
        std::env::split_paths(&path).any(|d| d.join(bin).is_file())
    }
    fn dir_names(&self, dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default()
    }
}

/// `app_installed` for this machine.
pub fn app_installed_here(target: &str) -> bool {
    let Some(home) = ax_utils::paths::home_dir() else {
        return false;
    };
    let places = Places {
        home,
        local_app_data: std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
        program_files: std::env::var_os("ProgramFiles").map(PathBuf::from),
    };
    app_installed(target, Os::current(), &places, &System)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    #[derive(Default)]
    struct Fake {
        paths: HashSet<PathBuf>,
        bins: HashSet<String>,
        dirs: HashMap<PathBuf, Vec<String>>,
    }

    impl Probe for Fake {
        fn exists(&self, path: &Path) -> bool {
            self.paths.contains(path)
        }
        fn on_path(&self, bin: &str) -> bool {
            self.bins.contains(bin)
        }
        fn dir_names(&self, dir: &Path) -> Vec<String> {
            self.dirs.get(dir).cloned().unwrap_or_default()
        }
    }

    fn places() -> Places {
        Places {
            home: PathBuf::from("/h"),
            local_app_data: Some(PathBuf::from("/lad")),
            program_files: Some(PathBuf::from("/pf")),
        }
    }

    fn with_path(p: &str) -> Fake {
        Fake { paths: [PathBuf::from(p)].into(), ..Fake::default() }
    }

    fn with_bin(b: &str) -> Fake {
        Fake { bins: [b.to_string()].into(), ..Fake::default() }
    }

    fn found(target: &str, os: Os, probe: &Fake) -> bool {
        app_installed(target, os, &places(), probe)
    }

    #[test]
    fn mac_apps_in_system_or_user_applications() {
        let cases = [
            ("cursor", "Cursor.app"),
            ("vscode", "Visual Studio Code.app"),
            ("windsurf", "Windsurf.app"),
            ("zed", "Zed.app"),
            ("kiro", "Kiro.app"),
            ("antigravity", "Antigravity.app"),
        ];
        for (t, app) in cases {
            assert!(found(t, Os::Mac, &with_path(&format!("/Applications/{app}"))), "{t}");
            assert!(found(t, Os::Mac, &with_path(&format!("/h/Applications/{app}"))), "{t} user");
            assert!(!found(t, Os::Mac, &with_path("/Applications/Other.app")), "{t} other");
        }
    }

    #[test]
    fn windows_apps_in_local_programs() {
        let cases = [
            ("cursor", "/lad/Programs/cursor/Cursor.exe"),
            ("vscode", "/lad/Programs/Microsoft VS Code/Code.exe"),
            ("vscode", "/pf/Microsoft VS Code/Code.exe"),
            ("windsurf", "/lad/Programs/Windsurf/Windsurf.exe"),
            ("zed", "/lad/Programs/Zed/Zed.exe"),
            ("kiro", "/lad/Programs/Kiro/Kiro.exe"),
            ("antigravity", "/lad/Programs/Antigravity/Antigravity.exe"),
        ];
        for (t, p) in cases {
            assert!(found(t, Os::Windows, &with_path(p)), "{t} {p}");
            assert!(!found(t, Os::Mac, &with_path(p)), "{t} not on mac");
        }
    }

    #[test]
    fn linux_binaries_or_install_dirs() {
        let bins = [
            ("cursor", "cursor"),
            ("vscode", "code"),
            ("windsurf", "windsurf"),
            ("zed", "zed"),
            ("zed", "zeditor"),
            ("kiro", "kiro"),
            ("antigravity", "antigravity"),
        ];
        for (t, b) in bins {
            assert!(found(t, Os::Linux, &with_bin(b)), "{t} {b}");
        }
        for (t, p) in [
            ("cursor", "/opt/Cursor"),
            ("cursor", "/usr/share/cursor"),
            ("vscode", "/usr/share/code"),
            ("windsurf", "/usr/share/windsurf"),
            ("kiro", "/usr/share/kiro"),
        ] {
            assert!(found(t, Os::Linux, &with_path(p)), "{t} {p}");
        }
        assert!(!found("cursor", Os::Linux, &with_bin("code")));
    }

    #[test]
    fn continue_needs_its_extension() {
        for ext_dir in ["/h/.vscode/extensions", "/h/.cursor/extensions"] {
            let mut fake = Fake::default();
            fake.dirs.insert(PathBuf::from(ext_dir), vec!["continue.continue-1.2.3".into()]);
            for os in [Os::Mac, Os::Windows, Os::Linux] {
                assert!(found("continue", os, &fake), "{ext_dir} {os:?}");
            }
        }
        let mut other = Fake::default();
        other.dirs.insert(PathBuf::from("/h/.vscode/extensions"), vec!["ms-python.python-1".into()]);
        assert!(!found("continue", Os::Mac, &other));
    }

    #[test]
    fn config_folders_alone_are_not_found() {
        let fake = Fake {
            paths: [
                "/h/.cursor", "/h/.cursor/mcp.json", "/h/.codex", "/h/.kiro", "/h/.gemini",
                "/h/.claude.json", "/h/.continue", "/h/.vscode", "/h/.codeium/windsurf",
            ]
            .iter()
            .map(PathBuf::from)
            .collect(),
            ..Fake::default()
        };
        for t in crate::targets::TARGETS {
            for os in [Os::Mac, Os::Windows, Os::Linux] {
                assert!(!found(t, os, &fake), "{t} {os:?}");
            }
        }
    }

    #[test]
    fn cli_only_targets_have_no_app() {
        let fake = with_path("/Applications/Cursor.app");
        for t in ["claude", "codex", "opencode", "hermes", "gemini"] {
            assert!(!found(t, Os::Mac, &fake), "{t}");
        }
    }
}
