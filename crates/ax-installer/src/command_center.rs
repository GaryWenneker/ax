//! Command Center (`ax web`) inside the IDE: the VS Code-family extension,
//! the JetBrains plugin, and the Zed task that opens it in the browser.

use std::path::{Path, PathBuf};

use crate::detect::{Os, Places, Probe};
use crate::report::FileAction;

pub const EXTENSION_ID: &str = "wenneker.ax-command-center";
/// Must match `ide/vscode/package.json`; checked by a test.
pub const EXTENSION_VERSION: &str = "0.1.0";
pub const JETBRAINS_JAR: &str = "ax-command-center.jar";
pub const ZED_TASK_LABEL: &str = "ax: Open Command Center";

const ZED_START: &str = "// ax:command-center:start";
const ZED_END: &str = "// ax:command-center:end";

/// Runs an IDE CLI, so installs can be tested without touching real IDEs.
pub trait Runner {
    fn run(&self, program: &Path, args: &[&str]) -> Result<(), String>;
}

struct VscodeIde {
    cli: &'static str,
    mac_app: &'static str,
    win_dir: &'static str,
    ext_dir: &'static str,
}

fn vscode_ide(target: &str) -> Option<VscodeIde> {
    let (cli, mac_app, win_dir, ext_dir) = match target {
        "vscode" => ("code", "Visual Studio Code.app", "Microsoft VS Code", ".vscode"),
        "cursor" => ("cursor", "Cursor.app", "cursor", ".cursor"),
        "windsurf" => ("windsurf", "Windsurf.app", "Windsurf", ".windsurf"),
        "antigravity" => ("antigravity", "Antigravity.app", "Antigravity", ".antigravity"),
        "kiro" => ("kiro", "Kiro.app", "Kiro", ".kiro"),
        _ => return None,
    };
    Some(VscodeIde { cli, mac_app, win_dir, ext_dir })
}

pub fn is_vscode_family(target: &str) -> bool {
    vscode_ide(target).is_some()
}

/// The IDE's command-line tool: PATH first, then the copy inside the app install.
pub fn resolve_vscode_cli(target: &str, os: Os, places: &Places, probe: &dyn Probe) -> Option<PathBuf> {
    let ide = vscode_ide(target)?;
    if probe.on_path(ide.cli) {
        return Some(PathBuf::from(ide.cli));
    }
    let candidates: Vec<PathBuf> = match os {
        Os::Mac => [PathBuf::from("/Applications"), places.home.join("Applications")]
            .iter()
            .map(|root| root.join(ide.mac_app).join("Contents/Resources/app/bin").join(ide.cli))
            .collect(),
        Os::Windows => places
            .local_app_data
            .iter()
            .map(|d| d.join("Programs"))
            .chain(places.program_files.iter().cloned())
            .map(|d| d.join(ide.win_dir).join("bin").join(format!("{}.cmd", ide.cli)))
            .collect(),
        Os::Linux => Vec::new(),
    };
    candidates.into_iter().find(|p| probe.exists(p))
}

/// Installs the bundled `.vsix`; never fails the connect, the outcome is a note.
pub fn install_vscode_extension(
    display_name: &str,
    cli: Option<&Path>,
    vsix: &Path,
    runner: &dyn Runner,
) -> String {
    let Some(cli) = cli else {
        return format!("Command Center panel not installed: {display_name} command-line tool not found.");
    };
    let vsix = vsix.to_string_lossy();
    match runner.run(cli, &["--install-extension", &vsix, "--force"]) {
        Ok(()) => "Command Center panel installed: run \"ax: Open Command Center\" or click ax in the status bar.".into(),
        Err(e) => format!("Command Center panel not installed: {e}"),
    }
}

pub fn uninstall_vscode_extension(cli: Option<&Path>, runner: &dyn Runner) -> Option<String> {
    let cli = cli?;
    Some(match runner.run(cli, &["--uninstall-extension", EXTENSION_ID]) {
        Ok(()) => "Command Center panel removed.".into(),
        Err(e) => format!("Command Center panel not removed: {e}"),
    })
}

