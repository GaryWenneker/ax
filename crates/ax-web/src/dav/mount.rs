//! Connect the WebDAV vault as an OS drive/volume (`/api/dav/mount`).

use std::path::{Path, PathBuf};

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::workspace_state::WebHub;

const DEFAULT_NAME: &str = "ax";
const WINDOWS_LABEL_KEY: &str =
    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\MountPoints2";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    Mac,
    Windows,
    Linux,
}

impl Os {
    pub fn current() -> Os {
        if cfg!(target_os = "macos") {
            Os::Mac
        } else if cfg!(windows) {
            Os::Windows
        } else {
            Os::Linux
        }
    }
}

pub fn valid_name(name: &str) -> bool {
    (1..=32).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '_' | '-'))
}

/// First unused drive letter, searching Z down to D.
pub fn free_letter(used: &[char]) -> Option<char> {
    ('D'..='Z')
        .rev()
        .find(|c| !used.iter().any(|u| u.eq_ignore_ascii_case(c)))
}

pub fn dav_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/ax/")
}

fn gio_url(port: u16) -> String {
    format!("dav://127.0.0.1:{port}/ax/")
}

fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|p| p.to_string()).collect()
}

pub fn mount_commands(
    os: Os,
    port: u16,
    name: &str,
    mount_point: &str,
    autostart: bool,
) -> Vec<Vec<String>> {
    match os {
        Os::Mac => vec![argv(&[
            "/sbin/mount_webdav",
            "-S",
            "-v",
            name,
            &dav_url(port),
            mount_point,
        ])],
        Os::Windows => {
            let persistent = if autostart {
                "/persistent:yes"
            } else {
                "/persistent:no"
            };
            let unc = format!(r"\\127.0.0.1@{port}\ax");
            let key = format!(r"{WINDOWS_LABEL_KEY}\##127.0.0.1@{port}#ax");
            vec![
                argv(&["net", "use", mount_point, &unc, persistent]),
                argv(&[
                    "reg",
                    "add",
                    &key,
                    "/v",
                    "_LabelFromReg",
                    "/t",
                    "REG_SZ",
                    "/d",
                    name,
                    "/f",
                ]),
            ]
        }
        Os::Linux => vec![argv(&["gio", "mount", &gio_url(port)])],
    }
}

pub fn unmount_commands(os: Os, port: u16, mount_point: &str) -> Vec<Vec<String>> {
    match os {
        Os::Mac => vec![argv(&["/sbin/umount", mount_point])],
        Os::Windows => vec![argv(&["net", "use", mount_point, "/delete", "/y"])],
        Os::Linux => vec![argv(&["gio", "mount", "-u", &gio_url(port)])],
    }
}

