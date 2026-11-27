use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

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
    /// Built on the first wasm call and reused for every later file.
    #[cfg(feature = "wasm")]
    wasm: std::sync::OnceLock<Result<crate::wasm_host::WasmRuntime, String>>,
}

impl PluginHost {
    /// Loads the project's plugins that the user has trusted
    /// (`ax plugins trust`). Untrusted plugins are kept aside and never run.
    pub fn load(project_root: &Path) -> Self {
        match crate::trust::trust_store_path() {
            Some(store) => Self::load_with_store(project_root, &store),
            None => Self::new(Vec::new(), discover_plugins(project_root)),
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
        Self::new(plugins, untrusted)
    }

    fn new(plugins: Vec<LoadedPlugin>, untrusted: Vec<LoadedPlugin>) -> Self {
        Self {
            plugins,
            untrusted,
            #[cfg(feature = "wasm")]
            wasm: std::sync::OnceLock::new(),
        }
    }

    #[cfg(feature = "wasm")]
    fn wasm_runtime(&self) -> Result<&crate::wasm_host::WasmRuntime, PluginRunError> {
        self.wasm
            .get_or_init(|| crate::wasm_host::WasmRuntime::new().map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|e| PluginRunError::Wasm(e.clone()))
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
            self.run_plugin(plugin, path, content),
        ))
    }
}

pub fn load_plugins(project_root: &Path) -> PluginHost {
    PluginHost::load(project_root)
}

impl PluginHost {
    fn run_plugin(
        &self,
        plugin: &LoadedPlugin,
        path: &str,
        content: &str,
    ) -> Result<ExtractionResult, PluginRunError> {
        match &plugin.manifest.wasm {
            #[cfg(feature = "wasm")]
            Some(wasm_rel) => self.wasm_runtime()?.run(
                &plugin.dir.join(wasm_rel),
                path,
                content,
                plugin.manifest.timeout(),
            ),
            #[cfg(not(feature = "wasm"))]
            Some(_) => Err(PluginRunError::WasmDisabled),
            None => run_process_plugin(plugin, path, content),
        }
    }
}

fn run_process_plugin(
    plugin: &LoadedPlugin,
    path: &str,
    content: &str,
) -> Result<ExtractionResult, PluginRunError> {
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

    let input = serde_json::json!({ "path": path, "content": content }).to_string();
    let mut child = Command::new(command)
        .args(&plugin.manifest.args)
        .current_dir(&cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| PluginRunError::Io(format!("spawn {command}: {e}")))?;

    // stdin is written and stdout/stderr are read on their own threads, so a
    // plugin that writes before it has read all its input cannot deadlock.
    let stdin = child.stdin.take();
    std::thread::spawn(move || {
        if let Some(mut stdin) = stdin {
            // A plugin may exit without reading everything; that is not an error.
            let _ = stdin.write_all(input.as_bytes());
        }
    });
    let stdout = spawn_reader(child.stdout.take());
    let stderr = spawn_reader(child.stderr.take());

    let name = &plugin.manifest.name;
    let timeout = plugin.manifest.timeout();
    let status = wait_with_timeout(&mut child, timeout)
        .map_err(|e| PluginRunError::Io(e.to_string()))?
        .ok_or_else(|| {
            PluginRunError::Failed(format!(
                "plugin {name} timed out after {}s on {path}; set timeout_secs in plugin.toml to allow longer",
                timeout.as_secs()
            ))
        })?;
    let stdout = collect(stdout, name, "stdout")?;
    if !status.success() {
        let err = collect(stderr, name, "stderr").unwrap_or_default();
        let err = String::from_utf8_lossy(&err);
        return Err(PluginRunError::Failed(format!(
            "plugin {name} exited {status}: {err}"
        )));
    }

    serde_json::from_slice(&stdout).map_err(|e| {
        PluginRunError::Parse(format!("plugin {name} invalid ExtractionResult JSON: {e}"))
    })
}

/// Largest stdout or stderr kept from one plugin run; the rest is drained.
const MAX_PLUGIN_OUTPUT: u64 = 64 * 1024 * 1024;

type Reader = std::sync::mpsc::Receiver<std::io::Result<(Vec<u8>, bool)>>;

/// Reads `pipe` to its end on a thread: `(kept bytes, truncated)`.
fn spawn_reader<R: Read + Send + 'static>(pipe: Option<R>) -> Reader {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = (|| {
            let Some(pipe) = pipe else {
                return Ok((Vec::new(), false));
            };
            let mut limited = pipe.take(MAX_PLUGIN_OUTPUT);
            let mut buf = Vec::new();
            limited.read_to_end(&mut buf)?;
            let truncated = std::io::copy(&mut limited.into_inner(), &mut std::io::sink())? > 0;
            Ok((buf, truncated))
        })();
        let _ = tx.send(result);
    });
    rx
}

/// The output of a reader after the child exited. A grandchild that keeps
/// the pipe open does not block the run for more than a moment.
fn collect(reader: Reader, name: &str, stream: &str) -> Result<Vec<u8>, PluginRunError> {
    match reader.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok((buf, false))) => Ok(buf),
        Ok(Ok((_, true))) => Err(PluginRunError::Failed(format!(
            "plugin {name} wrote more than {} MiB to {stream}",
            MAX_PLUGIN_OUTPUT / (1024 * 1024)
        ))),
        Ok(Err(e)) => Err(PluginRunError::Io(format!("plugin {name} {stream}: {e}"))),
        Err(_) => Err(PluginRunError::Failed(format!(
            "plugin {name} exited but its {stream} stayed open"
        ))),
    }
}