pub fn vscode_panel_installed(target: &str, home: &Path, probe: &dyn Probe) -> bool {
    let Some(ide) = vscode_ide(target) else {
        return false;
    };
    let prefix = format!("{EXTENSION_ID}-");
    probe
        .dir_names(&home.join(ide.ext_dir).join("extensions"))
        .iter()
        .any(|n| n.starts_with(&prefix))
}

/// The bundled extension version is already installed, so the IDE CLI need not run.
pub fn vscode_panel_current(target: &str, home: &Path, probe: &dyn Probe) -> bool {
    let Some(ide) = vscode_ide(target) else {
        return false;
    };
    let current = format!("{EXTENSION_ID}-{EXTENSION_VERSION}");
    probe.dir_names(&home.join(ide.ext_dir).join("extensions")).contains(&current)
}

fn zed_block(open_command: &str) -> String {
    let task = serde_json::json!({
        "label": ZED_TASK_LABEL,
        "command": open_command,
        "reveal": "never",
        "hide": "on_success",
    });
    let task = serde_json::to_string(&task).unwrap_or_default().replace("\":", "\": ").replace(",\"", ", \"");
    format!("\n  {ZED_START}\n  {task},\n  {ZED_END}")
}

/// Byte offset just after the opening `[` of the task array, skipping comment lines.
fn zed_array_start(content: &str) -> Option<usize> {
    let mut offset = 0;
    for line in content.split_inclusive('\n') {
        if !line.trim_start().starts_with("//") {
            if let Some(i) = line.find('[') {
                return Some(offset + i + 1);
            }
        }
        offset += line.len();
    }
    None
}

/// `tasks.json` with the ax task added once; other tasks and comments stay as they are.
pub fn zed_tasks_with_ax(existing: Option<&str>, open_command: &str) -> String {
    let existing = existing.unwrap_or("");
    if zed_has_ax_task(existing) {
        return existing.to_string();
    }
    let block = zed_block(open_command);
    match zed_array_start(existing) {
        Some(at) => format!("{}{}{}", &existing[..at], block, &existing[at..]),
        None => format!("[{block}\n]\n"),
    }
}

/// `tasks.json` without the ax task, or `None` when there was nothing to remove.
pub fn zed_tasks_without_ax(existing: &str) -> Option<String> {
    let start = existing.find(ZED_START)?;
    let start = existing[..start].rfind('\n').unwrap_or(start);
    let end = existing[start..].find(ZED_END)? + start + ZED_END.len();
    Some(format!("{}{}", &existing[..start], &existing[end..]))
}

pub fn zed_has_ax_task(content: &str) -> bool {
    content.contains(ZED_START)
}

const JETBRAINS_PRODUCTS: &[&str] = &[
    "IntelliJIdea", "IdeaIC", "PyCharm", "PyCharmCE", "WebStorm", "GoLand", "Rider", "CLion",
    "PhpStorm", "RubyMine", "DataGrip", "RustRover", "DataSpell", "Aqua",
];

fn is_jetbrains_product_dir(name: &str) -> bool {
    JETBRAINS_PRODUCTS.iter().any(|p| {
        name.strip_prefix(p).is_some_and(|v| {
            !v.is_empty() && v.starts_with(|c: char| c.is_ascii_digit()) && v.chars().all(|c| c.is_ascii_digit() || c == '.')
        })
    })
}

/// Per-product config folders (`IntelliJIdea2025.2`, `Rider2025.1`, …) under the JetBrains base.
pub fn jetbrains_config_dirs(base: &Path, probe: &dyn Probe) -> Vec<PathBuf> {
    probe
        .dir_names(base)
        .into_iter()
        .filter(|n| is_jetbrains_product_dir(n))
        .map(|n| base.join(n))
        .collect()
}

