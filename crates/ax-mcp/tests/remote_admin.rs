//! Admin extension of the remote MCP server (docs/specs/ax-admin/SPEC.md, E1-E7).
//! Real Ax projects behind the real HTTP router; API keys only.
mod common;

use ax_mcp::remote::{
    auth::TokenVerifier,
    config::{ApiKey, Project, RemoteConfig},
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
use std::time::Duration;
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
    common::isolate_process();
}

fn key(id: &str, secret: &str, project: &str, write: bool, admin: bool) -> ApiKey {
    common::api_key(id, secret, project, write, admin)
}

async fn project(dir: &std::path::Path, id: &str) -> Project {
    common::indexed_project(dir, id).await
}

fn base_config(projects: Vec<Project>, api_keys: Vec<ApiKey>) -> RemoteConfig {
    common::remote_config(projects, api_keys)
}

async fn fixture() -> (tempfile::TempDir, Router) {
    fixture_with(|state| state).await
}

/// An access token from the external provider for `subject` with `scope`.
fn jwt(subject: &str, scope: &str) -> String {
    common::sign(&common::claims(subject, scope))
}

/// Two projects. alpha's subject "alpha" is an admin subject; "ann" may only
/// write. beta has no admin subject.
async fn fixture_with(
    adjust: impl FnOnce(RemoteState) -> RemoteState,
) -> (tempfile::TempDir, Router) {
    isolate_usage_db();
    let dir = tempfile::tempdir().unwrap();
    let mut alpha = project(dir.path(), "alpha").await;
    alpha.subjects.push("ann".into());
    alpha.write_subjects.push("ann".into());
    alpha.admin_subjects = vec!["alpha".into()];
    let projects = vec![alpha, project(dir.path(), "beta").await];
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
    let verifier = TokenVerifier::with_keys(
        config.clone(),
        common::signing().jwks.clone(),
        reqwest::Client::new(),
    );
    (dir, router(adjust(RemoteState::new(config, verifier))))
}

async fn send(
    app: &Router,
    project: &str,
    secret: &str,
    session: Option<&str>,
    body: Value,
) -> (StatusCode, Option<String>, Value) {
    let (status, headers, value) = common::post(app, project, Some(secret), session, body).await;
    let session = headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    (status, session, value)
}

async fn open(app: &Router, project: &str, secret: &str) -> String {
    let (status, session, _) = send(
        app,
        project,
        secret,
        None,
        common::initialize_request("admin-test"),
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
        assert!(
            list.contains(&tool.to_string()),
            "{tool} missing from {list:?}"
        );
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
        assert!(tool
            .pointer("/inputSchema/properties/projectPath")
            .is_none());
    }
    let read_only = |name: &str| {
        tools.iter().find(|t| t["name"] == name).unwrap()["annotations"]["readOnlyHint"].clone()
    };
    assert_eq!(read_only("ax_logs"), json!(true));
    assert_eq!(read_only("ax_files"), json!(true));
    assert_eq!(read_only("ax_index"), json!(false));
    assert_eq!(read_only("ax_sync"), json!(false));
    let destructive = |name: &str| {
        tools.iter().find(|t| t["name"] == name).unwrap()["annotations"]["destructiveHint"].clone()
    };
    // ax_remember action=purge deletes a memory for good.
    assert_eq!(destructive("ax_remember"), json!(true));
    assert_eq!(destructive("ax_logs"), json!(false));
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
    // Only the field names are checked: the project root is a temporary path that may
    // itself contain "admin" (the Windows CI account is "runneradmin").
    let saved = serde_json::to_value(&config).unwrap();
    assert!(
        saved["projects"][0].get("admin_subjects").is_none(),
        "{saved}"
    );
    assert!(saved["api_keys"][0].get("admin").is_none(), "{saved}");
}

