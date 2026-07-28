//! Minimal stdio JSON-RPC LSP client (definition queries only).

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::servers::ServerSpec;

pub struct LspClient {
    child: Child,
    stdin: ChildStdin,
    incoming: Receiver<Result<Value, String>>,
    /// Keep sender alive so the reader thread can detect disconnect on drop.
    _reader_tx: Sender<Result<Value, String>>,
    next_id: i64,
    root_uri: String,
    /// Set from `rust-analyzer/serverStatus` (and treated true for other servers
    /// after the initial warmup window).
    quiescent: bool,
    server_id: String,
}

#[derive(Debug, Clone)]
pub struct LspLocation {
    pub path: PathBuf,
    pub line: u32,
    pub character: u32,
}

impl LspClient {
    pub fn start(spec: &ServerSpec, project_root: &Path) -> Result<Self, String> {
        let project_root = strip_verbatim_prefix(
            &project_root
                .canonicalize()
                .unwrap_or_else(|_| project_root.to_path_buf()),
        );
        let mut cmd = Command::new(spec.command);
        cmd.args(spec.args)
            .current_dir(&project_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if std::env::var_os("RUSTUP_TOOLCHAIN").is_none() {
            cmd.env("RUSTUP_TOOLCHAIN", "stable");
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("spawn {}: {e}", spec.command))?;
        let stdin = child.stdin.take().ok_or("missing stdin")?;
        let stdout = child.stdout.take().ok_or("missing stdout")?;
        let stderr = child.stderr.take().ok_or("missing stderr")?;
        // Drain stderr so a full pipe cannot stall the language server.
        thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                let trimmed = line.trim_end();
                if !trimmed.is_empty() {
                    tracing::debug!(target: "ax_lsp", "lsp-stderr: {trimmed}");
                }
                line.clear();
            }
        });
        let (tx, rx) = mpsc::channel::<Result<Value, String>>();
        let reader_tx = tx.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                match read_message_from(&mut reader) {
                    Ok(msg) => {
                        if reader_tx.send(Ok(msg)).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = reader_tx.send(Err(e));
                        break;
                    }
                }
            }
        });
        let root_uri = path_to_uri(&project_root);
        let mut client = Self {
            child,
            stdin,
            incoming: rx,
            _reader_tx: tx,
            next_id: 1,
            root_uri: root_uri.clone(),
            // Non-RA servers never send serverStatus; treat them ready after warmup.
            quiescent: spec.id != "rust-analyzer",
            server_id: spec.id.to_string(),
        };
        client.initialize()?;
        Ok(client)
    }

    fn initialize(&mut self) -> Result<(), String> {
        let params = json!({
            "processId": std::process::id(),
            "rootUri": self.root_uri,
            "capabilities": {
                "textDocument": {
                    "definition": { "linkSupport": true },
                    "synchronization": { "didSave": false }
                },
                "workspace": {
                    "workspaceFolders": true,
                    "configuration": true,
                    "didChangeConfiguration": { "dynamicRegistration": false }
                },
                "window": {
                    "workDoneProgress": true
                },
                "experimental": {
                    "serverStatusNotification": true
                }
            },
            "initializationOptions": {
                "cargo": {
                    "buildScripts": { "enable": true }
                }
            },
            "workspaceFolders": [{
                "uri": self.root_uri,
                "name": "root"
            }]
        });
        let _ = self.request("initialize", params)?;
        self.notify("initialized", json!({}))?;
        self.pump(Duration::from_millis(500))?;
        Ok(())
    }

    pub fn did_open(&mut self, path: &Path, language_id: &str, text: &str) -> Result<(), String> {
        let uri = path_to_uri(path);
        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": 1,
                    "text": text
                }
            }),
        )?;
        Ok(())
    }

    pub fn definition(
        &mut self,
        path: &Path,
        line: u32,
        character: u32,
        timeout: Duration,
    ) -> Result<Option<LspLocation>, String> {
        let uri = path_to_uri(path);
        let result = self.request_timeout(
            "textDocument/definition",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }),
            timeout,
        )?;
        Ok(parse_definition_result(&result))
    }

    /// Definition with retries while the server is still indexing.
    pub fn definition_ready(
        &mut self,
        path: &Path,
        line: u32,
        character: u32,
    ) -> Result<Option<LspLocation>, String> {
        const ATTEMPTS: u32 = 6;
        for attempt in 0..ATTEMPTS {
            if !self.quiescent {
                let _ = self.wait_ready(Duration::from_secs(15));
            }
            match self.definition(path, line, character, Duration::from_secs(45))? {
                Some(loc) => return Ok(Some(loc)),
                None if attempt + 1 < ATTEMPTS && !self.quiescent => {
                    let _ = self.wait_ready(Duration::from_secs(5));
                    continue;
                }
                None => return Ok(None),
            }
        }
        Ok(None)
    }

    /// Drain/answer server messages until quiescent (RA) or the budget expires.
    pub fn wait_ready(&mut self, total: Duration) -> Result<(), String> {
        let deadline = Instant::now() + total;
        while Instant::now() < deadline {
            if self.quiescent && self.server_id == "rust-analyzer" {
                // Keep draining briefly so pending requests are answered.
                self.pump(Duration::from_millis(200))?;
                return Ok(());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            self.pump(remaining.min(Duration::from_millis(500)))?;
        }
        // Non-RA (or RA that never sent status): proceed after warmup budget.
        if self.server_id != "rust-analyzer" {
            self.quiescent = true;
        }
        Ok(())
    }

    pub fn is_quiescent(&self) -> bool {
        self.quiescent
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.request_timeout(method, params, Duration::from_secs(60))
    }

    fn request_timeout(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });
        self.write_message(&msg)?;
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(format!("LSP timeout waiting for {method}"));
            }
            let msg = self.recv_message(remaining)?;
            self.observe_notification(&msg);
            if self.maybe_answer_server_request(&msg)? {
                continue;
            }
            if message_id_matches(&msg, id) {
                if let Some(err) = msg.get("error") {
                    return Err(format!("LSP error: {err}"));
                }
                return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
            }
        }
    }

    fn observe_notification(&mut self, msg: &Value) {
        let Some(method) = msg.get("method").and_then(|m| m.as_str()) else {
            return;
        };
        if method == "rust-analyzer/serverStatus" || method == "experimental/serverStatus" {
            if let Some(q) = msg.pointer("/params/quiescent").and_then(|v| v.as_bool()) {
                self.quiescent = q;
                tracing::debug!(target: "ax_lsp", quiescent = q, "serverStatus");
            }
        }
    }

    /// Answer server→client requests; return true when handled.
    fn maybe_answer_server_request(&mut self, msg: &Value) -> Result<bool, String> {
        let Some(method) = msg.get("method").and_then(|m| m.as_str()) else {
            return Ok(false);
        };
        let Some(id) = msg.get("id") else {
            return Ok(false); // notification
        };
        let result = match method {
            "window/workDoneProgress/create"
            | "client/registerCapability"
            | "client/unregisterCapability" => Value::Null,
            "workspace/configuration" => {
                let n = msg
                    .pointer("/params/items")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(1);
                Value::Array(vec![json!({}); n])
            }
            "workspace/workspaceFolders" => json!([{
                "uri": self.root_uri,
                "name": "root"
            }]),
            _ => Value::Null,
        };
        self.write_message(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result
        }))?;
        Ok(true)
    }

    fn pump(&mut self, budget: Duration) -> Result<(), String> {
        let deadline = Instant::now() + budget;
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match self.incoming.recv_timeout(remaining.min(Duration::from_millis(100))) {
                Ok(Ok(msg)) => {
                    self.observe_notification(&msg);
                    let _ = self.maybe_answer_server_request(&msg)?;
                }
                Ok(Err(e)) => return Err(e),
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    return Err("language server reader exited".into());
                }
            }
        }
        Ok(())
    }

    fn recv_message(&mut self, timeout: Duration) -> Result<Value, String> {
        match self.incoming.recv_timeout(timeout) {
            Ok(Ok(msg)) => Ok(msg),
            Ok(Err(e)) => Err(e),
            Err(RecvTimeoutError::Timeout) => Err("LSP read timeout".into()),
            Err(RecvTimeoutError::Disconnected) => Err("language server reader exited".into()),
        }
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        let msg = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });
        self.write_message(&msg)
    }

    fn write_message(&mut self, value: &Value) -> Result<(), String> {
        let body = serde_json::to_vec(value).map_err(|e| e.to_string())?;
        write!(self.stdin, "Content-Length: {}\r\n\r\n", body.len()).map_err(|e| e.to_string())?;
        self.stdin.write_all(&body).map_err(|e| e.to_string())?;
        self.stdin.flush().map_err(|e| e.to_string())
    }

    pub fn shutdown(mut self) {
        let _ = self.notify("shutdown", Value::Null);
        let _ = self.child.kill();
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn message_id_matches(msg: &Value, id: i64) -> bool {
    msg.get("id").and_then(|v| v.as_i64()) == Some(id)
        || msg.get("id").and_then(|v| v.as_u64()) == Some(id as u64)
}

fn read_message_from(reader: &mut impl BufRead) -> Result<Value, String> {
    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;
        if line.is_empty() {
            return Err("language server closed stdout".into());
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("content-length:") {
            content_length = rest.trim().parse().ok();
        }
    }
    let len = content_length.ok_or("missing Content-Length")?;
    let len = content_length.ok_or("missing Content-Length")?;
    let mut buf = vec![0u8; len];
    std::io::Read::read_exact(reader, &mut buf).map_err(|e| e.to_string())?;
    serde_json::from_slice(&buf).map_err(|e| e.to_string())
}

pub fn path_to_uri(path: &Path) -> String {
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let abs = strip_verbatim_prefix(&abs);
    let mut s = abs.to_string_lossy().replace('\\', "/");
    #[cfg(windows)]
    {
        if let Some(rest) = s.strip_prefix("//?/") {
            s = rest.to_string();
        }
        if !s.starts_with('/') {
            s = format!("/{s}");
        }
    }
    #[cfg(not(windows))]
    {
        if !s.starts_with('/') {
            s = format!("/{s}");
        }
    }
    format!("file://{s}")
}

/// Strip Windows `\\?\` / `\\?\UNC\` extended-length prefixes from canonicalize.
pub fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        if let Some(unc) = rest.strip_prefix(r"UNC\") {
            return PathBuf::from(format!(r"\\{unc}"));
        }
        return PathBuf::from(rest);
    }
    if let Some(rest) = s.strip_prefix("//?/") {
        if let Some(unc) = rest.strip_prefix("UNC/") {
            return PathBuf::from(format!("//{unc}"));
        }
        return PathBuf::from(rest);
    }
    path.to_path_buf()
}

pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let path = uri.strip_prefix("file://")?;
    let path = percent_decode(path);
    #[cfg(windows)]
    {
        let p = path.trim_start_matches('/');
        let p = p.strip_prefix("//?/").unwrap_or(p);
        return Some(PathBuf::from(p.replace('/', "\\")));
    }
    #[cfg(not(windows))]
    {
        Some(PathBuf::from(path))
    }
}

fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
            {
                out.push(b as char);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn parse_definition_result(result: &Value) -> Option<LspLocation> {
    if result.is_null() {
        return None;
    }
    let loc = if result.is_array() {
        result.as_array()?.first()?
    } else {
        result
    };
    let (uri, range) = if let Some(target_uri) = loc.get("targetUri") {
        (
            target_uri.as_str()?,
            loc.get("targetSelectionRange")
                .or_else(|| loc.get("targetRange"))?,
        )
    } else {
        (loc.get("uri")?.as_str()?, loc.get("range")?)
    };
    let line = range.pointer("/start/line")?.as_u64()? as u32;
    let character = range.pointer("/start/character")?.as_u64()? as u32;
    Some(LspLocation {
        path: uri_to_path(uri)?,
        line,
        character,
    })
}

pub fn language_id_for_ext(ext: &str) -> &'static str {
    match ext.trim_start_matches('.').to_ascii_lowercase().as_str() {
        "rs" => "rust",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "typescriptreact",
        "js" => "javascript",
        "jsx" => "javascriptreact",
        "py" | "pyi" => "python",
        "go" => "go",
        _ => "plaintext",
    }
}

