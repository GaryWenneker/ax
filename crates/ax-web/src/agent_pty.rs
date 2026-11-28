//! Interactive PTY passthrough for external agent CLIs (tubelord3000-style).

use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ax_installer::{build_child_env, detect_cli_available, resolve_cli_spawn};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::WebHub;

#[derive(Deserialize)]
pub struct PtyQuery {
    pub agent: String,
    #[serde(default = "default_profile")]
    pub profile: String,
}

fn default_profile() -> String {
    "default".into()
}

pub async fn handle_pty_ws(
    headers: axum::http::HeaderMap,
    State(hub): State<WebHub>,
    Query(query): Query<PtyQuery>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    if !crate::browser_origin::terminal_request_allowed(&headers, hub.port) {
        return axum::response::Response::builder()
            .status(403)
            .body(axum::body::Body::from("Untrusted origin"))
            .unwrap()
            .into_response();
    }
    if hub.readonly {
        return axum::response::Response::builder()
            .status(403)
            .body(axum::body::Body::from("Read-only mode"))
            .unwrap()
            .into_response();
    }
    ws.on_upgrade(move |socket| handle_pty_socket(socket, hub, query))
}

pub(crate) async fn reject_untrusted_origin(
    State(hub): State<WebHub>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    if !crate::browser_origin::terminal_request_allowed(req.headers(), hub.port) {
        return axum::response::Response::builder()
            .status(403)
            .body(axum::body::Body::from("Untrusted origin"))
            .unwrap();
    }
    next.run(req).await
}

async fn handle_pty_socket(socket: WebSocket, hub: WebHub, query: PtyQuery) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    let ws_guard = hub.read().await;
    let root = ws_guard.project_root.clone();
    drop(ws_guard);

    if query.agent == "builtin" {
        let _ = ws_tx
            .send(Message::Text(
                serde_json::json!({"t":"e","m": ax_usage::format_ax_tagged(
                    "Built-in ax uses chat mode — pick an external agent for the interactive CLI."
                )})
                .to_string()
                .into(),
            ))
            .await;
        return;
    }

    let agent = query.agent.clone();
    let profile = query.profile.clone();
    let spawn_result =
        tokio::task::spawn_blocking(move || spawn_pty(&agent, &profile, &root)).await;

    let pty = match spawn_result {
        Ok(Ok(spawn)) => spawn,
        Ok(Err(e)) => {
            let _ = ws_tx
                .send(Message::Text(
                    serde_json::json!({"t":"e","m": e}).to_string().into(),
                ))
                .await;
            return;
        }
        Err(e) => {
            let _ = ws_tx
                .send(Message::Text(
                    serde_json::json!({"t":"e","m": e.to_string()})
                        .to_string()
                        .into(),
                ))
                .await;
            return;
        }
    };

    let _ = ws_tx
        .send(Message::Text(
            serde_json::json!({"t":"o","d": pty.banner})
                .to_string()
                .into(),
        ))
        .await;

    let (out_tx, mut out_rx) = output_channel();
    let session = PtySession::start(pty.handles, out_tx);

    let forward = tokio::spawn(async move {
        let mut decoder = crate::utf8_stream::Utf8Decoder::default();
        while let Some(chunk) = out_rx.recv().await {
            let text = decoder.decode(&chunk);
            if text.is_empty() {
                continue;
            }
            if ws_tx
                .send(Message::Text(
                    serde_json::json!({"t":"o","d": text}).to_string().into(),
                ))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    while let Some(msg) = ws_rx.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
                    continue;
                };
                match v.get("t").and_then(|t| t.as_str()) {
                    Some("i") => {
                        if let Some(d) = v.get("d").and_then(|d| d.as_str()) {
                            session.input(PtyInput::Data(d.as_bytes().to_vec()));
                        }
                    }
                    Some("r") => {
                        let cols = v.get("cols").and_then(|c| c.as_u64()).unwrap_or(80) as u16;
                        let rows = v.get("rows").and_then(|r| r.as_u64()).unwrap_or(24) as u16;
                        if cols > 0 && rows > 0 {
                            session.input(PtyInput::Resize { rows, cols });
                        }
                    }
                    _ => {}
                }
            }
            Ok(Message::Binary(bytes)) => session.input(PtyInput::Data(bytes.to_vec())),
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }

    drop(session);
    forward.abort();
}

