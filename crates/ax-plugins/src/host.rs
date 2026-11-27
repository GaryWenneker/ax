use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use ax_types::ExtractionResult;

use crate::manifest::{discover_plugins, LoadedPlugin};

#[derive(Debug)]
pub enum PluginRunError {
    Io(String),
    Failed(String),
    Parse(String),
    #[cfg(feature = "wasm")]
    Wasm(String),
    #[cfg(not(feature = "wasm"))]
    WasmDisabled,
}

impl std::fmt::Display for PluginRunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) | Self::Failed(e) | Self::Parse(e) => write!(f, "{e}"),
            #[cfg(feature = "wasm")]
            Self::Wasm(e) => write!(f, "wasm: {e}"),
            #[cfg(not(feature = "wasm"))]
            Self::WasmDisabled => write!(
                f,
                "wasm plugins require building ax with --features plugins-wasm"
            ),
        }
    }
}

pub struct PluginHost {
    plugins: Vec<LoadedPlugin>,
    untrusted: Vec<LoadedPlugin>,
}

impl PluginHost {
    /// Loads the project's plugins that the user has trusted
    /// (`ax plugins trust`). Untrusted plugins are kept aside and never run.
    pub fn load(project_root: &Path) -> Self {
        match crate::trust::trust_store_path() {
            Some(store) => Self::load_with_store(project_root, &store),
            None => Self {
                plugins: Vec::new(),
                untrusted: discover_plugins(project_root),
            },
        }
    }

    /// [`PluginHost::load`] against an explicit trust store file.
    pub fn load_with_store(project_root: &Path, store: &Path) -> Self {
        let (plugins, untrusted): (Vec<_>, Vec<_>) = discover_plugins(project_root)
            .into_iter()
            .partition(|p| crate::trust::is_trusted_in(store, p));
        for p in &untrusted {
            tracing::warn!(
                plugin = %p.manifest.name,
                dir = %p.dir.display(),
                "skipping untrusted project plugin; run `ax plugins trust` to allow it"
            );
        }
        Self { plugins, untrusted }
    }

    /// Plugins found in the project but not trusted, so not run.
    pub fn untrusted(&self) -> &[LoadedPlugin] {
        &self.untrusted
    }

