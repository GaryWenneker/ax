//! Remote boundary tests use real Ax projects and the HTTP router, without an external issuer.
use ax_mcp::remote::{
    auth::TokenVerifier,
    config::{ApiKey, OAuthConfig, Project, RemoteConfig},
    server::{bind, router, RemoteState},
};
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use jsonwebtoken::{encode, jwk::JwkSet, Algorithm, EncodingKey, Header};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tower::ServiceExt;

async fn fixture() -> (tempfile::TempDir, Router, Arc<RemoteConfig>) {
    let dir = tempfile::tempdir().unwrap();
    let mut projects = Vec::new();
    for id in ["alpha", "beta"] {
        let root = dir.path().join(id);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("lib.rs"),
            "pub fn remote_fixture() -> u32 { 1 }\n",
        )
        .unwrap();
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
        projects.push(Project {
            id: id.into(),
            root,
            subjects: vec![id.into()],
            write_subjects: vec![id.into()],
        });
    }
    let mut config = RemoteConfig {
        public_url: "https://ax.example.com".into(),
        oauth: OAuthConfig {
            issuer: "https://issuer.example.com/".into(),
            jwks_uri: "https://issuer.example.com/jwks".into(),
        },
        projects,
        api_keys: vec![ApiKey {
            id: "ide".into(),
            sha256: hex::encode(Sha256::digest(b"test-ide-key")),
            projects: vec!["alpha".into()],
            write: false,
        }],
        allowed_origins: vec![],
    };
    config.validate().unwrap();
    let config = Arc::new(config);
    let verifier = verifier(config.clone());
    let app = router(RemoteState::new(config.clone(), verifier));
    (dir, app, config)
}

fn verifier(config: Arc<RemoteConfig>) -> TokenVerifier {
    let keys: JwkSet =
        serde_json::from_str(include_str!("fixtures/remote-test-jwks.json")).unwrap();
    TokenVerifier::with_keys(config, keys, reqwest::Client::new())
}

fn claims(sub: &str) -> Value {
    json!({"sub":sub,"iss":"https://issuer.example.com/","aud":"https://ax.example.com","exp":ax_mcp::remote::credentials::now()+3600,"scope":"ax:context"})
}
fn token(claims: &Value) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("ax-test-key".into());
    encode(
        &header,
        claims,
        &EncodingKey::from_rsa_pem(include_bytes!("fixtures/remote-test-key.pem")).unwrap(),
    )
    .unwrap()
}