pub fn jetbrains_base(os: Os, places: &Places, app_data: Option<&Path>) -> PathBuf {
    match os {
        Os::Mac => places.home.join("Library/Application Support/JetBrains"),
        Os::Linux => places.home.join(".config/JetBrains"),
        Os::Windows => app_data
            .map(Path::to_path_buf)
            .unwrap_or_else(|| places.home.join("AppData/Roaming"))
            .join("JetBrains"),
    }
}

/// Writes the plugin jar into `<product>/plugins/` of every product folder.
pub fn install_jetbrains_plugin(product_dirs: &[PathBuf], jar: &[u8]) -> Result<Vec<(PathBuf, FileAction)>, String> {
    let mut out = Vec::new();
    for dir in product_dirs {
        let path = dir.join("plugins").join(JETBRAINS_JAR);
        let action = match std::fs::read(&path) {
            Ok(current) if current == jar => FileAction::Unchanged,
            Ok(_) => FileAction::Updated,
            Err(_) => FileAction::Created,
        };
        if action != FileAction::Unchanged {
            std::fs::create_dir_all(dir.join("plugins")).map_err(|e| format!("{}: {e}", dir.display()))?;
            std::fs::write(&path, jar).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        out.push((path, action));
    }
    Ok(out)
}

/// Removes only the ax jar; returns the paths that were removed.
pub fn uninstall_jetbrains_plugin(product_dirs: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut removed = Vec::new();
    for dir in product_dirs {
        let path = dir.join("plugins").join(JETBRAINS_JAR);
        if path.is_file() {
            std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            removed.push(path);
        }
    }
    Ok(removed)
}

pub const VSIX: &[u8] = include_bytes!("../assets/ax-command-center.vsix");
pub const JETBRAINS_PLUGIN: &[u8] = include_bytes!("../assets/ax-command-center.jar");

/// Runs the real IDE CLI and turns a non-zero exit into its stderr.
pub struct SystemRunner;

impl Runner for SystemRunner {
    fn run(&self, program: &Path, args: &[&str]) -> Result<(), String> {
        let out = std::process::Command::new(program)
            .args(args)
            .output()
            .map_err(|e| format!("{}: {e}", program.display()))?;
        if out.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&out.stderr);
        let last = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
        Err(format!("{} exited with {}{}", program.display(), out.status, if last.is_empty() { String::new() } else { format!(": {last}") }))
    }
}

fn places_here() -> Option<Places> {
    Some(Places {
        home: dirs::home_dir()?,
        local_app_data: std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
        program_files: std::env::var_os("ProgramFiles").map(PathBuf::from),
    })
}

pub fn vscode_cli_here(target: &str) -> Option<PathBuf> {
    resolve_vscode_cli(target, Os::current(), &places_here()?, &crate::detect::System)
}

pub fn jetbrains_dirs_here() -> Vec<PathBuf> {
    let Some(places) = places_here() else {
        return Vec::new();
    };
    let app_data = std::env::var_os("APPDATA").map(PathBuf::from);
    let base = jetbrains_base(Os::current(), &places, app_data.as_deref());
    jetbrains_config_dirs(&base, &crate::detect::System)
}

