//! Offload sends project source to an outside model, so it must only run when the user
//! switched it on, and only when its answer is used.

use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use tempfile::{tempdir, TempDir};

static ENV_LOCK: Mutex<()> = Mutex::new(());

const PROVIDER_KEY_VARS: &[&str] = &[
    "OPENAI_API_KEY",
    "CEREBRAS_API_KEY",
    "GROQ_API_KEY",
    "TOGETHER_API_KEY",
    "FIREWORKS_API_KEY",
    "OPENROUTER_API_KEY",
    "AX_OFFLOAD_URL",
    "AX_OFFLOAD_KEY",
    "AX_OFFLOAD_DISABLE",
];

fn ax_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_ax")
        .map(PathBuf::from)
        .expect("CARGO_BIN_EXE_ax")
}

struct Sandbox {
    _guard: MutexGuard<'static, ()>,
    _dir: TempDir,
    home: PathBuf,
    project: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempdir().unwrap();
        let home = dir.path().join("home");
        let project = dir.path().join("project");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(project.join("src")).unwrap();
        fs::write(
            project.join("src").join("lib.rs"),
            "pub fn ping() -> i32 { 1 }\n",
        )
        .unwrap();
        Self {
            _guard: guard,
            _dir: dir,
            home,
            project,
        }
    }

    fn ax(&self, args: &[&str], env: &[(&str, &str)]) -> (i32, String, String) {
        let mut cmd = Command::new(ax_bin());
        cmd.current_dir(&self.project)
            .args(args)
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env("AX_HOME_DIR", &self.home)
            .env("AX_USAGE_DB", self.home.join("usage.db"))
            .env("AX_GLOBAL_DB", self.home.join("global.db"))
            .env("AX_NO_UPDATE_CHECK", "1")
            .stdin(std::process::Stdio::null());
        for var in PROVIDER_KEY_VARS {
            cmd.env_remove(var);
        }
        for (k, v) in env {
            cmd.env(k, v);
        }
        let out = cmd.output().expect("spawn ax");
        (
            out.status.code().unwrap_or(1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    fn init(&self, env: &[(&str, &str)]) -> String {
        let project = self.project.display().to_string();
        let (code, out, err) = self.ax(&["init", &project], env);
        assert_eq!(code, 0, "init failed: {out} {err}");
        out
    }

    fn global_config(&self) -> serde_json::Value {
        let path = self.home.join(".ax").join("config.json");
        fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| serde_json::json!({}))
    }
}

/// A local stand-in for the offload endpoint: it counts connections and closes each one.
fn counting_endpoint() -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            counter.fetch_add(1, Ordering::SeqCst);
            drop(stream);
        }
    });
    (url, hits)
}

fn offload_url(config: &serde_json::Value) -> Option<&str> {
    config.get("offload")?.get("url")?.as_str()
}

#[test]
fn explore_json_does_not_call_the_offload_endpoint() {
    let sandbox = Sandbox::new();
    sandbox.init(&[]);
    let (url, hits) = counting_endpoint();
    let env = [
        ("AX_OFFLOAD_URL", url.as_str()),
        ("AX_OFFLOAD_KEY", "test-key"),
        ("AX_OFFLOAD_TIMEOUT_MS", "3000"),
    ];

    // Text mode prints the synthesized answer, so it does call the endpoint. This proves
    // the endpoint is wired up for the JSON check below.
    let (code, _, err) = sandbox.ax(&["explore", "ping"], &env);
    assert_eq!(code, 0, "{err}");
    assert!(
        hits.load(Ordering::SeqCst) >= 1,
        "text explore uses offload"
    );

    let before = hits.load(Ordering::SeqCst);
    let (code, out, err) = sandbox.ax(&["explore", "ping", "--json"], &env);
    assert_eq!(code, 0, "{err}");
    serde_json::from_str::<serde_json::Value>(&out).expect("explore --json prints JSON");
    assert_eq!(
        hits.load(Ordering::SeqCst),
        before,
        "explore --json must not send the explore text to the offload endpoint"
    );
}

#[test]
fn init_does_not_switch_on_offload_from_an_api_key_in_the_environment() {
    let sandbox = Sandbox::new();
    let out = sandbox.init(&[("OPENAI_API_KEY", "sk-test-not-real")]);
    let config = sandbox.global_config();
    assert_eq!(
        offload_url(&config),
        None,
        "init must not activate an offload provider without consent: {config}"
    );
    assert!(
        !out.contains("Offload active"),
        "init must not report offload as active: {out}"
    );
    assert!(
        out.contains("ax offload"),
        "init names the command that switches offload on: {out}"
    );
}

#[test]
fn init_keeps_an_offload_endpoint_the_user_configured() {
    let sandbox = Sandbox::new();
    let ax_dir = sandbox.home.join(".ax");
    fs::create_dir_all(&ax_dir).unwrap();
    fs::write(
        ax_dir.join("config.json"),
        r#"{"offload":{"url":"http://127.0.0.1:9/v1","model":"m"}}"#,
    )
    .unwrap();
    sandbox.init(&[("OPENAI_API_KEY", "sk-test-not-real")]);
    let config = sandbox.global_config();
    assert_eq!(offload_url(&config), Some("http://127.0.0.1:9/v1"));
}
