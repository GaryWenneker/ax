//! The MCP daemon stops when its binary is replaced, and a proxy survives a daemon restart.

#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

struct Project {
    _dir: tempfile::TempDir,
    home: tempfile::TempDir,
    root: PathBuf,
}

fn ax(exe: &Path, home: &Path) -> Command {
    let mut cmd = Command::new(exe);
    cmd.env("HOME", home)
        .env("USERPROFILE", home)
        .env("AX_NO_UPDATE_CHECK", "1")
        .env("NO_COLOR", "1")
        .env_remove("AX_DAEMON_EXE_CHECK_MS")
        .env_remove("AX_DAEMON_IDLE_MS");
    cmd
}

fn project() -> Project {
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    std::fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
    let out = ax(Path::new(env!("CARGO_BIN_EXE_ax")), home.path())
        .arg("init")
        .current_dir(&root)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "ax init: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Project {
        _dir: dir,
        home,
        root,
    }
}

fn daemon_info(root: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(root.join(".ax/daemon.json")).ok()?;
    serde_json::from_str(&text).ok()
}

fn wait_until<T>(what: &str, timeout: Duration, mut probe: impl FnMut() -> Option<T>) -> T {
    let end = Instant::now() + timeout;
    loop {
        if let Some(found) = probe() {
            return found;
        }
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn kill(pid: u64) {
    let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status();
}

struct KillOnDrop(Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn e5_the_daemon_stops_when_its_binary_is_replaced() {
    let p = project();
    let bin_dir = tempfile::tempdir().unwrap();
    let exe = bin_dir.path().join("ax");
    std::fs::copy(env!("CARGO_BIN_EXE_ax"), &exe).unwrap();

    let mut daemon = KillOnDrop(
        ax(&exe, p.home.path())
            .args(["serve", "--mcp", "--daemon", "--path"])
            .arg(&p.root)
            .env("AX_DAEMON_EXE_CHECK_MS", "200")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let info = wait_until("daemon.json", Duration::from_secs(20), || {
        daemon_info(&p.root)
    });
    assert_eq!(
        info["exe"]["path"],
        exe.to_string_lossy().as_ref(),
        "identity in daemon.json"
    );
    std::thread::sleep(Duration::from_millis(600));
    assert!(
        daemon.0.try_wait().unwrap().is_none(),
        "an untouched binary keeps the daemon up"
    );

    let staged = bin_dir.path().join("ax.new");
    std::fs::copy(env!("CARGO_BIN_EXE_ax"), &staged).unwrap();
    std::fs::OpenOptions::new()
        .append(true)
        .open(&staged)
        .unwrap()
        .write_all(b"\0")
        .unwrap();
    std::fs::rename(&staged, &exe).unwrap();

    let status = wait_until("the daemon to exit", Duration::from_secs(10), || {
        daemon.0.try_wait().unwrap()
    });
    assert!(status.success(), "clean exit, got {status}");
    assert!(daemon_info(&p.root).is_none(), "daemon.json removed");
}

struct Client {
    stdin: ChildStdin,
    lines: mpsc::Receiver<Value>,
}

impl Client {
    fn start(stdout: ChildStdout) -> mpsc::Receiver<Value> {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Ok(v) = serde_json::from_str::<Value>(&line) {
                    if tx.send(v).is_err() {
                        break;
                    }
                }
            }
        });
        rx
    }

    fn send(&mut self, msg: Value) {
        writeln!(self.stdin, "{msg}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn reply(&self, id: u64) -> Value {
        let end = Instant::now() + Duration::from_secs(30);
        loop {
            let left = end.saturating_duration_since(Instant::now());
            let msg = self
                .lines
                .recv_timeout(left)
                .expect("no reply from the proxy");
            if msg["id"] == id {
                return msg;
            }
        }
    }

    fn call_status(&mut self, id: u64) -> Value {
        self.send(json!({"jsonrpc":"2.0","id":id,"method":"tools/call",
            "params":{"name":"ax_status","arguments":{}}}));
        self.reply(id)
    }
}

/// An `ax serve --mcp` proxy on `exe`, with the MCP handshake done.
fn start_proxy(exe: &Path, p: &Project) -> (KillOnDrop, Client) {
    let mut proxy = KillOnDrop(
        ax(exe, p.home.path())
            .args(["serve", "--mcp", "--path"])
            .arg(&p.root)
            .current_dir(&p.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(std::fs::File::create(p.home.path().join("proxy.stderr")).unwrap())
            .spawn()
            .unwrap(),
    );
    let mut client = Client {
        stdin: proxy.0.stdin.take().unwrap(),
        lines: Client::start(proxy.0.stdout.take().unwrap()),
    };
    client.send(
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":"2024-11-05","capabilities":{},
        "clientInfo":{"name":"daemon-lifecycle-test","version":"0"}}}),
    );
    assert!(client.reply(1).get("result").is_some(), "initialize");
    client.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    (proxy, client)
}

fn proxy_stderr(p: &Project) -> String {
    std::fs::read_to_string(p.home.path().join("proxy.stderr")).unwrap_or_default()
}