/// Output chunks (of up to 8 KiB) queued for a browser that reads slowly;
/// when they are full the reader waits, and so does the child.
const OUTPUT_CHUNKS: usize = 64;

type OutputSender = mpsc::Sender<Vec<u8>>;

/// The channel that carries PTY output to the WebSocket. It is bounded, so a
/// child that prints faster than the browser reads waits instead of filling
/// memory.
fn output_channel() -> (OutputSender, mpsc::Receiver<Vec<u8>>) {
    mpsc::channel(OUTPUT_CHUNKS)
}

/// Reads PTY output on its own thread until the PTY hangs up or nobody
/// listens any more.
fn spawn_reader(mut reader: Box<dyn Read + Send>, out_tx: OutputSender) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if out_tx.blocking_send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
}

#[cfg(test)]
mod reader_tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    /// A child that prints forever.
    struct Endless(Arc<AtomicUsize>);

    impl Read for Endless {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.0.fetch_add(1, Ordering::SeqCst);
            buf.fill(b'y');
            Ok(buf.len())
        }
    }

    #[tokio::test]
    async fn output_waits_for_a_slow_browser_instead_of_piling_up() {
        let reads = Arc::new(AtomicUsize::new(0));
        let (tx, mut rx) = output_channel();
        spawn_reader(Box::new(Endless(reads.clone())), tx);
        tokio::time::sleep(Duration::from_millis(300)).await;
        let queued = reads.load(Ordering::SeqCst);
        assert!(
            queued <= OUTPUT_CHUNKS + 2,
            "{queued} chunks of 8 KiB read while nobody sent any"
        );
        // Once the browser takes output again, reading goes on.
        for _ in 0..OUTPUT_CHUNKS * 2 {
            assert!(rx.recv().await.is_some());
        }
        drop(rx);
    }
}

enum PtyInput {
    Data(Vec<u8>),
    Resize { rows: u16, cols: u16 },
}

/// A running PTY child with its reader, writer and waiter threads. Writes
/// go through a channel to a writer thread, so a child that stops reading
/// never blocks the async runtime.
struct PtySession {
    input: std::sync::mpsc::Sender<PtyInput>,
    closed: Arc<AtomicBool>,
}

/// Closing the terminal ends the session: the child is killed, the writer
/// thread stops when the input channel closes and drops the PTY master, and
/// the reader thread ends when the PTY hangs up.
impl Drop for PtySession {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::SeqCst);
    }
}

impl PtySession {
    /// Starts the threads that serve `handles`. Output and the exit notice go
    /// to `out_tx`.
    fn start(handles: PtyHandles, out_tx: OutputSender) -> Self {
        let PtyHandles {
            mut writer,
            reader,
            mut child,
            master,
        } = handles;
        let out_tx_exit = out_tx.clone();
        spawn_reader(reader, out_tx);
        // The waiter owns the child. It polls instead of blocking in
        // `wait()` so it can kill the child itself when the terminal closes:
        // killing through the same handle before it is reaped can never hit
        // a reused process id.
        let closed = Arc::new(AtomicBool::new(false));
        let stop = closed.clone();
        std::thread::spawn(move || {
            let status = loop {
                match child.try_wait() {
                    Ok(Some(status)) => break Some(status),
                    Ok(None) if stop.load(Ordering::SeqCst) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break None;
                    }
                    Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                    Err(_) => break None,
                }
            };
            if let Some(status) = status {
                let code = status.exit_code();
                let _ = out_tx_exit.blocking_send(
                    format!(
                        "\r\n\x1b[33m{}\x1b[0m\r\n",
                        ax_usage::format_ax_tagged(format!("Process exited (code {code})"))
                    )
                    .into_bytes(),
                );
            }
        });
        let (input, input_rx) = std::sync::mpsc::channel::<PtyInput>();
        std::thread::spawn(move || {
            while let Ok(item) = input_rx.recv() {
                match item {
                    PtyInput::Data(bytes) => {
                        let _ = writer.write_all(&bytes);
                        let _ = writer.flush();
                    }
                    PtyInput::Resize { rows, cols } => {
                        let _ = master.resize(PtySize {
                            rows,
                            cols,
                            pixel_width: 0,
                            pixel_height: 0,
                        });
                    }
                }
            }
        });
        Self { input, closed }
    }

    fn input(&self, item: PtyInput) {
        let _ = self.input.send(item);
    }
}

