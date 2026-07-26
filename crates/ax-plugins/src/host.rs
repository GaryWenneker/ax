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
}

impl PluginHost {
    pub fn load(project_root: &Path) -> Self {
        Self {
            plugins: discover_plugins(project_root),
        }
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
        Some((plugin.manifest.name.clone(), run_plugin(plugin, path, content)))
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
            plugin.manifest.name,
            output.status
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
        let host = PluginHost::load(dir.path());
        assert_eq!(host.plugins.len(), 1);
        assert!(host.plugins[0].matches_ext("demo"));
        assert!(host.plugins[0].matches_ext(".demo"));
    }
}