fn start_daemon(exe: &Path, p: &Project) -> KillOnDrop {
    KillOnDrop(
        ax(exe, p.home.path())
            .args(["serve", "--mcp", "--daemon", "--path"])
            .arg(&p.root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    )
}

/// Two copies of the ax binary; the second has the later modification time.
fn older_and_newer(dir: &Path) -> (PathBuf, PathBuf) {
    let older = dir.join("older/ax");
    let newer = dir.join("newer/ax");
    let now = std::time::SystemTime::now();
    // `fs::copy` keeps the source's mtime on macOS, so both times are set explicitly.
    for (exe, mtime) in [(&older, now - Duration::from_secs(60)), (&newer, now)] {
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::copy(env!("CARGO_BIN_EXE_ax"), exe).unwrap();
        std::fs::File::options()
            .write(true)
            .open(exe)
            .unwrap()
            .set_modified(mtime)
            .unwrap();
    }
    (older, newer)
}

fn daemon_exe_path(root: &Path) -> Option<String> {
    daemon_info(root).and_then(|i| i["exe"]["path"].as_str().map(str::to_string))
}

#[test]
fn e3_a_newer_proxy_restarts_an_older_daemon_on_its_own_binary() {
    let p = project();
    let bins = tempfile::tempdir().unwrap();
    let (older, newer) = older_and_newer(bins.path());
    let _old_daemon = start_daemon(&older, &p);
    let old_pid = wait_until("the older daemon", Duration::from_secs(20), || {
        daemon_info(&p.root).and_then(|i| i["pid"].as_u64())
    });

    let (_proxy, mut client) = start_proxy(&newer, &p);
    let reply = client.call_status(2);
    assert!(
        reply.get("result").is_some(),
        "call through the new daemon: {reply}"
    );
    let info = daemon_info(&p.root).expect("a daemon serves the project");
    assert_eq!(
        daemon_exe_path(&p.root).as_deref(),
        newer.to_str(),
        "{}",
        proxy_stderr(&p)
    );
    assert_ne!(info["pid"].as_u64(), Some(old_pid));
    kill(info["pid"].as_u64().unwrap());
}

#[test]
fn e4_an_older_proxy_attaches_to_a_newer_daemon_without_restarting_it() {
    let p = project();
    let bins = tempfile::tempdir().unwrap();
    let (older, newer) = older_and_newer(bins.path());
    let _daemon = start_daemon(&newer, &p);
    let pid = wait_until("the newer daemon", Duration::from_secs(20), || {
        daemon_info(&p.root).and_then(|i| i["pid"].as_u64())
    });

    let (_proxy, mut client) = start_proxy(&older, &p);
    let reply = client.call_status(2);
    assert!(
        reply.get("result").is_some(),
        "call through the newer daemon: {reply}"
    );
    let info = daemon_info(&p.root).expect("a daemon serves the project");
    assert_eq!(info["pid"].as_u64(), Some(pid), "{}", proxy_stderr(&p));
    assert_eq!(daemon_exe_path(&p.root).as_deref(), newer.to_str());
}

#[test]
fn d7_a_proxy_keeps_serving_after_its_daemon_is_killed() {
    let p = project();
    let (mut proxy, mut client) = start_proxy(Path::new(env!("CARGO_BIN_EXE_ax")), &p);
    assert!(client.call_status(2).get("result").is_some(), "first call");

    let first = daemon_info(&p.root).expect("the proxy runs through a daemon");
    let first_pid = first["pid"].as_u64().unwrap();
    let socket = PathBuf::from(first["socket_path"].as_str().expect("unix socket daemon"));
    kill(first_pid);
    // A call sent while the daemon is still dying is in flight and gets the retry error (D2);
    // this checks the call after it.
    wait_until(
        "the killed daemon to stop listening",
        Duration::from_secs(10),
        || {
            std::os::unix::net::UnixStream::connect(&socket)
                .is_err()
                .then_some(())
        },
    );

    let reply = client.call_status(3);
    assert!(
        reply.get("result").is_some(),
        "call after the restart: {reply}"
    );
    let second = wait_until("a new daemon", Duration::from_secs(10), || {
        daemon_info(&p.root).filter(|i| i["pid"].as_u64() != Some(first_pid))
    });
    assert!(
        proxy.0.try_wait().unwrap().is_none(),
        "the proxy is still running"
    );
    // A zombie still answers `kill -0`: the proxy that spawned the daemon must reap it.
    wait_until(
        "the killed daemon to be reaped",
        Duration::from_secs(5),
        || {
            let alive = Command::new("kill")
                .args(["-0", &first_pid.to_string()])
                .stderr(Stdio::null())
                .status()
                .unwrap()
                .success();
            (!alive).then_some(())
        },
    );

    drop(client);
    let status = wait_until("the proxy to exit", Duration::from_secs(10), || {
        proxy.0.try_wait().unwrap()
    });
    assert!(
        status.success(),
        "stdin closed means a clean exit, got {status}: {}",
        proxy_stderr(&p)
    );
    kill(second["pid"].as_u64().unwrap());
}