#[tokio::test]
async fn e5_admin_calls_cannot_select_another_project_root() {
    let (dir, app) = fixture().await;
    let session = open(&app, "alpha", ADMIN_KEY).await;
    let beta = dir.path().join("beta");
    for tool in ["ax_index", "ax_sync", "ax_files", "ax_logs"] {
        for field in ["projectPath", "project_path"] {
            let (status, body) = call(
                &app,
                "alpha",
                ADMIN_KEY,
                &session,
                tool,
                json!({ field: beta }),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{tool}");
            assert_eq!(
                body["error"]["code"],
                json!(-32602),
                "{tool} {field}: {body}"
            );
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
    std::fs::write(
        alpha.join("added.rs"),
        "pub fn synced_remotely() -> u32 { 2 }\n",
    )
    .unwrap();
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

    std::fs::write(
        alpha.join("rebuilt.rs"),
        "pub fn rebuilt_remotely() -> u32 { 3 }\n",
    )
    .unwrap();
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
        call(
            &app,
            "alpha",
            ADMIN_KEY,
            &first,
            "ax_index",
            json!({"force":true})
        ),
        call(
            &app,
            "alpha",
            ADMIN_KEY,
            &second,
            "ax_index",
            json!({"force":true})
        ),
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
    call(
        &app,
        "beta",
        BETA_KEY,
        &beta_session,
        "ax_cycles",
        json!({}),
    )
    .await;
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
    assert!(
        tools.contains(&"ax_status") && tools.contains(&"ax_search"),
        "{tools:?}"
    );
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
            [
                "at",
                "durationMs",
                "id",
                "ok",
                "responseChars",
                "responseTokens",
                "tool",
                "transport"
            ]
        );
    }
    // Paging: one entry, then the next older one.
    let (_, page) = call(
        &app,
        "alpha",
        ADMIN_KEY,
        &session,
        "ax_logs",
        json!({"limit":1}),
    )
    .await;
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
    assert!(
        older["result"]["structuredContent"]["entries"][0]["id"]
            .as_i64()
            .unwrap()
            < newest
    );
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
    let found = found["result"]["structuredContent"]["entries"]
        .as_array()
        .unwrap()
        .clone();
    assert!(!found.is_empty() && found.iter().all(|e| e["tool"] == "ax_search"));
}

async fn advertised_scopes(app: Router) -> Value {
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
    body["scopes_supported"].clone()
}

#[tokio::test]
async fn the_admin_scope_is_advertised_only_where_a_token_can_carry_it() {
    // An external provider with an admin subject.
    let (_dir, app) = fixture().await;
    assert_eq!(
        advertised_scopes(app).await,
        json!(["ax:context", "ax:write", "ax:admin"])
    );
    // No admin subject (admin API keys need no OAuth scope): unchanged metadata.
    isolate_usage_db();
    let dir = tempfile::tempdir().unwrap();
    let alpha = project(dir.path(), "alpha").await;
    let mut config = base_config(
        vec![alpha],
        vec![key("adm", ADMIN_KEY, "alpha", true, true)],
    );
    config.validate().unwrap();
    let config = Arc::new(config);
    let verifier = TokenVerifier::with_keys(
        config.clone(),
        JwkSet { keys: vec![] },
        reqwest::Client::new(),
    );
    let app = router(RemoteState::new(config, verifier));
    assert_eq!(
        advertised_scopes(app).await,
        json!(["ax:context", "ax:write"])
    );
}

#[tokio::test]
async fn e1_the_non_admin_tool_list_is_the_admin_list_without_the_admin_tools() {
    let (_dir, app) = fixture().await;
    let list = |secret: &'static str| {
        let app = app.clone();
        async move {
            let session = open(&app, "alpha", secret).await;
            let (_, _, body) = send(
                &app,
                "alpha",
                secret,
                Some(&session),
                json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            )
            .await;
            body["result"]["tools"].as_array().unwrap().clone()
        }
    };
    let write = list(WRITE_KEY).await;
    let admin: Vec<Value> = list(ADMIN_KEY)
        .await
        .into_iter()
        .filter(|tool| !ADMIN_TOOLS.contains(&tool["name"].as_str().unwrap()))
        .collect();
    // Same definitions, schemas, annotations and order: the grant only adds.
    assert_eq!(write, admin);
}