/// Writes the bundled `.vsix` to a temp file the IDE CLI can read.
pub fn vsix_on_disk() -> Result<PathBuf, String> {
    let path = std::env::temp_dir().join(format!("ax-command-center-{}.vsix", env!("CARGO_PKG_VERSION")));
    std::fs::write(&path, VSIX).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

pub fn vscode_panel_installed_here(target: &str) -> bool {
    places_here().is_some_and(|p| vscode_panel_installed(target, &p.home, &crate::detect::System))
}

/// `AX_NO_IDE_PANEL=1` skips extension and plugin installs (tests, CI).
pub fn panels_disabled() -> bool {
    std::env::var("AX_NO_IDE_PANEL").is_ok_and(|v| v == "1")
}

/// Installs the VS Code-family extension unless the current version is already there.
pub fn install_vscode_panel_here(target: &str, display_name: &str) -> Option<String> {
    if panels_disabled() || !is_vscode_family(target) {
        return None;
    }
    if places_here().is_some_and(|p| vscode_panel_current(target, &p.home, &crate::detect::System)) {
        return Some("Command Center panel up to date.".into());
    }
    let Some(cli) = vscode_cli_here(target) else {
        return Some(install_vscode_extension(display_name, None, Path::new(""), &SystemRunner));
    };
    Some(match vsix_on_disk() {
        Ok(vsix) => install_vscode_extension(display_name, Some(&cli), &vsix, &SystemRunner),
        Err(e) => format!("Command Center panel not installed: {e}"),
    })
}

pub fn uninstall_vscode_panel_here(target: &str) -> Option<String> {
    if panels_disabled() || !vscode_panel_installed_here(target) {
        return None;
    }
    uninstall_vscode_extension(vscode_cli_here(target).as_deref(), &SystemRunner)
}

/// Command that opens a URL in the default browser on this OS, for the Zed task.
pub fn open_url_command(url: &str) -> String {
    match Os::current() {
        Os::Mac => format!("open {url}"),
        Os::Windows => format!("start {url}"),
        Os::Linux => format!("xdg-open {url}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ax-cc-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn jetbrains_install_and_uninstall_touch_only_own_file() {
        let base = scratch("jb");
        let idea = base.join("IntelliJIdea2025.2");
        let rider = base.join("Rider2025.1");
        std::fs::create_dir_all(idea.join("plugins/other-plugin")).unwrap();
        std::fs::write(idea.join("plugins/other.jar"), b"x").unwrap();
        std::fs::create_dir_all(&rider).unwrap();
        let dirs = vec![idea.clone(), rider.clone()];

        let first = install_jetbrains_plugin(&dirs, b"jar-v1").unwrap();
        assert_eq!(
            first,
            vec![
                (idea.join("plugins").join(JETBRAINS_JAR), FileAction::Created),
                (rider.join("plugins").join(JETBRAINS_JAR), FileAction::Created),
            ]
        );
        assert_eq!(std::fs::read(rider.join("plugins").join(JETBRAINS_JAR)).unwrap(), b"jar-v1");
        let again = install_jetbrains_plugin(&dirs, b"jar-v1").unwrap();
        assert!(again.iter().all(|(_, a)| *a == FileAction::Unchanged));
        let newer = install_jetbrains_plugin(&dirs, b"jar-v2").unwrap();
        assert!(newer.iter().all(|(_, a)| *a == FileAction::Updated));

        let removed = uninstall_jetbrains_plugin(&dirs).unwrap();
        assert_eq!(removed.len(), 2);
        assert!(!idea.join("plugins").join(JETBRAINS_JAR).exists());
        assert!(idea.join("plugins/other.jar").exists());
        assert!(idea.join("plugins/other-plugin").is_dir());
        assert!(uninstall_jetbrains_plugin(&dirs).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }
    use std::cell::RefCell;
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

    #[derive(Default)]
    struct Recorder {
        calls: RefCell<Vec<(PathBuf, Vec<String>)>>,
        fail: Option<String>,
    }

    impl Runner for Recorder {
        fn run(&self, program: &Path, args: &[&str]) -> Result<(), String> {
            self.calls
                .borrow_mut()
                .push((program.to_path_buf(), args.iter().map(|a| a.to_string()).collect()));
            match &self.fail {
                Some(e) => Err(e.clone()),
                None => Ok(()),
            }
        }
    }

    fn places() -> Places {
        Places {
            home: PathBuf::from("/h"),
            local_app_data: Some(PathBuf::from("/lad")),
            program_files: None,
        }
    }

    #[test]
    fn resolve_vscode_cli_prefers_path_then_app_bundle() {
        let on_path = Fake { bins: ["cursor".into()].into(), ..Fake::default() };
        assert_eq!(resolve_vscode_cli("cursor", Os::Mac, &places(), &on_path), Some(PathBuf::from("cursor")));

        let bundle = "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code";
        let mac = Fake { paths: [PathBuf::from(bundle)].into(), ..Fake::default() };
        assert_eq!(resolve_vscode_cli("vscode", Os::Mac, &places(), &mac), Some(PathBuf::from(bundle)));

        let user = "/h/Applications/Kiro.app/Contents/Resources/app/bin/kiro";
        let mac_user = Fake { paths: [PathBuf::from(user)].into(), ..Fake::default() };
        assert_eq!(resolve_vscode_cli("kiro", Os::Mac, &places(), &mac_user), Some(PathBuf::from(user)));

        let win = "/lad/Programs/Windsurf/bin/windsurf.cmd";
        let windows = Fake { paths: [PathBuf::from(win)].into(), ..Fake::default() };
        assert_eq!(resolve_vscode_cli("windsurf", Os::Windows, &places(), &windows), Some(PathBuf::from(win)));
        assert_eq!(resolve_vscode_cli("windsurf", Os::Mac, &places(), &windows), None);

        assert_eq!(resolve_vscode_cli("vscode", Os::Linux, &places(), &Fake::default()), None);
        assert_eq!(resolve_vscode_cli("zed", Os::Mac, &places(), &on_path), None);
    }

    #[test]
    fn vscode_extension_install_runs_cli_with_vsix() {
        let runner = Recorder::default();
        let note = install_vscode_extension("Cursor", Some(Path::new("/bin/cursor")), Path::new("/tmp/ax.vsix"), &runner);
        assert_eq!(
            runner.calls.borrow().as_slice(),
            &[(PathBuf::from("/bin/cursor"), vec!["--install-extension".into(), "/tmp/ax.vsix".into(), "--force".into()])]
        );
        assert_eq!(note, "Command Center panel installed: run \"ax: Open Command Center\" or click ax in the status bar.");

        let gone = Recorder::default();
        assert_eq!(
            uninstall_vscode_extension(Some(Path::new("/bin/code")), &gone),
            Some("Command Center panel removed.".into())
        );
        assert_eq!(
            gone.calls.borrow().as_slice(),
            &[(PathBuf::from("/bin/code"), vec!["--uninstall-extension".into(), EXTENSION_ID.into()])]
        );
    }

    #[test]
    fn missing_cli_is_note_not_error() {
        let runner = Recorder::default();
        let note = install_vscode_extension("Kiro", None, Path::new("/tmp/ax.vsix"), &runner);
        assert_eq!(note, "Command Center panel not installed: Kiro command-line tool not found.");
        assert!(runner.calls.borrow().is_empty());

        let failing = Recorder { fail: Some("exit 1".into()), ..Recorder::default() };
        let note = install_vscode_extension("Cursor", Some(Path::new("cursor")), Path::new("/tmp/ax.vsix"), &failing);
        assert_eq!(note, "Command Center panel not installed: exit 1");
        assert_eq!(uninstall_vscode_extension(None, &runner), None);
        assert_eq!(
            uninstall_vscode_extension(Some(Path::new("cursor")), &failing),
            Some("Command Center panel not removed: exit 1".into())
        );
    }

    #[test]
    fn extension_version_matches_package_json() {
        let pkg = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../ide/vscode/package.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&pkg).unwrap();
        assert_eq!(v["version"], EXTENSION_VERSION);
        assert_eq!(format!("{}.{}", v["publisher"].as_str().unwrap(), v["name"].as_str().unwrap()), EXTENSION_ID);
    }

    #[test]
    fn current_panel_version_is_detected() {
        let mut fake = Fake::default();
        fake.dirs.insert(PathBuf::from("/h/.cursor/extensions"), vec![format!("{EXTENSION_ID}-{EXTENSION_VERSION}")]);
        fake.dirs.insert(PathBuf::from("/h/.kiro/extensions"), vec![format!("{EXTENSION_ID}-0.0.1")]);
        assert!(vscode_panel_current("cursor", Path::new("/h"), &fake));
        assert!(!vscode_panel_current("kiro", Path::new("/h"), &fake));
        assert!(vscode_panel_installed("kiro", Path::new("/h"), &fake));
    }

    #[test]
    fn panel_detected_from_extensions_folder() {
        let mut fake = Fake::default();
        fake.dirs.insert(PathBuf::from("/h/.cursor/extensions"), vec!["wenneker.ax-command-center-0.1.0".into()]);
        fake.dirs.insert(PathBuf::from("/h/.vscode/extensions"), vec!["wenneker.ax-other-1.0.0".into()]);
        assert!(vscode_panel_installed("cursor", Path::new("/h"), &fake));
        assert!(!vscode_panel_installed("vscode", Path::new("/h"), &fake));
        assert!(!vscode_panel_installed("zed", Path::new("/h"), &fake));
    }

    #[test]
    fn zed_task_merge_is_idempotent_and_preserves_existing() {
        let fresh = zed_tasks_with_ax(None, "open http://127.0.0.1:7070");
        assert!(zed_has_ax_task(&fresh));
        assert!(fresh.contains("\"command\": \"open http://127.0.0.1:7070\""));
        assert_eq!(zed_tasks_with_ax(Some(&fresh), "open http://127.0.0.1:7070"), fresh);

        let existing = "// my tasks\n[\n  { \"label\": \"build\", \"command\": \"make\" }\n]\n";
        let merged = zed_tasks_with_ax(Some(existing), "open http://127.0.0.1:7070");
        assert!(merged.starts_with("// my tasks\n["));
        assert!(merged.contains("{ \"label\": \"build\", \"command\": \"make\" }"));
        assert_eq!(merged.matches(ZED_TASK_LABEL).count(), 1);
        assert_eq!(zed_tasks_with_ax(Some(&merged), "open http://127.0.0.1:7070"), merged);

        let empty = zed_tasks_with_ax(Some("  \n"), "xdg-open http://127.0.0.1:7070");
        assert!(zed_has_ax_task(&empty));
        assert!(!zed_has_ax_task(existing));
    }

    #[test]
    fn zed_uninstall_removes_only_ax_task() {
        let existing = "// my tasks\n[\n  { \"label\": \"build\", \"command\": \"make\" }\n]\n";
        let merged = zed_tasks_with_ax(Some(existing), "open http://127.0.0.1:7070");
        assert_eq!(zed_tasks_without_ax(&merged).as_deref(), Some(existing));
        assert_eq!(zed_tasks_without_ax(existing), None);
    }

    #[test]
    fn jetbrains_detects_product_config_dirs() {
        let base = PathBuf::from("/h/Library/Application Support/JetBrains");
        let mut fake = Fake::default();
        fake.dirs.insert(
            base.clone(),
            vec![
                "IntelliJIdea2025.2".into(),
                "Rider2025.1".into(),
                "PyCharmCE2024.3".into(),
                "consentOptions".into(),
                "JetBrainsClient2025.1".into(),
                "WebStorm".into(),
            ],
        );
        let mut found = jetbrains_config_dirs(&base, &fake);
        found.sort();
        assert_eq!(
            found,
            vec![base.join("IntelliJIdea2025.2"), base.join("PyCharmCE2024.3"), base.join("Rider2025.1")]
        );

        assert_eq!(jetbrains_base(Os::Mac, &places(), None), base);
        assert_eq!(jetbrains_base(Os::Linux, &places(), None), PathBuf::from("/h/.config/JetBrains"));
        assert_eq!(
            jetbrains_base(Os::Windows, &places(), Some(Path::new("/ad"))),
            PathBuf::from("/ad/JetBrains")
        );
    }
}
