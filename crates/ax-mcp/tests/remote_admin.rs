//! Admin extension of the remote MCP server (docs/specs/ax-admin/SPEC.md, E1-E7).
//! Real Ax projects behind the real HTTP router; API keys only.
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
use std::sync::{Arc, Once};
use tower::ServiceExt;

const CONTEXT_KEY: &str = "context-only-key";
const WRITE_KEY: &str = "write-only-key";
const ADMIN_KEY: &str = "alpha-admin-key";
const BETA_KEY: &str = "beta-context-key";

const ADMIN_TOOLS: &[&str] = &[
    "ax_sync",
    "ax_index",
    "ax_policy_index",
    "ax_files",
    "ax_cost",
    "ax_tool_economics",
    "ax_budget",
    "ax_optimization_advice",
    "ax_logs",
];

/// The tool list a write-only credential received before the admin extension.
/// E1 compares against this recorded list, so a leak of any new tool fails.
const RECORDED_WRITE_LIST: &[&str] = &[
    "ax_affected",
    "ax_api",
    "ax_cache_status",
    "ax_callees",
    "ax_callers",
    "ax_context",
    "ax_cycles",
    "ax_durable",
    "ax_expand",
    "ax_explore",
    "ax_guard",
    "ax_history",
    "ax_impact",
    "ax_insights",
    "ax_node",
    "ax_path",
    "ax_policy_capture",
    "ax_preflight",
    "ax_recall",
    "ax_remember",
    "ax_report",
    "ax_rules",
    "ax_search",
    "ax_session",
    "ax_skill",
    "ax_stash",
    "ax_status",
];

fn isolate_usage_db() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let dir = tempfile::tempdir().unwrap().keep();
        std::env::set_var("AX_USAGE_DB", dir.join("usage.db"));
        std::env::remove_var("AX_MCP_TOOLS");
    });
}

fn key(id: &str, secret: &str, project: &str, write: bool, admin: bool) -> ApiKey {
    ApiKey {
        id: id.into(),
        sha256: hex::encode(Sha256::digest(secret.as_bytes())),
        projects: vec![project.into()],
        write,
        admin,
    }
}

async fn project(dir: &std::path::Path, id: &str) -> Project {
    let root = dir.join(id);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("lib.rs"), "pub fn remote_fixture() -> u32 { 1 }\n").unwrap();
    let mut ax = ax_core::Ax::init(&root).await.unwrap();
    ax.index_all(
        ax_extraction::orchestrator::IndexOptions {
            quiet: true,
            ..Default::default()
        },
        None,
    )
    .await
    .unwrap();
    drop(ax);
    Project {
        id: id.into(),
        root,
        subjects: vec![id.into()],
        write_subjects: vec![id.into()],
        admin_subjects: vec![],
    }
}

fn base_config(projects: Vec<Project>, api_keys: Vec<ApiKey>) -> RemoteConfig {
    RemoteConfig {
        public_url: "https://ax.example.com".into(),
        oauth: OAuthConfig {
            built_in: false,
            issuer: "https://issuer.example.com/".into(),
            jwks_uri: "https://issuer.example.com/jwks".into(),
        },
        projects,
        api_keys,
        allowed_origins: vec![],
    }
}

async fn fixture() -> (tempfile::TempDir, Router) {
    isolate_usage_db();
    let dir = tempfile::tempdir().unwrap();
    let projects = vec![
        project(dir.path(), "alpha").await,
        project(dir.path(), "beta").await,
    ];
    let mut config = base_config(
        projects,
        vec![
            key("ctx", CONTEXT_KEY, "alpha", false, false),
            key("wr", WRITE_KEY, "alpha", true, false),
            key("adm", ADMIN_KEY, "alpha", true, true),
            key("beta", BETA_KEY, "beta", false, false),
        ],
    );
    config.validate().unwrap();
    let config = Arc::new(config);
    let verifier =
        TokenVerifier::with_keys(config.clone(), JwkSet { keys: vec![] }, reqwest::Client::new());
    (dir, router(RemoteState::new(config, verifier)))
}

async fn send(
    app: &Router,
    project: &str,
    secret: &str,
    session: Option<&str>,
    body: Value,
) -> (StatusCode, Option<String>, Value) {
    let mut request = Request::post(format!("/projects/{project}/mcp"))
        .header("authorization", format!("Bearer {secret}"))
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream");
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
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let bytes = to_bytes(response.into_body(), 8 * 1024 * 1024).await.unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, session, value)
}