/// Prefer the column of `name` on the given 1-based line when the stored column
/// points at a call receiver (e.g. `session_id.into()` stored at `session_id`).
pub fn column_for_name(content: &str, line_1based: i32, column: i32, name: &str) -> u32 {
    let line = content
        .lines()
        .nth(line_1based.max(1) as usize - 1)
        .unwrap_or("");
    let start = (column.max(0) as usize).min(line.len());
    if !name.is_empty() {
        if let Some(rel) = line[start..].find(name) {
            return (start + rel) as u32;
        }
        if let Some(abs) = line.find(name) {
            return abs as u32;
        }
    }
    start as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Manual probe: `cargo test -p ax-lsp probe_ra_definition -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn probe_ra_definition() {
        use crate::servers::SERVERS;

        let root = PathBuf::from(r"C:\gary\ax");
        if !root.join("Cargo.toml").exists() {
            eprintln!("skip: not at C:\\gary\\ax");
            return;
        }
        let spec = SERVERS.iter().find(|s| s.id == "rust-analyzer").unwrap();
        let path = root.join("crates/ax-agent/src/config.rs");
        let text = std::fs::read_to_string(&path).unwrap();
        eprintln!("root_uri={}", path_to_uri(&root));
        eprintln!("file_uri={}", path_to_uri(&path));

        let line_idx = 73u32;
        let col = column_for_name(&text, 74, 0, "config_path");
        eprintln!("query line={} col={} name=config_path", line_idx + 1, col);

        let mut client = LspClient::start(spec, &root).expect("start ra");
        client.did_open(&path, "rust", &text).expect("didOpen");
        eprintln!("waiting for quiescent…");
        client
            .wait_ready(Duration::from_secs(180))
            .expect("wait_ready");
        eprintln!("quiescent={}", client.is_quiescent());

        match client.definition_ready(&path, line_idx, col) {
            Ok(Some(loc)) => {
                eprintln!("RESOLVED -> {}:{}", loc.path.display(), loc.line + 1);
            }
            Ok(None) => panic!("definition returned None (quiescent={})", client.is_quiescent()),
            Err(e) => panic!("definition error: {e}"),
        }
    }

    #[test]
    fn parses_location() {
        let v = json!({
            "uri": "file:///C:/proj/src/lib.rs",
            "range": { "start": { "line": 10, "character": 4 }, "end": { "line": 10, "character": 8 } }
        });
        let loc = parse_definition_result(&v).unwrap();
        assert_eq!(loc.line, 10);
        assert!(loc.path.to_string_lossy().contains("lib.rs"));
    }

    #[test]
    fn strips_windows_verbatim_prefix() {
        let p = PathBuf::from(r"\\?\C:\gary\ax\src\lib.rs");
        let stripped = strip_verbatim_prefix(&p);
        assert_eq!(stripped, PathBuf::from(r"C:\gary\ax\src\lib.rs"));
        let uri = {
            let mut s = stripped.to_string_lossy().replace('\\', "/");
            if !s.starts_with('/') {
                s = format!("/{s}");
            }
            format!("file://{s}")
        };
        assert_eq!(uri, "file:///C:/gary/ax/src/lib.rs");
        assert!(!uri.contains('?'));
    }

    #[test]
    fn strips_unc_verbatim_prefix() {
        let p = PathBuf::from(r"\\?\UNC\server\share\file.rs");
        let stripped = strip_verbatim_prefix(&p);
        assert_eq!(stripped, PathBuf::from(r"\\server\share\file.rs"));
    }

    #[test]
    fn column_for_name_finds_method() {
        let src = "            session_id: session_id.into(),\n";
        assert_eq!(column_for_name(src, 1, 24, "into"), 35);
    }
}
