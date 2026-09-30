//! `GET /api/changes` (docs/specs/live-updates.md).

use std::sync::OnceLock;
use std::time::Duration;

use ax_web::WebHub;
use axum::body::{Body, BodyDataStream};
use axum::http::Request;
use futures_util::StreamExt;
use tempfile::TempDir;
use tower::ServiceExt;
use tower_http::cors::CorsLayer;

static HOME: OnceLock<TempDir> = OnceLock::new();

fn isolate() {
    HOME.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", dir.path());
        std::env::set_var("AX_HOME_DIR", dir.path());
        std::env::set_var("AX_GLOBAL_DB", dir.path().join("global.db"));
        dir
    });
}

async fn open_hub() -> (TempDir, WebHub) {
    isolate();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let hub = WebHub::open(root, false, 0).await.unwrap();
    (dir, hub)
}

async fn subscribe(hub: &WebHub) -> BodyDataStream {
    let app = hub.nest_routers(CorsLayer::new());
    let res = app
        .oneshot(Request::get("/api/changes").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert!(res.status().is_success());
    res.into_body().into_data_stream()
}

/// Collect `change` event payloads for `window`, ignoring keep-alive comments.
async fn collect(stream: &mut BodyDataStream, window: Duration) -> Vec<String> {
    let mut out = Vec::new();
    let deadline = tokio::time::Instant::now() + window;
    while let Ok(Some(chunk)) = tokio::time::timeout_at(deadline, stream.next()).await {
        let text = String::from_utf8_lossy(&chunk.unwrap()).to_string();
        for line in text.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                out.push(data.to_string());
            }
        }
    }
    out
}

async fn remember(hub: &WebHub, title: &str) {
    let ws = hub.read().await;
    ax_memory::remember(
        &ws.graph_pool,
        ax_memory::RememberInput {
            title: title.into(),
            body: "body".into(),
            kind: Some("decision".into()),
            tags: vec![],
            files: vec![],
            source: None,
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn a_new_memory_produces_one_memory_event() {
    let (_dir, hub) = open_hub().await;
    let mut stream = subscribe(&hub).await;
    // Let the feed record its baseline before the write.
    collect(&mut stream, Duration::from_millis(1500)).await;

    remember(&hub, "first").await;
    let events = collect(&mut stream, Duration::from_millis(2500)).await;

    assert_eq!(events.len(), 1, "events: {events:?}");
    let ev: serde_json::Value = serde_json::from_str(&events[0]).unwrap();
    assert_eq!(ev["topic"], "memory");
    assert!(ev["version"].as_str().is_some_and(|v| !v.is_empty()));
}

#[tokio::test]
async fn an_idle_project_sends_no_events() {
    let (_dir, hub) = open_hub().await;
    let mut stream = subscribe(&hub).await;
    let events = collect(&mut stream, Duration::from_secs(5)).await;
    assert!(events.is_empty(), "events: {events:?}");
}