pub fn open_command(os: Os, mount_point: &str) -> Vec<String> {
    match os {
        Os::Mac => argv(&["open", mount_point]),
        Os::Windows => argv(&["explorer", &format!(r"{mount_point}\")]),
        Os::Linux => argv(&["xdg-open", mount_point]),
    }
}

fn is_loopback_host(host: &str) -> bool {
    let bare = if let Some(rest) = host.strip_prefix('[') {
        match rest.split_once(']') {
            Some((ip, tail)) if tail.is_empty() || tail.starts_with(':') => ip,
            _ => return false,
        }
    } else {
        host.split(':').next().unwrap_or_default()
    };
    matches!(bare, "127.0.0.1" | "localhost" | "::1")
}

/// Only the local user's own browser may mount: no share sessions, LAN clients, or foreign pages.
pub fn allowed(headers: &HeaderMap, readonly: bool) -> bool {
    if readonly {
        return false;
    }
    let header = |k: &str| headers.get(k).and_then(|v| v.to_str().ok());
    let Some(host) = header("host") else {
        return false;
    };
    if !is_loopback_host(host) {
        return false;
    }
    match header("origin") {
        None => true,
        Some(origin) => origin
            .strip_prefix("http://")
            .or_else(|| origin.strip_prefix("https://"))
            .is_some_and(is_loopback_host),
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    name: String,
    mount_point: String,
    autostart: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    os: Os,
    mounted: bool,
    mount_point: Option<String>,
    url: String,
    name: String,
    autostart: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    commands: Option<Vec<Vec<String>>>,
}

#[derive(Deserialize, Default)]
struct MountRequest {
    name: Option<String>,
    #[serde(default)]
    autostart: bool,
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn state_path() -> PathBuf {
    home().join(".ax").join("dav-mount.json")
}

fn load() -> Option<Saved> {
    serde_json::from_str(&std::fs::read_to_string(state_path()).ok()?).ok()
}

fn dry_run() -> bool {
    std::env::var("AX_DAV_MOUNT_DRY_RUN").as_deref() == Ok("1")
}

#[cfg(unix)]
fn is_mount_root(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let (Ok(me), Some(Ok(parent))) = (
        std::fs::metadata(path),
        path.parent().map(std::fs::metadata),
    ) else {
        return false;
    };
    me.dev() != parent.dev()
}

#[cfg(not(unix))]
fn is_mount_root(path: &Path) -> bool {
    path.exists()
}

fn is_mounted(os: Os, mount_point: &str) -> bool {
    match os {
        Os::Mac => is_mount_root(Path::new(mount_point)),
        Os::Windows => Path::new(&format!(r"{mount_point}\")).exists(),
        Os::Linux => Path::new(mount_point).exists(),
    }
}

fn status(port: u16, commands: Option<Vec<Vec<String>>>) -> Status {
    let os = Os::current();
    let saved = load();
    let mount_point = saved.as_ref().map(|s| s.mount_point.clone());
    Status {
        os,
        mounted: mount_point.as_deref().is_some_and(|mp| is_mounted(os, mp)),
        mount_point,
        url: dav_url(port),
        name: saved
            .as_ref()
            .map_or_else(|| DEFAULT_NAME.to_string(), |s| s.name.clone()),
        autostart: saved.is_some_and(|s| s.autostart),
        commands,
    }
}

struct MountError {
    code: StatusCode,
    body: serde_json::Value,
}

impl IntoResponse for MountError {
    fn into_response(self) -> Response {
        (self.code, Json(self.body)).into_response()
    }
}

fn error(code: StatusCode, body: serde_json::Value) -> MountError {
    MountError { code, body }
}

fn run(cmd: &[String]) -> Result<(), MountError> {
    if dry_run() {
        return Ok(());
    }
    let out = std::process::Command::new(&cmd[0])
        .args(&cmd[1..])
        .output()
        .map_err(|e| e.to_string());
    let failure = match out {
        Ok(o) if o.status.success() => return Ok(()),
        Ok(o) => String::from_utf8_lossy(&o.stderr).trim().to_string(),
        Err(e) => e,
    };
    Err(error(
        StatusCode::BAD_GATEWAY,
        json!({ "error": failure, "command": cmd.join(" ") }),
    ))
}

fn gvfs_point(port: u16) -> String {
    let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/run/user/1000".into());
    format!("{runtime}/gvfs/dav:host=127.0.0.1,port={port},ssl=false,prefix=%2Fax")
}

fn windows_used_letters() -> Vec<char> {
    ('D'..='Z')
        .filter(|c| Path::new(&format!(r"{c}:\")).exists())
        .collect()
}

fn webclient_running() -> bool {
    dry_run()
        || std::process::Command::new("sc")
            .args(["query", "WebClient"])
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("RUNNING"))
}

fn mount_point_for(os: Os, port: u16, name: &str) -> Result<String, MountError> {
    match os {
        Os::Mac => {
            let dir = home().join(name);
            std::fs::create_dir_all(&dir).map_err(|e| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    json!({ "error": e.to_string() }),
                )
            })?;
            Ok(dir.to_string_lossy().into_owned())
        }
        Os::Windows => {
            if !webclient_running() {
                return Err(error(
                    StatusCode::CONFLICT,
                    json!({
                        "error": "The Windows WebClient service is not running.",
                        "command": "sc config WebClient start= auto && sc start WebClient",
                    }),
                ));
            }
            free_letter(&windows_used_letters())
                .map(|c| format!("{c}:"))
                .ok_or_else(|| {
                    error(
                        StatusCode::CONFLICT,
                        json!({ "error": "no free drive letter" }),
                    )
                })
        }
        Os::Linux => Ok(gvfs_point(port)),
    }
}

fn autostart_file(os: Os) -> Option<PathBuf> {
    match os {
        Os::Mac => Some(home().join("Library/LaunchAgents/io.getax.dav.plist")),
        Os::Linux => Some(home().join(".config/autostart/ax-dav.desktop")),
        Os::Windows => None,
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn autostart_body(os: Os, cmd: &[String]) -> String {
    match os {
        Os::Mac => {
            let args: String = cmd
                .iter()
                .map(|a| format!("    <string>{}</string>\n", xml_escape(a)))
                .collect();
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                 <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
                 <plist version=\"1.0\">\n<dict>\n  <key>Label</key>\n  <string>io.getax.dav</string>\n\
                 \x20 <key>ProgramArguments</key>\n  <array>\n{args}  </array>\n\
                 \x20 <key>RunAtLoad</key>\n  <true/>\n</dict>\n</plist>\n"
            )
        }
        _ => format!(
            "[Desktop Entry]\nType=Application\nName=ax vault\nExec={}\nX-GNOME-Autostart-enabled=true\n",
            cmd.join(" ")
        ),
    }
}

fn write_file(path: &Path, body: &str) -> Result<(), MountError> {
    let res = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|_| std::fs::write(path, body));
    res.map_err(|e| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({ "error": e.to_string() }),
        )
    })
}

fn linux_bookmark(port: u16, name: &str, add: bool) -> Result<(), MountError> {
    let path = home().join(".config/gtk-3.0/bookmarks");
    let url = gio_url(port);
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<&str> = current
        .lines()
        .filter(|l| l.split(' ').next() != Some(url.as_str()))
        .collect();
    let entry = format!("{url} {name}");
    if add {
        lines.push(&entry);
    }
    write_file(&path, &(lines.join("\n") + "\n"))
}

fn do_mount(port: u16, req: MountRequest) -> Result<Status, MountError> {
    let name = req.name.unwrap_or_else(|| DEFAULT_NAME.to_string());
    if !valid_name(&name) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            json!({ "error": "name must be 1-32 letters, digits, spaces, '_' or '-'" }),
        ));
    }
    let os = Os::current();
    let mount_point = mount_point_for(os, port, &name)?;
    let commands = mount_commands(os, port, &name, &mount_point, req.autostart);
    for cmd in &commands {
        run(cmd)?;
    }
    if os == Os::Linux {
        linux_bookmark(port, &name, true)?;
    }
    if let Some(file) = autostart_file(os) {
        if req.autostart {
            write_file(&file, &autostart_body(os, &commands[0]))?;
        } else {
            let _ = std::fs::remove_file(file);
        }
    }
    let saved = Saved {
        name,
        mount_point,
        autostart: req.autostart,
    };
    write_file(
        &state_path(),
        &serde_json::to_string_pretty(&saved).unwrap_or_default(),
    )?;
    Ok(status(port, Some(commands)))
}

fn do_unmount(port: u16) -> Result<Status, MountError> {
    let os = Os::current();
    let Some(saved) = load() else {
        return Ok(status(port, Some(Vec::new())));
    };
    let commands = unmount_commands(os, port, &saved.mount_point);
    if is_mounted(os, &saved.mount_point) {
        for cmd in &commands {
            run(cmd)?;
        }
    }
    if os == Os::Linux {
        linux_bookmark(port, &saved.name, false)?;
    }
    if let Some(file) = autostart_file(os) {
        let _ = std::fs::remove_file(file);
    }
    let _ = std::fs::remove_file(state_path());
    Ok(status(port, Some(commands)))
}

fn do_open(port: u16) -> Result<Status, MountError> {
    let Some(saved) = load() else {
        return Err(error(
            StatusCode::CONFLICT,
            json!({ "error": "not connected" }),
        ));
    };
    let cmd = open_command(Os::current(), &saved.mount_point);
    run(&cmd)?;
    Ok(status(port, Some(vec![cmd])))
}

async fn blocking(
    hub: &WebHub,
    headers: &HeaderMap,
    f: impl FnOnce(u16) -> Result<Status, MountError> + Send + 'static,
) -> Response {
    if !allowed(headers, hub.readonly) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let port = hub.port;
    match tokio::task::spawn_blocking(move || f(port)).await {
        Ok(Ok(s)) => Json(s).into_response(),
        Ok(Err(e)) => e.into_response(),
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({ "error": e.to_string() }),
        )
        .into_response(),
    }
}

async fn get_status(State(hub): State<WebHub>, headers: HeaderMap) -> Response {
    blocking(&hub, &headers, |port| Ok(status(port, None))).await
}

async fn post_mount(
    State(hub): State<WebHub>,
    headers: HeaderMap,
    body: Option<Json<MountRequest>>,
) -> Response {
    let req = body.map(|Json(r)| r).unwrap_or_default();
    blocking(&hub, &headers, move |port| do_mount(port, req)).await
}

async fn delete_mount(State(hub): State<WebHub>, headers: HeaderMap) -> Response {
    blocking(&hub, &headers, do_unmount).await
}

async fn post_open(State(hub): State<WebHub>, headers: HeaderMap) -> Response {
    blocking(&hub, &headers, do_open).await
}

pub fn router(hub: WebHub) -> Router {
    Router::new()
        .route("/", get(get_status).post(post_mount).delete(delete_mount))
        .route("/open", post(post_open))
        .with_state(hub)
}

#[cfg(test)]
#[path = "mount_tests.rs"]
mod tests;