    pub fn manifests(&self) -> Vec<&crate::manifest::PluginManifest> {
        self.plugins.iter().map(|p| &p.manifest).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    pub fn extensions(&self) -> Vec<String> {
        let mut exts = Vec::new();
        for p in &self.plugins {
            for e in &p.manifest.extensions {
                if !exts.iter().any(|x: &String| x.eq_ignore_ascii_case(e)) {
                    exts.push(e.clone());
                }
            }
        }
        exts
    }

    /// Extract via the first matching plugin for `path`'s extension.
    /// Returns `(plugin_name, result)`.
    pub fn extract(
        &self,
        path: &str,
        content: &str,
    ) -> Option<(String, Result<ExtractionResult, PluginRunError>)> {
        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let plugin = self.plugins.iter().find(|p| p.matches_ext(ext))?;
        Some((
            plugin.manifest.name.clone(),
            run_plugin(plugin, path, content),
        ))
    }
}

pub fn load_plugins(project_root: &Path) -> PluginHost {
    PluginHost::load(project_root)
}

fn run_plugin(
    plugin: &LoadedPlugin,
    path: &str,
    content: &str,
) -> Result<ExtractionResult, PluginRunError> {
    if let Some(wasm_rel) = &plugin.manifest.wasm {
        #[cfg(feature = "wasm")]
        {
            let wasm_path = plugin.dir.join(wasm_rel);
            return crate::wasm_host::run_wasm(&wasm_path, path, content);
        }
        #[cfg(not(feature = "wasm"))]
        {
            let _ = wasm_rel;
            return Err(PluginRunError::WasmDisabled);
        }
    }

    let command = plugin
        .manifest
        .command
        .as_deref()
        .ok_or_else(|| PluginRunError::Failed("plugin.toml needs command= or wasm=".into()))?;

    let cwd = plugin
        .manifest
        .cwd
        .as_ref()
        .map(|c| plugin.dir.join(c))
        .unwrap_or_else(|| plugin.dir.clone());

    let input = serde_json::json!({ "path": path, "content": content });
    let mut child = Command::new(command)
        .args(&plugin.manifest.args)
        .current_dir(&cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| PluginRunError::Io(format!("spawn {command}: {e}")))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(input.to_string().as_bytes())
            .map_err(|e| PluginRunError::Io(e.to_string()))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| PluginRunError::Io(e.to_string()))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(PluginRunError::Failed(format!(
            "plugin {} exited {}: {err}",
            plugin.manifest.name, output.status
        )));
    }

    serde_json::from_slice(&output.stdout).map_err(|e| {
        PluginRunError::Parse(format!(
            "plugin {} invalid ExtractionResult JSON: {e}",
            plugin.manifest.name
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn discovers_process_plugin_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let plug = dir.path().join(".ax/plugins/demo");
        fs::create_dir_all(&plug).unwrap();
        fs::write(
            plug.join("plugin.toml"),
            r#"
name = "demo"
extensions = [".demo"]
command = "echo"
args = []
"#,
        )
        .unwrap();
        let found = discover_plugins(dir.path());
        assert_eq!(found.len(), 1);
        assert!(found[0].matches_ext("demo"));
        assert!(found[0].matches_ext(".demo"));
        // Found, but not loaded for running until trusted.
        let store = dir.path().join("trust.json");
        let host = PluginHost::load_with_store(dir.path(), &store);
        assert!(host.is_empty());
        assert_eq!(host.untrusted().len(), 1);
    }

    #[cfg(unix)]
    fn marker_plugin(project: &std::path::Path, marker: &std::path::Path) {
        let plug = project.join(".ax/plugins/evil");
        fs::create_dir_all(&plug).unwrap();
        fs::write(
            plug.join("plugin.toml"),
            format!(
                "name = \"evil\"\nextensions = [\".rs\"]\ncommand = \"sh\"\nargs = [\"-c\", \"touch '{}'; echo '{{}}'\"]\n",
                marker.display()
            ),
        )
        .unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn untrusted_project_plugin_never_runs() {
        let dir = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let marker = dir.path().join("pwned");
        marker_plugin(dir.path(), &marker);
        std::env::set_var("AX_HOME_DIR", home.path());
        let host = PluginHost::load(dir.path());
        let ran = host.extract("src/main.rs", "fn main() {}");
        assert!(ran.is_none(), "untrusted plugin was offered for .rs");
        assert!(!marker.exists(), "untrusted plugin command ran");
        assert!(host.extensions().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn trusted_plugin_runs_until_its_manifest_changes() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("ran");
        marker_plugin(dir.path(), &marker);
        let store = dir.path().join("home/.ax/plugin-trust.json");

        for p in discover_plugins(dir.path()) {
            crate::trust::trust_in(&store, &p).unwrap();
        }
        let host = PluginHost::load_with_store(dir.path(), &store);
        assert_eq!(host.extensions(), vec![".rs".to_string()]);
        let (name, _res) = host.extract("a.rs", "").unwrap();
        assert_eq!(name, "evil");
        assert!(marker.exists(), "trusted plugin did not run");

        // Editing plugin.toml (here: a new command) drops the trust.
        fs::remove_file(&marker).unwrap();
        let manifest = dir.path().join(".ax/plugins/evil/plugin.toml");
        let raw = fs::read_to_string(&manifest).unwrap();
        fs::write(&manifest, raw.replace("touch", "touch -c")).unwrap();
        let host = PluginHost::load_with_store(dir.path(), &store);
        assert!(host.extract("a.rs", "").is_none());
        assert!(!marker.exists());

        // Untrust removes it again.
        for p in discover_plugins(dir.path()) {
            crate::trust::trust_in(&store, &p).unwrap();
        }
        assert!(!PluginHost::load_with_store(dir.path(), &store).is_empty());
        for p in discover_plugins(dir.path()) {
            crate::trust::untrust_in(&store, &p).unwrap();
        }
        assert!(PluginHost::load_with_store(dir.path(), &store).is_empty());
    }

    #[test]
    fn unparsable_trust_store_is_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let plug = dir.path().join(".ax/plugins/demo");
        fs::create_dir_all(&plug).unwrap();
        fs::write(
            plug.join("plugin.toml"),
            "name = \"demo\"\ncommand = \"x\"\n",
        )
        .unwrap();
        let store = dir.path().join("trust.json");
        fs::write(&store, "{ broken").unwrap();
        let p = &discover_plugins(dir.path())[0];
        assert!(crate::trust::trust_in(&store, p).is_err());
        assert_eq!(fs::read_to_string(&store).unwrap(), "{ broken");
        assert!(PluginHost::load_with_store(dir.path(), &store).is_empty());
    }
}
