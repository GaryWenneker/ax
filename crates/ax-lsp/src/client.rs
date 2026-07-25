//! Minimal stdio JSON-RPC LSP client (definition queries only).

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::servers::ServerSpec;

pub struct LspClient {
    child: Child,
    stdin: ChildStdin,
    reader: BufReader<std::process::ChildStdout>,
    next_id: i64,
    root_uri: String,
}

#[derive(Debug, Clone)]
pub struct LspLocation {
    pub path: PathBuf,
    pub line: u32,
    pub character: u32,
}

impl LspClient {
    pub fn start(spec: &ServerSpec, project_root: &Path) -> Result<Self, String> {
        let mut child = Command::new(spec.command)
            .args(spec.args)
            .current_dir(project_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("spawn {}: {e}", spec.command))?;
        let stdin = child.stdin.take().ok_or("missing stdin")?;
        let stdout = child.stdout.take().ok_or("missing stdout")?;
        let root_uri = path_to_uri(project_root);
        let mut client = Self {
            child,
            stdin,
            reader: BufReader::new(stdout),
            next_id: 1,
            root_uri: root_uri.clone(),
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
                    "definition": { "linkSupport": true }
                }
            },
            "workspaceFolders": [{
                "uri": self.root_uri,
                "name": "ax"
            }]
        });
        let _ = self.request("initialize", params)?;
        self.notify("initialized", json!({}))?;
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
        )
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

    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.request_timeout(method, params, Duration::from_secs(15))
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
            if Instant::now() > deadline {
                return Err(format!("LSP timeout waiting for {method}"));
            }
            let msg = self.read_message()?;
            if msg.get("id").and_then(|v| v.as_i64()) == Some(id) {
                if let Some(err) = msg.get("error") {
                    return Err(format!("LSP error: {err}"));
                }
                return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
            }
            // Drop notifications / other responses.
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
        write!(
            self.stdin,
            "Content-Length: {}\r\n\r\n",
            body.len()
        )
        .map_err(|e| e.to_string())?;
        self.stdin.write_all(&body).map_err(|e| e.to_string())?;
        self.stdin.flush().map_err(|e| e.to_string())
    }

    fn read_message(&mut self) -> Result<Value, String> {
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            self.reader
                .read_line(&mut line)
                .map_err(|e| e.to_string())?;
            if line.is_empty() || line == "\r\n" || line == "\n" {
                break;
            }
            let lower = line.to_ascii_lowercase();
            if let Some(rest) = lower.strip_prefix("content-length:") {
                content_length = rest.trim().parse().ok();
            }
        }
        let len = content_length.ok_or("missing Content-Length")?;
        let mut buf = vec![0u8; len];
        self.reader
            .read_exact(&mut buf)
            .map_err(|e| e.to_string())?;
        serde_json::from_slice(&buf).map_err(|e| e.to_string())
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

pub fn path_to_uri(path: &Path) -> String {
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut s = abs.to_string_lossy().replace('\\', "/");
    if !s.starts_with('/') {
        // Windows drive letter
        s = format!("/{s}");
    }
    format!("file://{s}")
}

pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let path = uri.strip_prefix("file://")?;
    let path = percent_decode(path);
    #[cfg(windows)]
    {
        let p = path.trim_start_matches('/');
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
    let loc = if result.is_array() {
        result.as_array()?.first()?
    } else {
        result
    };
    // Location or LocationLink
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
