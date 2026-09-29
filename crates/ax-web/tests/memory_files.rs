//! `GET /api/memory/{id}/file-changes`, `/diff` and `/image`
//! (docs/specs/policy-graph-edit-autogroup-memory-diff.md, section 3).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use ax_web::WebHub;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::Value;
use tempfile::TempDir;
use tower::ServiceExt;
use tower_http::cors::CorsLayer;

static HOME: OnceLock<TempDir> = OnceLock::new();

fn isolate() {
    HOME.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", dir.path());
        std::env::set_var("AX_GLOBAL_DB", dir.path().join("global.db"));
        dir
    });
}

fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(root)
        .args([
            "-c",
            "user.email=t@example.com",
            "-c",
            "user.name=t",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

const PNG: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, b'I', b'H', b'D', b'R',
];

struct Fx {
    _dir: TempDir,
    app: Router,
    memory_id: String,
    image: PathBuf,
}

async fn fixture() -> Fx {
    isolate();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("proj");
    std::fs::create_dir_all(root.join("src")).unwrap();
    git(&root, &["init", "-q"]);
    std::fs::write(root.join("src/a.rs"), "fn a() {}\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "initial"]);
    std::fs::write(root.join("src/a.rs"), "fn a() { fixed() }\n").unwrap();
    git(&root, &["commit", "-q", "-am", "fix a"]);
    let hash = git(&root, &["rev-parse", "HEAD"]);
    let image = dir.path().join("shot.png");
    std::fs::write(&image, PNG).unwrap();

    std::fs::create_dir_all(root.join(".ax")).unwrap();
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let hub = WebHub::open(root.clone(), false, 0).await.unwrap();
    let memory_id = {
        let ws = hub.read().await;
        ax_memory::remember(
            &ws.graph_pool,
            ax_memory::RememberInput {
                title: "Fix a".into(),
                body: format!(
                    "Fix a ![shot]({})\n\nFiles: src/a.rs\n\nCommits:\n- {hash} fix a\n\nChanges:\nM src/a.rs",
                    image.display()
                ),
                kind: Some("bug_fix".into()),
                tags: vec![],
                files: vec!["src/a.rs".into()],
                source: None,
            },
        )
        .await
        .unwrap()
        .id
    };
    Fx {
        _dir: dir,
        app: hub.nest_routers(CorsLayer::new()),
        memory_id,
        image,
    }
}

async fn get(app: &Router, path: &str) -> (StatusCode, Vec<u8>) {
    let res = app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, bytes.to_vec())
}

fn json(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap_or(Value::Null)
}

#[tokio::test]
async fn m1_file_changes_report_the_change_kind() {
    let fx = fixture().await;
    let (status, body) = get(
        &fx.app,
        &format!("/api/memory/{}/file-changes", fx.memory_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        json(&body)["files"]["src/a.rs"],
        "modified",
        "{}",
        json(&body)
    );
}

#[tokio::test]
async fn m2_diff_returns_the_changed_lines() {
    let fx = fixture().await;
    let (status, body) = get(
        &fx.app,
        &format!("/api/memory/{}/diff?path=src/a.rs", fx.memory_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let v = json(&body);
    let lines = v["files"][0]["hunks"][0]["lines"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        lines
            .iter()
            .any(|l| l["kind"] == "added" && l["text"] == "fn a() { fixed() }"),
        "{v}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l["kind"] == "removed" && l["text"] == "fn a() {}"),
        "{v}"
    );
}

#[tokio::test]
async fn m3_image_in_the_body_is_served() {
    let fx = fixture().await;
    let src = urlencoding(&fx.image.display().to_string());
    let (status, body) = get(
        &fx.app,
        &format!("/api/memory/{}/image?src={src}", fx.memory_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, PNG);
}

#[tokio::test]
async fn m5_a_turn_without_a_commit_diffs_the_worktree() {
    isolate();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("proj");
    std::fs::create_dir_all(root.join("src")).unwrap();
    git(&root, &["init", "-q"]);
    std::fs::write(root.join("src/a.rs"), "fn a() {}\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "initial"]);
    std::fs::write(root.join("src/a.rs"), "fn a() { later() }\n").unwrap();
    std::fs::write(root.join("src/new.rs"), "fn brand_new() {}\n").unwrap();
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let hub = WebHub::open(root, false, 0).await.unwrap();
    let id = {
        let ws = hub.read().await;
        ax_memory::remember(
            &ws.graph_pool,
            ax_memory::RememberInput {
                title: "Uncommitted turn".into(),
                body: "prompt\n\nFiles: src/a.rs, src/new.rs\n\nChanges:\nM src/a.rs\nA src/new.rs"
                    .into(),
                kind: Some("turn".into()),
                tags: vec![],
                files: vec!["src/a.rs".into(), "src/new.rs".into()],
                source: None,
            },
        )
        .await
        .unwrap()
        .id
    };
    let app = hub.nest_routers(CorsLayer::new());

    let (status, body) = get(&app, &format!("/api/memory/{id}/diff?path=src/a.rs")).await;
    assert_eq!(status, StatusCode::OK);
    let v = json(&body);
    let lines = v["files"][0]["hunks"][0]["lines"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        lines
            .iter()
            .any(|l| l["kind"] == "added" && l["text"] == "fn a() { later() }"),
        "{v}"
    );

    let (status, body) = get(&app, &format!("/api/memory/{id}/diff?path=src/new.rs")).await;
    assert_eq!(status, StatusCode::OK);
    let v = json(&body);
    let lines = v["files"][0]["hunks"][0]["lines"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        lines
            .iter()
            .any(|l| l["kind"] == "added" && l["text"] == "fn brand_new() {}"),
        "{v}"
    );

    let (status, body) = get(&app, &format!("/api/memory/{id}/diff?path=src/secret.rs")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json(&body)["files"].as_array().map(|a| a.len()), Some(0));
}

#[tokio::test]
async fn m4_unknown_memory_is_404_and_path_traversal_is_400() {
    let fx = fixture().await;
    for tail in ["file-changes", "file-links", "diff"] {
        let (status, _) = get(&fx.app, &format!("/api/memory/nope/{tail}")).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{tail}");
    }
    let (status, _) = get(
        &fx.app,
        &format!("/api/memory/{}/diff?path=../etc/passwd", fx.memory_id),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

fn urlencoding(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}