async fn open(app: &Router, project: &str, secret: &str) -> String {
    let (status, session, _) = send(
        app,
        project,
        secret,
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"admin-test","version":"1"}}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    session.expect("session id")
}

async fn tool_names(app: &Router, project: &str, secret: &str) -> Vec<String> {
    let session = open(app, project, secret).await;
    let (status, _, body) = send(
        app,
        project,
        secret,
        Some(&session),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let mut names: Vec<String> = body["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect();
    names.sort();
    names
}

async fn call(
    app: &Router,
    project: &str,
    secret: &str,
    session: &str,
    name: &str,
    arguments: Value,
) -> (StatusCode, Value) {
    let (status, _, body) = send(
        app,
        project,
        secret,
        Some(session),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":name,"arguments":arguments}}),
    )
    .await;
    (status, body)
}

fn text(body: &Value) -> String {
    body.pointer("/result/content/0/text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned()
}

#[tokio::test]
async fn e1_non_admin_credentials_see_the_recorded_list_and_cannot_call_admin_tools() {
    let (_dir, app) = fixture().await;
    let write_list = tool_names(&app, "alpha", WRITE_KEY).await;
    assert_eq!(write_list, RECORDED_WRITE_LIST);
    let context_list = tool_names(&app, "alpha", CONTEXT_KEY).await;
    let expected: Vec<&str> = RECORDED_WRITE_LIST
        .iter()
        .copied()
        .filter(|name| *name != "ax_remember")
        .collect();
    assert_eq!(context_list, expected);
    for secret in [CONTEXT_KEY, WRITE_KEY] {
        let session = open(&app, "alpha", secret).await;
        for tool in ADMIN_TOOLS {
            let (status, body) = call(&app, "alpha", secret, &session, tool, json!({})).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{tool} with {secret}");
            assert_eq!(body["error"], "Tool is unavailable remotely", "{tool}");
        }
    }
}

#[tokio::test]
async fn e2_admin_key_lists_and_calls_admin_tools_on_its_own_project_only() {
    let (_dir, app) = fixture().await;
    let list = tool_names(&app, "alpha", ADMIN_KEY).await;
    for tool in ADMIN_TOOLS {
        assert!(list.contains(&tool.to_string()), "{tool} missing from {list:?}");
    }
    for tool in RECORDED_WRITE_LIST {
        assert!(list.contains(&tool.to_string()), "{tool} dropped");
    }
    assert_eq!(list.len(), RECORDED_WRITE_LIST.len() + ADMIN_TOOLS.len());
    let session = open(&app, "alpha", ADMIN_KEY).await;
    let (status, body) = call(&app, "alpha", ADMIN_KEY, &session, "ax_files", json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(body["result"]["isError"], json!(true), "{body}");
    assert!(text(&body).contains("lib.rs"), "{body}");
    // The key is granted on alpha only: beta rejects it before any tool runs.
    let (status, _, _) = send(
        &app,
        "beta",
        ADMIN_KEY,
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn e2_admin_tools_are_described_as_admin_scoped_without_project_arguments() {
    let (_dir, app) = fixture().await;
    let session = open(&app, "alpha", ADMIN_KEY).await;
    let (_, _, body) = send(
        &app,
        "alpha",
        ADMIN_KEY,
        Some(&session),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    )
    .await;
    let tools = body["result"]["tools"].as_array().unwrap();
    for name in ADMIN_TOOLS {
        let tool = tools.iter().find(|t| t["name"] == *name).expect(name);
        assert_eq!(
            tool["securitySchemes"][0]["scopes"],
            json!(["ax:context", "ax:write", "ax:admin"]),
            "{name}"
        );
        assert!(tool.pointer("/inputSchema/properties/projectPath").is_none());
    }
    let read_only = |name: &str| {
        tools.iter().find(|t| t["name"] == name).unwrap()["annotations"]["readOnlyHint"].clone()
    };
    assert_eq!(read_only("ax_logs"), json!(true));
    assert_eq!(read_only("ax_files"), json!(true));
    assert_eq!(read_only("ax_index"), json!(false));
    assert_eq!(read_only("ax_sync"), json!(false));
}

#[tokio::test]
async fn e3_admin_grants_require_write_access() {
    isolate_usage_db();
    let dir = tempfile::tempdir().unwrap();
    let alpha = project(dir.path(), "alpha").await;
    let mut config = base_config(
        vec![alpha.clone()],
        vec![key("bad", "some-key", "alpha", false, true)],
    );
    assert_eq!(
        config.validate().unwrap_err(),
        "Admin API keys must also have write access"
    );
    let mut project = alpha.clone();
    project.write_subjects = vec![];
    project.admin_subjects = vec!["alpha".into()];
    let mut config = base_config(vec![project], vec![]);
    assert_eq!(
        config.validate().unwrap_err(),
        "Admin subjects must also have write access"
    );
    let mut project = alpha;
    project.admin_subjects = vec!["alpha".into()];
    base_config(vec![project], vec![]).validate().unwrap();
}

#[tokio::test]
async fn e4_configuration_without_admin_fields_loads_and_grants_nothing() {
    isolate_usage_db();
    let dir = tempfile::tempdir().unwrap();
    let alpha = project(dir.path(), "alpha").await;
    let file = dir.path().join("remote-mcp.json");
    std::fs::write(
        &file,
        json!({
            "public_url": "https://ax.example.com",
            "oauth": {"issuer": "https://issuer.example.com/", "jwks_uri": "https://issuer.example.com/jwks"},
            "projects": [{"id": "alpha", "root": alpha.root, "subjects": ["alpha"], "write_subjects": ["alpha"]}],
            "api_keys": [{"id": "old", "sha256": hex::encode(Sha256::digest(b"old-key")), "projects": ["alpha"], "write": true}],
            "allowed_origins": []
        })
        .to_string(),
    )
    .unwrap();
    let config = RemoteConfig::load(&file).unwrap();
    assert!(config.projects[0].admin_subjects.is_empty());
    assert!(!config.api_keys[0].admin);
    // Serializing an admin-free configuration stays readable by a server without the extension.
    let text = serde_json::to_string(&config).unwrap();
    assert!(!text.contains("admin"), "{text}");
}

#[tokio::test]
async fn e5_admin_calls_cannot_select_another_project_root() {
    let (dir, app) = fixture().await;
    let session = open(&app, "alpha", ADMIN_KEY).await;
    let beta = dir.path().join("beta");
    for tool in ["ax_index", "ax_sync", "ax_files", "ax_logs"] {
        for field in ["projectPath", "project_path"] {
            let (status, body) =
                call(&app, "alpha", ADMIN_KEY, &session, tool, json!({ field: beta })).await;
            assert_eq!(status, StatusCode::OK, "{tool}");
            assert_eq!(body["error"]["code"], json!(-32602), "{tool} {field}: {body}");
            assert_eq!(
                body["error"]["message"],
                "Project and policy session are selected by the authenticated connection",
                "{tool} {field}"
            );
        }
    }
}

#[tokio::test]
async fn e6_sync_and_index_update_the_pinned_project() {
    let (dir, app) = fixture().await;
    let session = open(&app, "alpha", ADMIN_KEY).await;
    let alpha = dir.path().join("alpha");
    std::fs::write(alpha.join("added.rs"), "pub fn synced_remotely() -> u32 { 2 }\n").unwrap();
    let (status, body) = call(&app, "alpha", ADMIN_KEY, &session, "ax_sync", json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(body["result"]["isError"], json!(true), "{body}");
    let (_, found) = call(
        &app,
        "alpha",
        ADMIN_KEY,
        &session,
        "ax_search",
        json!({"query":"synced_remotely"}),
    )
    .await;
    assert!(text(&found).contains("synced_remotely"), "{found}");

    std::fs::write(alpha.join("rebuilt.rs"), "pub fn rebuilt_remotely() -> u32 { 3 }\n").unwrap();
    let (status, body) = call(
        &app,
        "alpha",
        ADMIN_KEY,
        &session,
        "ax_index",
        json!({"force":true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(body["result"]["isError"], json!(true), "{body}");
    let (_, found) = call(
        &app,
        "alpha",
        ADMIN_KEY,
        &session,
        "ax_search",
        json!({"query":"rebuilt_remotely"}),
    )
    .await;
    assert!(text(&found).contains("rebuilt_remotely"), "{found}");
    // The other project was never touched.
    assert!(!dir.path().join("beta/added.rs").exists());
    let beta_session = open(&app, "beta", BETA_KEY).await;
    let (_, beta) = call(
        &app,
        "beta",
        BETA_KEY,
        &beta_session,
        "ax_search",
        json!({"query":"rebuilt_remotely"}),
    )
    .await;
    assert!(!text(&beta).contains("rebuilt.rs"), "{beta}");
}

#[tokio::test]
async fn e6_two_concurrent_index_requests_both_finish_with_a_consistent_index() {
    let (_dir, app) = fixture().await;
    let first = open(&app, "alpha", ADMIN_KEY).await;
    let second = open(&app, "alpha", ADMIN_KEY).await;
    let (a, b) = tokio::join!(
        call(&app, "alpha", ADMIN_KEY, &first, "ax_index", json!({"force":true})),
        call(&app, "alpha", ADMIN_KEY, &second, "ax_index", json!({"force":true})),
    );
    for (status, body) in [a, b] {
        assert_eq!(status, StatusCode::OK);
        assert_ne!(body["result"]["isError"], json!(true), "{body}");
    }
    let (_, found) = call(
        &app,
        "alpha",
        ADMIN_KEY,
        &first,
        "ax_search",
        json!({"query":"remote_fixture"}),
    )
    .await;
    assert!(text(&found).contains("remote_fixture"), "{found}");
}

#[tokio::test]
async fn e7_logs_are_newest_first_paged_bounded_scoped_and_without_bodies() {
    let (_dir, app) = fixture().await;
    // Calls on beta must never show up in alpha's log.
    let beta_session = open(&app, "beta", BETA_KEY).await;
    call(&app, "beta", BETA_KEY, &beta_session, "ax_cycles", json!({})).await;
    let session = open(&app, "alpha", ADMIN_KEY).await;
    call(&app, "alpha", ADMIN_KEY, &session, "ax_status", json!({})).await;
    call(
        &app,
        "alpha",
        ADMIN_KEY,
        &session,
        "ax_search",
        json!({"query":"remote_fixture"}),
    )
    .await;
    // Call records are written by a background task.
    let mut entries = Vec::new();
    for _ in 0..100 {
        let (status, body) = call(&app, "alpha", ADMIN_KEY, &session, "ax_logs", json!({})).await;
        assert_eq!(status, StatusCode::OK);
        assert_ne!(body["result"]["isError"], json!(true), "{body}");
        entries = body["result"]["structuredContent"]["entries"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let tools: Vec<_> = entries.iter().filter_map(|e| e["tool"].as_str()).collect();
        if tools.contains(&"ax_status") && tools.contains(&"ax_search") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let tools: Vec<_> = entries.iter().filter_map(|e| e["tool"].as_str()).collect();
    assert!(tools.contains(&"ax_status") && tools.contains(&"ax_search"), "{tools:?}");
    assert!(!tools.contains(&"ax_cycles"), "beta call leaked: {tools:?}");
    let ids: Vec<i64> = entries.iter().map(|e| e["id"].as_i64().unwrap()).collect();
    let mut sorted = ids.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(ids, sorted, "newest first");
    for entry in &entries {
        let mut fields: Vec<_> = entry.as_object().unwrap().keys().cloned().collect();
        fields.sort();
        assert_eq!(
            fields,
            ["at", "durationMs", "id", "ok", "responseChars", "responseTokens", "tool", "transport"]
        );
    }
    // Paging: one entry, then the next older one.
    let (_, page) = call(&app, "alpha", ADMIN_KEY, &session, "ax_logs", json!({"limit":1})).await;
    let page = &page["result"]["structuredContent"];
    assert_eq!(page["entries"].as_array().unwrap().len(), 1);
    let newest = page["entries"][0]["id"].as_i64().unwrap();
    assert_eq!(page["nextBefore"], json!(newest));
    let (_, older) = call(
        &app,
        "alpha",
        ADMIN_KEY,
        &session,
        "ax_logs",
        json!({"limit":1,"before":newest}),
    )
    .await;
    assert!(older["result"]["structuredContent"]["entries"][0]["id"].as_i64().unwrap() < newest);
    // The limit is capped.
    let (_, capped) = call(
        &app,
        "alpha",
        ADMIN_KEY,
        &session,
        "ax_logs",
        json!({"limit":100000}),
    )
    .await;
    assert_eq!(capped["result"]["structuredContent"]["limit"], json!(500));
    let (_, found) = call(
        &app,
        "alpha",
        ADMIN_KEY,
        &session,
        "ax_logs",
        json!({"tool":"ax_search"}),
    )
    .await;
    let found = found["result"]["structuredContent"]["entries"].as_array().unwrap().clone();
    assert!(!found.is_empty() && found.iter().all(|e| e["tool"] == "ax_search"));
}

#[tokio::test]
async fn protected_resource_metadata_advertises_the_admin_scope() {
    let (_dir, app) = fixture().await;
    let response = app
        .oneshot(
            Request::get("/.well-known/oauth-protected-resource")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        body["scopes_supported"],
        json!(["ax:context", "ax:write", "ax:admin"])
    );
}