struct PtyHandles {
    writer: Box<dyn Write + Send>,
    reader: Box<dyn Read + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    master: Box<dyn MasterPty + Send>,
}

struct PtySpawn {
    handles: PtyHandles,
    banner: String,
}

fn spawn_pty(agent: &str, profile_id: &str, cwd: &Path) -> Result<PtySpawn, String> {
    if !detect_cli_available(agent) {
        return Err(format!(
            "{} CLI not installed — use Settings → AI Agents → Install CLI, then reconnect.",
            catalog_display_name(agent)
        ));
    }

    let spawn = resolve_cli_spawn(agent)?;
    let envs = ax_agent::profiles::ensure_profile_env(agent, profile_id)?;

    let program = spawn
        .program
        .to_str()
        .ok_or_else(|| "CLI path is not valid UTF-8".to_string())?;

    let mut cmd = CommandBuilder::new(program);
    for arg in &spawn.args_prefix {
        cmd.arg(arg);
    }
    cmd.cwd(cwd);

    for (k, v) in build_child_env() {
        cmd.env(k, v);
    }
    for (k, v) in spawn.extra_env {
        cmd.env(k, v);
    }
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");

    let handles = open_pty_command(cmd)?;

    let label = ax_usage::format_ax_tagged(format!(
        "{} — `{}",
        catalog_display_name(agent),
        program.replace('`', "'")
    ));
    let mut banner = format!("\r\n\x1b[36m{label}");
    if !spawn.args_prefix.is_empty() {
        banner.push(' ');
        banner.push_str(&spawn.args_prefix.join(" "));
    }
    banner.push_str("`\x1b[0m\r\n");

    Ok(PtySpawn { handles, banner })
}

/// Opens a PTY and starts `cmd` on it.
fn open_pty_command(cmd: CommandBuilder) -> Result<PtyHandles, String> {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())?;
    let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    let reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    Ok(PtyHandles {
        writer,
        reader,
        child,
        master: pair.master,
    })
}

fn catalog_display_name(agent: &str) -> &str {
    ax_installer::catalog_entry(agent)
        .map(|e| e.display_name)
        .unwrap_or(agent)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn alive(pid: &str) -> bool {
        std::process::Command::new("kill")
            .arg("-0")
            .arg(pid)
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[tokio::test]
    async fn closing_the_terminal_kills_the_child() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("pid");
        let mut cmd = CommandBuilder::new("sh");
        cmd.arg("-c");
        // An idle interactive CLI: it waits for input and prints nothing.
        cmd.arg(format!("echo $$ > '{}'; exec sleep 60", file.display()));
        let handles = open_pty_command(cmd).unwrap();
        let (out_tx, _out_rx) = output_channel();
        let session = PtySession::start(handles, out_tx);
        let mut pid = String::new();
        for _ in 0..100 {
            pid = std::fs::read_to_string(&file)
                .unwrap_or_default()
                .trim()
                .to_string();
            if !pid.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(!pid.is_empty(), "the child never started");
        assert!(alive(&pid));
        drop(session);
        let mut stopped = false;
        for _ in 0..100 {
            if !alive(&pid) {
                stopped = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        if !stopped {
            let _ = std::process::Command::new("kill").arg(&pid).status();
        }
        assert!(
            stopped,
            "the PTY child kept running after the terminal closed"
        );
    }
}
