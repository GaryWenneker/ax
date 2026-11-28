//! Replay routes behind the manifest Authority panel, per
//! docs/specs/specs-hierarchy-ui/REPLAY-ACTION.md.

mod common;

use ax_usage::context_authority::{
    resolve_context, store_manifest, store_receipt, ContextCandidate, ContextManifest, ReplayMode,
};
use ax_web::WebHub;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tempfile::TempDir;
use tower::ServiceExt;
use tower_http::cors::{Any, CorsLayer};

/// Point `HOME`, `AX_HOME_DIR`, and `AX_GLOBAL_DB` at a temp dir shared by every test in this binary.
fn isolate() {
    common::isolate_home();
}

struct Fx {
    _dir: TempDir,
    hub: WebHub,
    app: Router,
}

async fn fixture(readonly: bool) -> Fx {
    isolate();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let hub = WebHub::open(root, readonly, 0).await.unwrap();
    let app = hub.nest_routers(
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any),
    );
    Fx {
        _dir: dir,
        hub,
        app,
    }
}

async fn seed_manifest(hub: &WebHub, turn_id: &str) -> String {
    let pool = hub.read().await.graph_pool.clone();
    let receipt = resolve_context(
        ReplayMode::Current,
        None,
        &[],
        100,
        &[ContextCandidate {
            id: "rule-english".into(),
            kind: "rule".into(),
            mandatory: true,
            content_hash: "r1".into(),
            body: "Write in English.".into(),
            tokens: 10,
            created_at_ms: 1,
            tags: Vec::new(),
        }],
    );
    let receipt_id = store_receipt(&pool, "ax", &receipt).await.unwrap();
    store_manifest(
        &pool,
        &ContextManifest {
            project_id: "ax".into(),
            session_id: "session-1".into(),
            turn_id: turn_id.into(),
            agent_id: "agent-ax".into(),
            provider_chat_id: None,
            mode: ReplayMode::Current,
            cutoff_ms: None,
            snapshot_id: None,
            receipt_id,
            model: "test-model".into(),
            tool_versions: "ax_explore=1".into(),
            commit_hash: "abc123".into(),
        },
    )
    .await
    .unwrap()
}

async fn break_seal(hub: &WebHub, manifest_id: &str) {
    let pool = hub.read().await.graph_pool.clone();
    sqlx::query("UPDATE context_manifests SET model = 'other-model' WHERE id = ?1")
        .bind(manifest_id)
        .execute(&pool)
        .await
        .unwrap();
}

async fn send(app: &Router, method: &str, path: &str, body: Option<Value>) -> Value {
    let mut request = Request::builder().method(method).uri(path);
    let body = match body {
        Some(value) => {
            request = request.header("content-type", "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };
    let response = app
        .clone()
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "{method} {path}");
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn start(app: &Router, manifest_id: &str) -> Value {
    send(
        app,
        "POST",
        "/api/context-authority/replay",
        Some(json!({ "manifestId": manifest_id })),
    )
    .await
}

#[tokio::test]
async fn a_replay_uses_the_stored_identity_and_claims_no_score() {
    let fx = fixture(false).await;
    let manifest_id = seed_manifest(&fx.hub, "turn-1").await;

    let started = start(&fx.app, &manifest_id).await;
    assert_eq!(started["ok"], true, "{started}");
    let run = &started["run"];
    assert_eq!(run["manifest_id"], manifest_id.as_str());
    assert_eq!(run["session_id"], "session-1");
    assert_eq!(run["turn_id"], "turn-1");
    assert_eq!(run["agent_id"], "agent-ax");
    assert_eq!(run["automated"], "not_implemented");
    assert_eq!(run["score"], "not_tested");
    assert_eq!(run["approval"], "unapproved");
    assert!(!started.to_string().contains("95%"), "{started}");

    let again = start(&fx.app, &manifest_id).await;
    assert_eq!(again["run"]["id"], run["id"], "{again}");

    let id = run["id"].as_str().unwrap();
    let shown = send(
        &fx.app,
        "GET",
        &format!("/api/context-authority/replay/{id}"),
        None,
    )
    .await;
    assert_eq!(shown, started);

    let listed = send(&fx.app, "GET", "/api/context-authority/manifests", None).await;
    let row = &listed["manifests"][0];
    assert_eq!(row["id"], manifest_id.as_str());
    assert_eq!(row["integrity_ok"], true);
    assert_eq!(row["replay_status"], "not_tested");
    assert_eq!(row["uncertainties"], json!(["unapproved", "not_tested"]));
}

#[tokio::test]
async fn a_broken_seal_cannot_start_a_replay() {
    let fx = fixture(false).await;
    let manifest_id = seed_manifest(&fx.hub, "turn-1").await;
    break_seal(&fx.hub, &manifest_id).await;

    let refused = start(&fx.app, &manifest_id).await;
    assert_eq!(refused, json!({ "ok": false, "error": "integrity_failed" }));

    let listed = send(&fx.app, "GET", "/api/context-authority/manifests", None).await;
    let row = &listed["manifests"][0];
    assert_eq!(row["integrity_ok"], false);
    assert_eq!(row["replay_status"], "not_run");
    assert_eq!(
        row["uncertainties"],
        json!(["unapproved", "integrity_failed", "not_run"])
    );
}

#[tokio::test]
async fn an_unknown_manifest_or_replay_is_missing() {
    let fx = fixture(false).await;
    assert_eq!(
        start(&fx.app, "no-such-manifest").await,
        json!({ "ok": false, "error": "missing manifest" })
    );
    assert_eq!(
        start(&fx.app, "  ").await,
        json!({ "ok": false, "error": "manifest is required" })
    );
    assert_eq!(
        send(
            &fx.app,
            "GET",
            "/api/context-authority/replay/no-such-run",
            None
        )
        .await,
        json!({ "ok": false, "error": "missing replay" })
    );
}

#[tokio::test]
async fn a_readonly_hub_refuses_to_start_a_replay() {
    let fx = fixture(true).await;
    assert_eq!(
        start(&fx.app, "any-manifest").await,
        json!({ "ok": false, "error": "readonly" })
    );
}

/// Manual walkthrough seed, not part of the suite: one intact and one broken-seal manifest in
/// `$AX_WALKTHROUGH_ROOT/.ax/ax.db`. Refuses a project outside the system temp directory.
#[tokio::test]
#[ignore = "manual walkthrough seed; needs AX_WALKTHROUGH_ROOT"]
async fn seed_walkthrough_manifests() {
    isolate();
    let root = std::path::PathBuf::from(std::env::var("AX_WALKTHROUGH_ROOT").unwrap())
        .canonicalize()
        .unwrap();
    let temp = std::env::temp_dir().canonicalize().unwrap();
    assert!(
        root.starts_with(&temp),
        "{} is outside {}",
        root.display(),
        temp.display()
    );
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let hub = WebHub::open(root, false, 0).await.unwrap();
    let intact = seed_manifest(&hub, "turn-intact").await;
    let broken = seed_manifest(&hub, "turn-broken").await;
    break_seal(&hub, &broken).await;
    println!("intact={intact} broken={broken}");
}