/// Waits for `child` up to `timeout`; on expiry kills it and returns `None`.
fn wait_with_timeout(
    child: &mut Child,
    timeout: Duration,
) -> std::io::Result<Option<std::process::ExitStatus>> {
    let deadline = Instant::now() + timeout;
    let mut pause = Duration::from_millis(1);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        let now = Instant::now();
        if now >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        std::thread::sleep(pause.min(deadline - now));
        pause = (pause * 2).min(Duration::from_millis(50));
    }
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

    /// A trusted host with one process plugin for `.rs` running `sh -c script`.
    #[cfg(unix)]
    fn sh_plugin_host(dir: &std::path::Path, script: &str, extra: &str) -> PluginHost {
        let plug = dir.join(".ax/plugins/sh");
        fs::create_dir_all(&plug).unwrap();
        let manifest = format!(
            "name = \"sh\"\nextensions = [\".rs\"]\ncommand = \"sh\"\nargs = [\"-c\", {}]\n{extra}",
            toml::Value::String(script.to_string())
        );
        fs::write(plug.join("plugin.toml"), manifest).unwrap();
        let store = dir.join("trust.json");
        for p in discover_plugins(dir) {
            crate::trust::trust_in(&store, &p).unwrap();
        }
        PluginHost::load_with_store(dir, &store)
    }

    /// Runs `extract` on a thread so a deadlock fails the test instead of hanging it.
    #[cfg(unix)]
    fn extract_within(
        host: PluginHost,
        content: String,
        limit: std::time::Duration,
    ) -> Result<ExtractionResult, PluginRunError> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (_, res) = host.extract("a.rs", &content).unwrap();
            let _ = tx.send(res);
        });
        rx.recv_timeout(limit)
            .unwrap_or_else(|_| panic!("plugin run did not finish within {limit:?}"))
    }

    #[cfg(unix)]
    #[test]
    fn a_plugin_that_writes_before_reading_its_input_does_not_deadlock() {
        let dir = tempfile::tempdir().unwrap();
        // 200 KB on stderr and stdout-sized echo, before stdin is read.
        let host = sh_plugin_host(
            dir.path(),
            "head -c 200000 /dev/zero | tr '\\0' x >&2; cat >/dev/null; echo '{\"nodes\":[],\"edges\":[],\"unresolvedReferences\":[],\"errors\":[],\"durationMs\":0}'",
            "",
        );
        let content = "x".repeat(300_000);
        let res = extract_within(host, content, std::time::Duration::from_secs(30));
        assert!(res.is_ok(), "{:?}", res.err());
    }

    #[cfg(unix)]
    #[test]
    fn a_hung_plugin_is_stopped_after_its_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let host = sh_plugin_host(dir.path(), "sleep 60", "timeout_secs = 1\n");
        let started = std::time::Instant::now();
        let res = extract_within(host, String::new(), std::time::Duration::from_secs(20));
        let err = res.expect_err("hung plugin returned a result");
        assert!(err.to_string().contains("timed out"), "{err}");
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
    }

    /// A trusted host with one wasm plugin for `.rs` built from `wat`.
    #[cfg(feature = "wasm")]
    fn wat_plugin_host(dir: &std::path::Path, wat: &str) -> PluginHost {
        let plug = dir.join(".ax/plugins/w");
        fs::create_dir_all(&plug).unwrap();
        fs::write(plug.join("extractor.wat"), wat).unwrap();
        fs::write(
            plug.join("plugin.toml"),
            "name = \"w\"\nextensions = [\".rs\"]\nwasm = \"extractor.wat\"\ntimeout_secs = 1\n",
        )
        .unwrap();
        let store = dir.join("trust.json");
        for p in discover_plugins(dir) {
            crate::trust::trust_in(&store, &p).unwrap();
        }
        PluginHost::load_with_store(dir, &store)
    }

    #[cfg(feature = "wasm")]
    fn guest(extract_body: &str) -> String {
        format!(
            r#"(module
  (memory (export "memory") 1)
  (func (export "alloc") (param i32) (result i32) i32.const 1024)
  (func (export "extract") (param i32 i32) (result i64) {extract_body}))"#
        )
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn a_looping_wasm_plugin_is_interrupted_after_its_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let host = wat_plugin_host(dir.path(), &guest("(loop $l br $l) i64.const 0"));
        let started = std::time::Instant::now();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(host.extract("a.rs", "").unwrap().1);
        });
        let res = rx
            .recv_timeout(std::time::Duration::from_secs(20))
            .expect("wasm run did not stop");
        let err = res.expect_err("looping guest returned a result");
        assert!(err.to_string().contains("timed out"), "{err}");
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn a_wasm_plugin_claiming_a_huge_output_is_refused_before_allocating() {
        let dir = tempfile::tempdir().unwrap();
        // out_ptr = 0, out_len = 0xFFFF_FFFF
        let host = wat_plugin_host(dir.path(), &guest("i64.const 4294967295"));
        let err = host.extract("a.rs", "").unwrap().1.expect_err("accepted");
        assert!(err.to_string().contains("too large"), "{err}");
    }

    #[cfg(feature = "wasm")]
    #[test]
    fn a_wasm_module_is_compiled_once_per_host() {
        let dir = tempfile::tempdir().unwrap();
        let host = wat_plugin_host(dir.path(), &guest("i64.const 0"));
        for _ in 0..3 {
            // Empty output is not valid JSON; only the compile count matters here.
            let _ = host.extract("a.rs", "").unwrap().1;
        }
        assert_eq!(host.wasm_runtime().unwrap().cached_modules(), 1);
    }
}
