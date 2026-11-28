//! `GET /api/changes` (docs/specs/live-updates.md).

mod common;

use std::time::Duration;

use ax_web::WebHub;
use axum::body::{Body, BodyDataStream};
use axum::http::Request;
use futures_util::StreamExt;
use tempfile::TempDir;
use tower::ServiceExt;
use tower_http::cors::CorsLayer;

fn isolate() {
    common::isolate_home();
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

/// The longest the tests wait for something the feed must send.
const PATIENCE: Duration = Duration::from_secs(30);

/// Data lines and comments of the next chunks until `done` says stop.
async fn read_until(
    stream: &mut BodyDataStream,
    within: Duration,
    mut done: impl FnMut(&[String], &[String]) -> bool,
) -> (Vec<String>, Vec<String>) {
    let (mut data, mut comments) = (Vec::new(), Vec::new());
    let deadline = tokio::time::Instant::now() + within;
    while !done(&data, &comments) {
        match tokio::time::timeout_at(deadline, stream.next()).await {
            Ok(Some(chunk)) => {
                let text = String::from_utf8_lossy(&chunk.unwrap()).to_string();
                for line in text.lines() {
                    if let Some(d) = line.strip_prefix("data: ") {
                        data.push(d.to_string());
                    } else if let Some(c) = line.strip_prefix(':') {
                        comments.push(c.trim().to_string());
                    }
                }
            }
            _ => break,
        }
    }
    (data, comments)
}

/// Waits until the feed says it recorded its baseline.
async fn wait_ready(stream: &mut BodyDataStream) {
    let (_, comments) = read_until(stream, PATIENCE, |_, c| c.iter().any(|c| c == "ready")).await;
    assert!(
        comments.iter().any(|c| c == "ready"),
        "the feed never reported its baseline: {comments:?}"
    );
}

fn memory_events(data: &[String]) -> Vec<serde_json::Value> {
    data.iter()
        .map(|d| serde_json::from_str::<serde_json::Value>(d).unwrap())
        .filter(|ev| ev["topic"] == "memory")
        .collect()
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
    wait_ready(&mut stream).await;

    remember(&hub, "first").await;
    let (data, _) = read_until(&mut stream, PATIENCE, |d, _| !memory_events(d).is_empty()).await;
    // A second event for the same write would show within two feed ticks.
    let (more, _) = read_until(&mut stream, Duration::from_millis(2500), |_, _| false).await;
    let events = memory_events(&[data, more].concat());

    assert_eq!(events.len(), 1, "events: {events:?}");
    assert!(events[0]["version"].as_str().is_some_and(|v| !v.is_empty()));
}

#[tokio::test]
async fn an_idle_project_sends_no_events() {
    let (_dir, hub) = open_hub().await;
    let mut stream = subscribe(&hub).await;
    wait_ready(&mut stream).await;
    // Two feed ticks after the baseline.
    let (data, _) = read_until(&mut stream, Duration::from_millis(2500), |_, _| false).await;
    assert!(data.is_empty(), "events: {data:?}");
}
