//! `axc_` client tokens are checked against one shared global database pool.
//! This binary sets `AX_GLOBAL_DB`, so it holds only tests that agree on it.
use ax_mcp::remote::{
    auth::TokenVerifier,
    config::{OAuthConfig, Project, RemoteConfig},
    server::{router, RemoteState},
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use jsonwebtoken::jwk::JwkSet;
use std::{path::Path, sync::Arc};
use tower::ServiceExt;

fn config(root: &Path, subject: &str) -> Arc<RemoteConfig> {
    Arc::new(RemoteConfig {
        public_url: "https://ax.example.com".into(),
        oauth: OAuthConfig {
            built_in: false,
            issuer: "https://issuer.example.com/".into(),
            jwks_uri: "https://issuer.example.com/jwks".into(),
            clock_skew_seconds: ax_mcp::remote::config::DEFAULT_CLOCK_SKEW_SECONDS,
        },
        projects: vec![Project {
            id: "alpha".into(),
            root: root.to_path_buf(),
            subjects: vec![subject.into()],
            write_subjects: vec![],
            admin_subjects: vec![],
        }],
        api_keys: vec![],
        allowed_origins: vec![],
        client_address_header: None,
    })
}

fn build(state: RemoteState) -> Router {
    router(state)
}

fn state(config: Arc<RemoteConfig>) -> RemoteState {
    let verifier = TokenVerifier::with_keys(
        config.clone(),
        JwkSet { keys: vec![] },
        reqwest::Client::new(),
    );
    RemoteState::new(config, verifier)
}

async fn initialize(app: &Router, token: &str) -> StatusCode {
    let request = Request::builder()
        .method("POST")
        .uri("/projects/alpha/mcp")
        .header("accept", "application/json, text/event-stream")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#,
        ))
        .unwrap();
    app.clone().oneshot(request).await.unwrap().status()
}

fn remove_db(path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
    }
}

#[tokio::test]
async fn client_tokens_use_one_shared_pool_instead_of_opening_the_database_per_request() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("alpha");
    std::fs::create_dir_all(&project).unwrap();
    let global = dir.path().join("global.db");
    std::env::set_var("AX_GLOBAL_DB", &global);

    // Without a pool from the server, the first client token opens the database once.
    let app = build(state(config(&project, "member-1")));
    let junk = format!("axc_{}", "0".repeat(64));
    assert_eq!(initialize(&app, &junk).await, StatusCode::UNAUTHORIZED);
    assert!(global.exists());
    remove_db(&global);
    for _ in 0..3 {
        assert_eq!(initialize(&app, &junk).await, StatusCode::UNAUTHORIZED);
    }
    assert!(
        !global.exists(),
        "a request opened and re-initialized the global database again"
    );

    // The pool the server opened at startup authenticates client tokens.
    let pool = ax_global_db::open_and_init(&dir.path().join("server.db"))
        .await
        .unwrap();
    use ax_global_db::access;
    access::bootstrap_owner(&pool, "member-1", "Owner", None)
        .await
        .unwrap();
    access::create_client(
        &pool,
        "client-1",
        "member-1",
        "IDE",
        "ide",
        &["context:read".to_string()],
    )
    .await
    .unwrap();
    let issued = access::issue_client_credential(&pool, "client-1", None)
        .await
        .unwrap();
    let app = build(state(config(&project, "member-1")).with_client_db(pool));
    assert_eq!(initialize(&app, &issued.token).await, StatusCode::OK);
    assert!(
        !global.exists(),
        "the server's own pool was not used for client tokens"
    );
}