async fn post(
    app: &Router,
    project: &str,
    token: Option<&str>,
    session: Option<&str>,
    value: Value,
) -> (StatusCode, axum::http::HeaderMap, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri(format!("/projects/{project}/mcp"))
        .header("accept", "application/json, text/event-stream")
        .header("content-type", "application/json");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    if let Some(session) = session {
        request = request
            .header("mcp-session-id", session)
            .header("mcp-protocol-version", "2025-11-25");
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::from(value.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, headers, body)
}
async fn initialize(app: &Router, project: &str, token: &str) -> String {
    let (status, headers, body) = post(app, project, Some(token), None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["protocolVersion"], "2025-11-25");
    headers["mcp-session-id"].to_str().unwrap().to_owned()
}

#[tokio::test]
async fn remote_tokens_require_signature_issuer_audience_expiry_and_scope() {
    let (_dir, app, config) = fixture().await;
    let verify = verifier(config);
    assert!(verify.verify(&token(&claims("alpha"))).await.is_ok());
    for (field, value) in [
        ("iss", json!("https://wrong.example/")),
        ("aud", json!("wrong")),
        ("exp", json!(1)),
        ("sub", json!("")),
        ("nbf", json!(ax_mcp::remote::credentials::now() + 3600)),
    ] {
        let mut c = claims("alpha");
        c[field] = value;
        assert!(verify.verify(&token(&c)).await.is_err(), "{field}");
    }
    let bad = encode(
        &Header::new(Algorithm::HS256),
        &claims("alpha"),
        &EncodingKey::from_secret(b"attacker"),
    )
    .unwrap();
    assert!(verify.verify(&bad).await.is_err());
    let mut bad_signature = token(&claims("alpha"));
    let signature = bad_signature.rfind('.').unwrap() + 1;
    let replacement = if &bad_signature[signature..signature + 1] == "A" {
        "B"
    } else {
        "A"
    };
    bad_signature.replace_range(signature..signature + 1, replacement);
    assert!(verify.verify(&bad_signature).await.is_err());
    let mut c = claims("alpha");
    c["scope"] = json!("ax:write");
    assert_eq!(
        post(
            &app,
            "alpha",
            Some(&token(&c)),
            None,
            json!({"jsonrpc":"2.0","id":1,"method":"initialize"})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn remote_authentication_precedes_project_and_engine_access() {
    let (_dir, app, _) = fixture().await;
    let request = json!({"jsonrpc":"2.0","id":1,"method":"initialize"});
    let (status, headers, _) = post(&app, "alpha", None, None, request.clone()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(headers["www-authenticate"]
        .to_str()
        .unwrap()
        .contains("oauth-protected-resource"));
    assert_eq!(headers["cache-control"], "no-store");
    assert_eq!(
        post(&app, "beta", Some("test-ide-key"), None, request.clone())
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        post(&app, "missing", Some("test-ide-key"), None, request.clone())
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        post(
            &app,
            "alpha",
            Some(&token(&claims("ungranted"))),
            None,
            request
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn remote_transport_enforces_session_project_principal_and_lifecycle() {
    let (_dir, app, _) = fixture().await;
    let alpha = token(&claims("alpha"));
    let beta = token(&claims("beta"));
    let session = initialize(&app, "alpha", &alpha).await;
    let ping = json!({"jsonrpc":"2.0","id":2,"method":"ping"});
    assert_eq!(
        post(&app, "alpha", Some(&alpha), Some(&session), ping.clone())
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        post(&app, "beta", Some(&beta), Some(&session), ping.clone())
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        post(
            &app,
            "alpha",
            Some("test-ide-key"),
            Some(&session),
            ping.clone()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        post(&app, "alpha", Some(&alpha), Some("foreign"), ping.clone())
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        post(
            &app,
            "alpha",
            Some(&alpha),
            Some(&session),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )
        .await
        .0,
        StatusCode::ACCEPTED
    );
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/projects/alpha/mcp")
                .header("authorization", format!("Bearer {alpha}"))
                .header("mcp-session-id", &session)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        post(&app, "alpha", Some(&alpha), Some(&session), ping)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn remote_tools_deny_execution_writes_root_overrides_and_foreign_chats() {
    let (_dir, app, _) = fixture().await;
    let session = initialize(&app, "alpha", "test-ide-key").await;
    let call = |name: &str, arguments: Value| json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":name,"arguments":arguments}});
    for name in ["ax_index", "ax_sync", "ax_ship", "ax_lsp", "unknown"] {
        assert_eq!(
            post(
                &app,
                "alpha",
                Some("test-ide-key"),
                Some(&session),
                call(name, json!({}))
            )
            .await
            .0,
            StatusCode::FORBIDDEN,
            "{name}"
        );
    }
    for (name, args) in [
        ("ax_remember", json!({})),
        ("ax_policy_capture", json!({"action":"save"})),
    ] {
        let (status, _, body) = post(
            &app,
            "alpha",
            Some("test-ide-key"),
            Some(&session),
            call(name, args),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["result"]["isError"], true);
        assert!(body["result"]["_meta"]["mcp/www_authenticate"][0]
            .as_str()
            .unwrap()
            .contains("ax:write"));
    }
    let (_, _, body) = post(
        &app,
        "alpha",
        Some("test-ide-key"),
        Some(&session),
        call("ax_status", json!({"projectPath":"/tmp/other"})),
    )
    .await;
    assert_eq!(body["error"]["code"], -32602);
    assert_eq!(
        post(
            &app,
            "alpha",
            Some("test-ide-key"),
            Some(&session),
            call("ax_preflight", json!({"session":"foreign"}))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (_, _, list) = post(
        &app,
        "alpha",
        Some("test-ide-key"),
        Some(&session),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}),
    )
    .await;
    let tools = list["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().any(|t| t["name"] == "ax_preflight"));
    assert!(!tools
        .iter()
        .any(|t| t["name"] == "ax_sync" || t["name"] == "ax_remember"));
}

#[tokio::test]
async fn remote_chats_and_caches_are_connection_owned() {
    let (_dir, app, _) = fixture().await;
    let a = initialize(&app, "alpha", "test-ide-key").await;
    let b = initialize(&app, "alpha", "test-ide-key").await;
    let call = |name: &str, arguments: Value| json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":name,"arguments":arguments}});
    let (_, _, stash) = post(
        &app,
        "alpha",
        Some("test-ide-key"),
        Some(&a),
        call("ax_stash", json!({"text":"PRIVATE-REMOTE-A"})),
    )
    .await;
    assert!(!stash.to_string().contains("\"isError\":true"), "{stash}");
    let id = stash["result"]["structuredContent"]["id"].as_str().unwrap();
    let (_, _, expanded) = post(
        &app,
        "alpha",
        Some("test-ide-key"),
        Some(&a),
        call("ax_expand", json!({"id":id})),
    )
    .await;
    assert!(expanded.to_string().contains("PRIVATE-REMOTE-A"));
    assert_eq!(
        post(
            &app,
            "alpha",
            Some("test-ide-key"),
            Some(&b),
            call("ax_expand", json!({"id":id}))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    // Even an explicit guessed chat cannot select another connection's context.
    assert_eq!(
        post(
            &app,
            "alpha",
            Some("test-ide-key"),
            Some(&b),
            call("ax_session", json!({"session":"guess","action":"get"}))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn remote_dynamic_port_does_not_replace_an_occupied_listener() {
    let first = bind(0).await.unwrap();
    let port = first.local_addr().unwrap().port();
    assert_ne!(port, 0);
    assert!(bind(port).await.is_err());
    assert_eq!(first.local_addr().unwrap().port(), port);
    let second = bind(0).await.unwrap();
    assert_ne!(second.local_addr().unwrap().port(), port);
}

#[tokio::test]
async fn remote_http_wire_negotiates_initializes_lists_and_rejects_a_bad_version() {
    let (_dir, app, _) = fixture().await;
    let listener = bind(0).await.unwrap();
    let url = format!(
        "http://{}/projects/alpha/mcp",
        listener.local_addr().unwrap()
    );
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = reqwest::Client::new();
    let response = client.post(&url).bearer_auth("test-ide-key").header("accept","application/json, text/event-stream")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"unsupported"}})).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let session = response.headers()["mcp-session-id"]
        .to_str()
        .unwrap()
        .to_owned();
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["result"]["protocolVersion"], "2025-11-25");
    for (version, expected) in [
        ("2025-11-25", StatusCode::OK),
        ("unsupported", StatusCode::BAD_REQUEST),
    ] {
        let response = client
            .post(&url)
            .bearer_auth("test-ide-key")
            .header("accept", "application/json, text/event-stream")
            .header("mcp-session-id", &session)
            .header("mcp-protocol-version", version)
            .json(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
    }
    server.abort();
}

#[tokio::test]
async fn remote_writes_require_scope_and_the_explicit_project_grant() {
    let (_dir, _, config) = fixture().await;
    let verifier = verifier(config.clone());
    let read = verifier.verify(&token(&claims("alpha"))).await.unwrap();
    assert!(!read.can_write(&config.projects[0]));
    let mut c = claims("alpha");
    c["scope"] = json!("ax:context ax:write");
    let writer = verifier.verify(&token(&c)).await.unwrap();
    assert!(writer.can_write(&config.projects[0]));
    assert!(!writer.can_write(&config.projects[1]));
    let mut project = config.projects[0].clone();
    project.write_subjects.clear();
    assert!(!writer.can_write(&project));
    assert!(!verifier
        .verify("test-ide-key")
        .await
        .unwrap()
        .can_write(&project));
}

#[tokio::test]
async fn remote_discovery_advertises_oauth_and_rejects_invalid_identity_without_borrowing() {
    let (_dir, app, _) = fixture().await;
    let session = initialize(&app, "alpha", "test-ide-key").await;
    let (_, _, list) = post(
        &app,
        "alpha",
        Some("test-ide-key"),
        Some(&session),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    )
    .await;
    for tool in list["result"]["tools"].as_array().unwrap() {
        assert_eq!(tool["securitySchemes"][0]["type"], "oauth2");
        assert_eq!(tool["securitySchemes"][0]["scopes"], json!(["ax:context"]));
        assert!(tool["inputSchema"]["properties"].get("window_id").is_none());
    }
    for args in [
        json!({"session":4}),
        json!({"session":"bad<>"}),
        json!({"context_reset":"yes"}),
        json!({"sessionId":false}),
        json!({"session":"one","chatId":"two"}),
    ] {
        let (_, _, result) = post(&app, "alpha", Some("test-ide-key"), Some(&session),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ax_preflight","arguments":args}})).await;
        assert_eq!(result["error"]["code"], -32602, "{result}");
    }
}

#[tokio::test]
async fn remote_context_reset_rehydrates_policy_and_fork_keeps_connection_ownership() {
    let (_dir, app, config) = fixture().await;
    let rules = config.projects[0].root.join(".agents/rules");
    std::fs::create_dir_all(&rules).unwrap();
    std::fs::write(rules.join("remote-required.mdc"),
        "---\nid: remote-required\nlevel: CRITICAL\nalwaysApply: true\n---\nREMOTE_REQUIRED_CONSTRAINT\n").unwrap();
    let session = initialize(&app, "alpha", "test-ide-key").await;
    let call = |name: &str, args: Value| json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":name,"arguments":args}});
    let (_, _, first) = post(
        &app,
        "alpha",
        Some("test-ide-key"),
        Some(&session),
        call("ax_preflight", json!({})),
    )
    .await;
    assert!(
        first.to_string().contains("REMOTE_REQUIRED_CONSTRAINT"),
        "{first}"
    );
    let chat = first["result"]["structuredContent"]["session"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("missing preflight session: {first}"));
    let (_, _, second) = post(
        &app,
        "alpha",
        Some("test-ide-key"),
        Some(&session),
        call("ax_preflight", json!({"session":chat})),
    )
    .await;
    assert!(!second.to_string().contains("REMOTE_REQUIRED_CONSTRAINT"));
    let (_, _, reset) = post(
        &app,
        "alpha",
        Some("test-ide-key"),
        Some(&session),
        call("ax_preflight", json!({"session":chat,"context_reset":true})),
    )
    .await;
    assert!(reset.to_string().contains("REMOTE_REQUIRED_CONSTRAINT"));
    let (_, _, added) = post(
        &app,
        "alpha",
        Some("test-ide-key"),
        Some(&session),
        call(
            "ax_session",
            json!({"session":chat,"action":"add","objective":"Continue remote context work"}),
        ),
    )
    .await;
    assert_ne!(added["result"]["isError"], true, "{added}");
    let (_, _, fork) = post(
        &app,
        "alpha",
        Some("test-ide-key"),
        Some(&session),
        call("ax_session", json!({"session":chat,"action":"fork"})),
    )
    .await;
    let child = fork["result"]["structuredContent"]["session"]
        .as_str()
        .unwrap_or_else(|| panic!("missing fork session: {fork}"));
    assert_eq!(
        post(
            &app,
            "alpha",
            Some("test-ide-key"),
            Some(&session),
            call("ax_preflight", json!({"session":child}))
        )
        .await
        .0,
        StatusCode::OK
    );
    let other = initialize(&app, "alpha", "test-ide-key").await;
    assert_eq!(
        post(
            &app,
            "alpha",
            Some("test-ide-key"),
            Some(&other),
            call("ax_preflight", json!({"session":child}))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn remote_durable_branches_register_only_server_issued_child_ids() {
    let (_dir, app, _) = fixture().await;
    let transport = initialize(&app, "alpha", "test-ide-key").await;
    let call = |args: Value| json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"ax_durable","arguments":args}});
    let (_, _, appended) = post(
        &app,
        "alpha",
        Some("test-ide-key"),
        Some(&transport),
        call(json!({"action":"append", "role":"user", "body":"remote durable note"})),
    )
    .await;
    assert_ne!(appended["result"]["isError"], true, "{appended}");
    for action in ["fork", "handoff"] {
        let (_, _, branch) = post(
            &app,
            "alpha",
            Some("test-ide-key"),
            Some(&transport),
            call(json!({"action":action,"note":"continue this project"})),
        )
        .await;
        let child = branch["result"]["structuredContent"]["session"]
            .as_str()
            .unwrap_or_else(|| panic!("missing durable child: {branch}"));
        let (_, _, read) = post(
            &app,
            "alpha",
            Some("test-ide-key"),
            Some(&transport),
            call(json!({"action":"read","session":child})),
        )
        .await;
        assert_ne!(read["result"]["isError"], true, "{read}");
        let other = initialize(&app, "alpha", "test-ide-key").await;
        assert_eq!(
            post(
                &app,
                "alpha",
                Some("test-ide-key"),
                Some(&other),
                call(json!({"action":"read","session":child}))
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }
}

#[tokio::test]
async fn remote_authorized_writer_can_remember_but_cannot_save_global_policy() {
    let (_dir, app, _) = fixture().await;
    let mut c = claims("alpha");
    c["scope"] = json!("ax:context ax:write");
    let writer = token(&c);
    let transport = initialize(&app, "alpha", &writer).await;
    let call = |name: &str, args: Value| json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":name,"arguments":args}});
    let (_, _, saved) = post(
        &app,
        "alpha",
        Some(&writer),
        Some(&transport),
        call(
            "ax_remember",
            json!({"body":"REMOTE_WRITER_NOTE", "kind":"note"}),
        ),
    )
    .await;
    assert_ne!(saved["result"]["isError"], true, "{saved}");
    assert!(
        saved["result"]["structuredContent"]["id"].is_string(),
        "{saved}"
    );
    let (_, _, tools) = post(
        &app,
        "alpha",
        Some(&writer),
        Some(&transport),
        json!({"jsonrpc":"2.0","id":7,"method":"tools/list"}),
    )
    .await;
    let remember = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "ax_remember")
        .unwrap();
    assert_eq!(
        remember["securitySchemes"][0]["scopes"],
        json!(["ax:context", "ax:write"])
    );
    for frontmatter in [
        json!({"scope":"global"}),
        json!({"scope":"project","rootId":"other"}),
    ] {
        assert_eq!(post(&app, "alpha", Some(&writer), Some(&transport),
            call("ax_policy_capture",json!({"action":"save","rule":{"frontmatter":frontmatter,"body":"foreign policy"}}))).await.0, StatusCode::FORBIDDEN);
    }
}
