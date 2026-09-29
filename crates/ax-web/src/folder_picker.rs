//! `POST /api/vault/folder-picker`: open the operating system's folder dialog and return the path.

use std::sync::atomic::{AtomicBool, Ordering};

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::json;

use crate::workspace_state::WebHub;

/// Test hook: run this program instead of the OS dialog.
const OVERRIDE_ENV: &str = "AX_FOLDER_PICKER_CMD";

const PROMPT: &str = "Choose a folder for the ax vault";

static OPEN: AtomicBool = AtomicBool::new(false);

/// Dialog programs to try on `os`, in order.
fn candidates(os: &str) -> Vec<(String, Vec<String>)> {
    let own = |p: &str, a: &[&str]| (p.to_string(), a.iter().map(|s| s.to_string()).collect());
    match os {
        "macos" => vec![own(
            "osascript",
            &[
                "-e",
                &format!("POSIX path of (choose folder with prompt \"{PROMPT}\")"),
            ],
        )],
        "windows" => vec![own(
            "powershell",
            &[
                "-NoProfile",
                "-STA",
                "-Command",
                "Add-Type -AssemblyName System.Windows.Forms; \
                 $d = New-Object System.Windows.Forms.FolderBrowserDialog; \
                 $d.Description = 'Choose a folder for the ax vault'; \
                 if ($d.ShowDialog() -eq 'OK') { $d.SelectedPath } else { exit 1 }",
            ],
        )],
        _ => vec![
            own(
                "zenity",
                &[
                    "--file-selection",
                    "--directory",
                    &format!("--title={PROMPT}"),
                ],
            ),
            own(
                "kdialog",
                &["--getexistingdirectory", ".", "--title", PROMPT],
            ),
        ],
    }
}

/// The dialog's printed path without the newline and trailing separator; `None` when empty.
fn clean_path(out: &str) -> Option<String> {
    let line = out.trim_end_matches(['\r', '\n']).trim();
    if line.is_empty() {
        return None;
    }
    let is_root = line == "/" || (line.len() == 3 && line.ends_with(":\\"));
    let path = if is_root {
        line
    } else {
        line.trim_end_matches(['/', '\\'])
    };
    Some(path.to_string())
}

fn error(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "error": msg }))).into_response()
}

/// Clears `OPEN` when the request ends, even if the client disconnects mid-dialog.
struct OpenGuard;

impl Drop for OpenGuard {
    fn drop(&mut self) {
        OPEN.store(false, Ordering::SeqCst);
    }
}

/// `Ok(None)` when the user cancelled; `Err(())` when no dialog program could start.
async fn run_dialog() -> Result<Option<String>, ()> {
    let programs = match std::env::var(OVERRIDE_ENV) {
        Ok(cmd) if !cmd.is_empty() => vec![(cmd, Vec::new())],
        _ => candidates(std::env::consts::OS),
    };
    for (program, args) in programs {
        let mut cmd = tokio::process::Command::new(&program);
        cmd.args(&args).kill_on_drop(true);
        let Ok(out) = cmd.output().await else {
            continue;
        };
        if !out.status.success() {
            return Ok(None);
        }
        return Ok(clean_path(&String::from_utf8_lossy(&out.stdout)));
    }
    Err(())
}

async fn pick(State(hub): State<WebHub>, headers: HeaderMap) -> Response {
    if !crate::dav::mount::allowed(&headers, hub.readonly) {
        return error(
            StatusCode::FORBIDDEN,
            "only the local browser on this machine can open the folder dialog",
        );
    }
    if OPEN.swap(true, Ordering::SeqCst) {
        return error(StatusCode::CONFLICT, "a folder dialog is already open");
    }
    let _guard = OpenGuard;
    match run_dialog().await {
        Ok(path) => Json(json!({ "path": path })).into_response(),
        Err(()) => error(
            StatusCode::NOT_IMPLEMENTED,
            "no folder dialog is available on this system; type the path instead",
        ),
    }
}

pub fn router_hub(hub: WebHub) -> Router {
    Router::new().route("/", post(pick)).with_state(hub)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_path_trims_newline_and_trailing_separator() {
        assert_eq!(
            clean_path("/Users/me/notes/\n").as_deref(),
            Some("/Users/me/notes")
        );
        assert_eq!(
            clean_path("C:\\Users\\me\\notes\\\r\n").as_deref(),
            Some("C:\\Users\\me\\notes")
        );
        assert_eq!(clean_path("/\n").as_deref(), Some("/"));
        assert_eq!(clean_path("C:\\\r\n").as_deref(), Some("C:\\"));
        assert_eq!(clean_path("\n"), None);
        assert_eq!(clean_path(""), None);
    }

    #[test]
    fn each_os_gets_its_own_dialog() {
        let first = |os| {
            candidates(os)
                .into_iter()
                .map(|(p, _)| p)
                .collect::<Vec<_>>()
        };
        assert_eq!(first("macos"), ["osascript"]);
        assert_eq!(first("windows"), ["powershell"]);
        assert_eq!(first("linux"), ["zenity", "kdialog"]);
        let (_, mac) = &candidates("macos")[0];
        assert!(mac.iter().any(|a| a.contains("choose folder")), "{mac:?}");
        let (_, win) = &candidates("windows")[0];
        assert!(
            win.iter().any(|a| a.contains("FolderBrowserDialog")),
            "{win:?}"
        );
        assert!(win.contains(&"-STA".to_string()), "{win:?}");
    }
}
