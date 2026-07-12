//! Interactive PTY passthrough for external agent CLIs (tubelord3000-style).

use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

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
    ws: WebSocketUpgrade,
    State(hub): State<WebHub>,
    Query(query): Query<PtyQuery>,
) -> impl IntoResponse {
    if hub.readonly {
        return axum::response::Response::builder()
            .status(403)
            .body(axum::body::Body::from("Read-only mode"))
            .unwrap()
            .into_response();
    }
    ws.on_upgrade(move |socket| handle_pty_socket(socket, hub, query))
}

async fn handle_pty_socket(socket: WebSocket, hub: WebHub, query: PtyQuery) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    let ws_guard = hub.read().await;
    let root = ws_guard.project_root.clone();
    drop(ws_guard);

    if query.agent == "builtin" {
        let _ = ws_tx
            .send(Message::Text(
                serde_json::json!({"t":"e","m":"Built-in ax uses chat mode — pick an external agent for the interactive CLI."})
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

    let mut pty = match spawn_result {
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
            serde_json::json!({"t":"o","d": pty.banner}).to_string().into(),
        ))
        .await;

    let writer = Arc::new(Mutex::new(pty.handles.writer));
    let master = Arc::new(Mutex::new(pty.handles.master));
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let out_tx_exit = out_tx.clone();

    std::thread::spawn(move || {
        let mut reader = pty.handles.reader;
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if out_tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    std::thread::spawn(move || {
        let status = pty.handles.child.wait();
        let code = status.map(|s| s.exit_code()).unwrap_or(1);
        let _ = out_tx_exit.send(
            format!("\r\n\x1b[33m[ax] Process exited (code {code})\x1b[0m\r\n").into_bytes(),
        );
    });

    let forward = tokio::spawn(async move {
        while let Some(chunk) = out_rx.recv().await {
            let text = String::from_utf8_lossy(&chunk).into_owned();
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
                            if let Ok(mut w) = writer.lock() {
                                let _ = w.write_all(d.as_bytes());
                                let _ = w.flush();
                            }
                        }
                    }
                    Some("r") => {
                        let cols = v.get("cols").and_then(|c| c.as_u64()).unwrap_or(80) as u16;
                        let rows = v.get("rows").and_then(|r| r.as_u64()).unwrap_or(24) as u16;
                        if cols > 0 && rows > 0 {
                            if let Ok(m) = master.lock() {
                                let _ = m.resize(PtySize {
                                    rows,
                                    cols,
                                    pixel_width: 0,
                                    pixel_height: 0,
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Message::Binary(bytes)) => {
                if let Ok(mut w) = writer.lock() {
                    let _ = w.write_all(&bytes);
                    let _ = w.flush();
                }
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }

    forward.abort();
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

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())?;

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

    let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    let reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;

    let mut banner = format!(
        "\r\n\x1b[36m[ax] {} — `{}",
        catalog_display_name(agent),
        program.replace('`', "'")
    );
    if !spawn.args_prefix.is_empty() {
        banner.push(' ');
        banner.push_str(&spawn.args_prefix.join(" "));
    }
    banner.push_str("`\x1b[0m\r\n");

    Ok(PtySpawn {
        handles: PtyHandles {
            writer,
            reader,
            child,
            master: pair.master,
        },
        banner,
    })
}

fn catalog_display_name(agent: &str) -> &str {
    ax_installer::catalog_entry(agent)
        .map(|e| e.display_name)
        .unwrap_or(agent)
}
