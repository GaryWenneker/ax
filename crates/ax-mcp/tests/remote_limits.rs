//! Session and request limits are per principal, so one caller cannot lock out the rest.
use ax_mcp::remote::{
    auth::TokenVerifier,
    config::{ApiKey, OAuthConfig, Project, RemoteConfig},
    server::{router, RemoteState},
};
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use jsonwebtoken::jwk::JwkSet;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tower::ServiceExt;

fn key(id: &str) -> ApiKey {
    ApiKey {
        id: id.into(),
        sha256: hex::encode(Sha256::digest(id.as_bytes())),
        projects: vec!["alpha".into()],
        write: false,
        admin: false,
    }
}

fn setup() -> (tempfile::TempDir, RemoteState, Router) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("alpha");
    std::fs::create_dir_all(&root).unwrap();
    let config = Arc::new(RemoteConfig {
        public_url: "https://ax.example.com".into(),
        oauth: OAuthConfig {
            built_in: false,
            issuer: "https://issuer.example.com/".into(),
            jwks_uri: "https://issuer.example.com/jwks".into(),
            clock_skew_seconds: ax_mcp::remote::config::DEFAULT_CLOCK_SKEW_SECONDS,
        },
        projects: vec![Project {
            id: "alpha".into(),
            root,
            subjects: vec![],
            write_subjects: vec![],
            admin_subjects: vec![],
        }],
        api_keys: ["key-a", "key-b", "key-c", "key-d", "key-e"]
            .into_iter()
            .map(key)
            .collect(),
        allowed_origins: vec![],
    });
    let verifier = TokenVerifier::with_keys(
        config.clone(),
        JwkSet { keys: vec![] },
        reqwest::Client::new(),
    );
    let state = RemoteState::new(config, verifier);
    let app = router(state.clone());
    (dir, state, app)
}

async fn post(
    app: &Router,
    token: &str,
    session: Option<&str>,
    body: Value,
) -> (StatusCode, Option<String>) {
    let mut request = Request::builder()
        .method("POST")
        .uri("/projects/alpha/mcp")
        .header("accept", "application/json, text/event-stream")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"));
    if let Some(session) = session {
        request = request
            .header("mcp-session-id", session)
            .header("mcp-protocol-version", "2025-11-25");
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let session = response
        .headers()
        .get("mcp-session-id")
        .map(|v| v.to_str().unwrap().to_owned());
    let _ = to_bytes(response.into_body(), 1024 * 1024).await;
    (status, session)
}

async fn initialize(app: &Router, token: &str) -> (StatusCode, Option<String>) {
    post(
        app,
        token,
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}),
    )
    .await
}

fn tool_call() -> Value {
    json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ax_status","arguments":{}}})
}

#[tokio::test]
async fn one_principal_cannot_use_up_every_session() {
    let (_dir, _state, app) = setup();
    let mut first = None;
    for i in 0..130 {
        let (status, session) = initialize(&app, "key-a").await;
        assert_eq!(
            status,
            StatusCode::OK,
            "initialize {i} of one principal was refused"
        );
        first.get_or_insert(session.unwrap());
    }
    let (status, _) = initialize(&app, "key-b").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "another principal was locked out by one principal's sessions"
    );
    let (status, _) = post(&app, "key-a", first.as_deref(), tool_call()).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "the principal's oldest session was not the one closed"
    );
}

#[tokio::test]
async fn one_principal_cannot_hold_every_request_slot() {
    let (_dir, state, app) = setup();
    let mut sessions = Vec::new();
    for _ in 0..8 {
        sessions.push(initialize(&app, "key-a").await.1.unwrap());
    }
    let (_, other) = initialize(&app, "key-b").await;
    // A running index keeps each tool call waiting (and holding its slots) for a while.
    let held = state.hold_project("alpha").await;
    let mut calls = Vec::new();
    for session in sessions.iter().chain(sessions.iter()) {
        let app = app.clone();
        let session = session.clone();
        calls.push(tokio::spawn(async move {
            post(&app, "key-a", Some(&session), tool_call()).await.0
        }));
    }
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let (status, _) = post(&app, "key-b", other.as_deref(), tool_call()).await;
    assert_ne!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "another principal found the server busy with one principal's calls"
    );
    drop(held);
    let mut admitted = 0;
    for call in calls {
        if call.await.unwrap() != StatusCode::TOO_MANY_REQUESTS {
            admitted += 1;
        }
    }
    assert!(
        admitted <= 4,
        "{admitted} calls of one principal ran at once"
    );
}

#[tokio::test]
async fn callers_without_a_valid_token_never_find_the_server_busy() {
    let (_dir, state, app) = setup();
    // Four principals with four calls each fill every shared request slot.
    let mut sessions = Vec::new();
    for token in ["key-a", "key-b", "key-c", "key-d"] {
        for _ in 0..4 {
            sessions.push((token, initialize(&app, token).await.1.unwrap()));
        }
    }
    let held = state.hold_project("alpha").await;
    let mut calls = Vec::new();
    for (token, session) in sessions {
        let app = app.clone();
        calls.push(tokio::spawn(async move {
            post(&app, token, Some(&session), tool_call()).await.0
        }));
    }
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let (status, _) = initialize(&app, "not-a-valid-token").await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "an unauthenticated request must be refused for its token, not counted against the shared slots"
    );
    drop(held);
    for call in calls {
        call.await.unwrap();
    }
}