#[tokio::test]
async fn e2_an_admin_subject_needs_the_scope_and_the_grant() {
    let (_dir, app) = fixture().await;
    let all = "ax:context ax:write ax:admin";
    let admin = tool_names(&app, "alpha", &jwt("alpha", all)).await;
    for tool in ADMIN_TOOLS {
        assert!(admin.contains(&tool.to_string()), "{tool}");
    }
    let session = open(&app, "alpha", &jwt("alpha", all)).await;
    let (status, body) = call(
        &app,
        "alpha",
        &jwt("alpha", all),
        &session,
        "ax_files",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(text(&body).contains("lib.rs"), "{body}");

    // The admin subject without the scope in the token.
    let without_scope = jwt("alpha", "ax:context ax:write");
    // A write subject that is not an admin subject, with the scope.
    let not_granted = jwt("ann", all);
    for token in [&without_scope, &not_granted] {
        assert_eq!(tool_names(&app, "alpha", token).await, RECORDED_WRITE_LIST);
        let session = open(&app, "alpha", token).await;
        for tool in ADMIN_TOOLS {
            let (status, _) = call(&app, "alpha", token, &session, tool, json!({})).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{tool}");
        }
    }
    // beta has no admin subject at all.
    let beta = jwt("beta", all);
    assert_eq!(tool_names(&app, "beta", &beta).await, RECORDED_WRITE_LIST);
}

#[tokio::test]
async fn policy_reload_is_the_only_policy_action_available_remotely() {
    let (_dir, app) = fixture().await;
    let session = open(&app, "alpha", ADMIN_KEY).await;
    for action in ["restore", "history", "audit", "anything"] {
        let (status, body) = call(
            &app,
            "alpha",
            ADMIN_KEY,
            &session,
            "ax_policy_index",
            json!({"action": action, "id": "english-only"}),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{action}");
        assert_eq!(body["error"], "Only action=index is available remotely");
    }
    let (status, body) = call(
        &app,
        "alpha",
        ADMIN_KEY,
        &session,
        "ax_policy_index",
        json!({"action": 7}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    for arguments in [json!({}), json!({"action": "index"})] {
        let (status, body) = call(
            &app,
            "alpha",
            ADMIN_KEY,
            &session,
            "ax_policy_index",
            arguments,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_ne!(body["result"]["isError"], json!(true), "{body}");
    }
    // Discovery shows the same restriction.
    let (_, _, body) = send(
        &app,
        "alpha",
        ADMIN_KEY,
        Some(&session),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    )
    .await;
    let tool = body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "ax_policy_index")
        .unwrap()
        .clone();
    assert_eq!(
        tool["inputSchema"]["properties"]["action"]["enum"],
        json!(["index"])
    );
    assert!(tool["inputSchema"]["properties"].get("id").is_none());
}

#[tokio::test]
async fn an_index_run_finishes_after_its_request_timed_out() {
    let (dir, app) =
        fixture_with(|state| state.with_admin_limits(Duration::from_millis(1), 4)).await;
    let session = open(&app, "alpha", ADMIN_KEY).await;
    std::fs::write(
        dir.path().join("alpha/late.rs"),
        "pub fn indexed_after_timeout() -> u32 { 9 }\n",
    )
    .unwrap();
    let (status, body) = call(
        &app,
        "alpha",
        ADMIN_KEY,
        &session,
        "ax_index",
        json!({"force":true}),
    )
    .await;
    assert_eq!(status, StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(
        body["error"],
        "The operation is still running on the server"
    );
    assert_eq!(body["running"], true);
    // The run was not dropped: the rebuilt index appears, on a fresh session too.
    let other = open(&app, "alpha", CONTEXT_KEY).await;
    let mut found = String::new();
    for _ in 0..200 {
        let (status, body) = call(
            &app,
            "alpha",
            CONTEXT_KEY,
            &other,
            "ax_search",
            json!({"query":"indexed_after_timeout"}),
        )
        .await;
        // While the index still runs the project is busy; afterwards it answers.
        assert!(
            status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE,
            "{status} {body}"
        );
        found = text(&body);
        if found.contains("late.rs") && found.contains("indexed_after_timeout") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(found.contains("late.rs"), "{found}");
    // The run released the project's file lock: another writer gets it at once.
    let mut other_writer = ax_core::Ax::init(&dir.path().join("alpha")).await.unwrap();
    let synced = tokio::time::timeout(
        Duration::from_secs(20),
        other_writer.sync(ax_extraction::orchestrator::IndexOptions::default(), None),
    )
    .await
    .expect("the file lock is free");
    assert!(synced.is_ok(), "{:?}", synced.err());
}

#[tokio::test]
async fn calls_do_not_queue_behind_a_running_index() {
    let handle = Arc::new(std::sync::Mutex::new(None));
    let keep = handle.clone();
    let (_dir, app) = fixture_with(move |state| {
        *keep.lock().unwrap() = Some(state.clone());
        state
    })
    .await;
    let state: RemoteState = handle.lock().unwrap().take().unwrap();
    let session = open(&app, "alpha", CONTEXT_KEY).await;
    let beta_session = open(&app, "beta", BETA_KEY).await;

    // alpha is being re-indexed.
    let indexing = state.hold_project("alpha").await;
    let started = std::time::Instant::now();
    let (status, body) = call(&app, "alpha", CONTEXT_KEY, &session, "ax_status", json!({})).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        body["error"],
        "This project is being re-indexed; try again when it is done"
    );
    assert_eq!(body["reindexing"], true);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "told quickly, not after minutes"
    );
    // Listing tools needs no index, and the other project is not affected.
    let (status, _, _) = send(
        &app,
        "alpha",
        CONTEXT_KEY,
        Some(&session),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(
        &app,
        "beta",
        BETA_KEY,
        &beta_session,
        "ax_status",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    drop(indexing);
    let (status, _) = call(&app, "alpha", CONTEXT_KEY, &session, "ax_status", json!({})).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn index_changing_calls_are_limited_and_reads_are_not() {
    let (_dir, app) =
        fixture_with(|state| state.with_admin_limits(Duration::from_secs(60), 0)).await;
    let session = open(&app, "alpha", ADMIN_KEY).await;
    for (tool, arguments) in [
        ("ax_sync", json!({})),
        ("ax_index", json!({"force":true})),
        ("ax_policy_index", json!({})),
    ] {
        let (status, body) = call(&app, "alpha", ADMIN_KEY, &session, tool, arguments).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{tool}");
        assert_eq!(
            body["error"],
            "Too many index operations are running; try again later"
        );
    }
    for tool in ["ax_files", "ax_logs", "ax_status"] {
        let (status, body) = call(&app, "alpha", ADMIN_KEY, &session, tool, json!({})).await;
        assert_eq!(status, StatusCode::OK, "{tool}");
        assert_ne!(body["result"]["isError"], json!(true), "{tool}: {body}");
    }
}
