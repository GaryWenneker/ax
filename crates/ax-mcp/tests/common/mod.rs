//! Shared fixtures for the remote MCP integration tests: indexed projects, the
//! remote configuration, in-memory token signing material and the HTTP helper.
#![allow(dead_code)]

use ax_mcp::remote::config::{ApiKey, OAuthConfig, Project, RemoteConfig};
use axum::{
    body::{to_bytes, Body},
    http::{HeaderMap, Request, StatusCode},
    Router,
};
use jsonwebtoken::{encode, jwk::JwkSet, Algorithm, EncodingKey, Header};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tower::ServiceExt;

pub const ISSUER: &str = "https://issuer.example.com/";
pub const AUDIENCE: &str = "https://ax.example.com";
pub const KEY_ID: &str = "ax-test-key";

/// Points the usage database and the ax home at a temporary directory for the
/// whole test process, so no test writes the developer's real data. Runs once.
pub fn isolate_process() -> &'static Path {
    static BASE: OnceLock<PathBuf> = OnceLock::new();
    BASE.get_or_init(|| {
        let base = tempfile::tempdir().unwrap().keep();
        let home = base.join("home");
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("AX_HOME_DIR", &home);
        std::env::set_var("AX_USAGE_DB", base.join("usage.db"));
        std::env::remove_var("AX_MCP_TOOLS");
        base
    })
}

/// An API key for one project whose secret is `secret`.
pub fn api_key(id: &str, secret: &str, project: &str, write: bool, admin: bool) -> ApiKey {
    ApiKey {
        id: id.into(),
        sha256: hex::encode(Sha256::digest(secret.as_bytes())),
        projects: vec![project.into()],
        write,
        admin,
    }
}

/// An indexed one-file project `<dir>/<id>` that subject `id` may read and write.
pub async fn indexed_project(dir: &Path, id: &str) -> Project {
    let root = dir.join(id);
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
    Project {
        id: id.into(),
        root,
        subjects: vec![id.into()],
        write_subjects: vec![id.into()],
        admin_subjects: vec![],
    }
}

/// A remote configuration with the test issuer and audience. Not validated.
pub fn remote_config(projects: Vec<Project>, api_keys: Vec<ApiKey>) -> RemoteConfig {
    RemoteConfig {
        public_url: AUDIENCE.into(),
        oauth: OAuthConfig {
            built_in: false,
            issuer: ISSUER.into(),
            jwks_uri: format!("{ISSUER}jwks"),
            clock_skew_seconds: ax_mcp::remote::config::DEFAULT_CLOCK_SKEW_SECONDS,
        },
        projects,
        api_keys,
        allowed_origins: vec![],
    }
}

pub struct Signing {
    pub pem: String,
    pub jwks: JwkSet,
}

/// An RSA key generated in memory once per test binary, plus its public JWKS.
pub fn signing() -> &'static Signing {
    static SIGNING: OnceLock<Signing> = OnceLock::new();
    SIGNING.get_or_init(|| {
        use base64::Engine;
        // The rsa crate has no patched release for RUSTSEC-2023-0071.
        let key = openssl::rsa::Rsa::generate(2048).expect("test key");
        let b64 = |n: Vec<u8>| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(n);
        let (n, e) = (b64(key.n().to_vec()), b64(key.e().to_vec()));
        let pem = String::from_utf8(
            openssl::pkey::PKey::from_rsa(key)
                .expect("pkey")
                .private_key_to_pem_pkcs8()
                .expect("pem"),
        )
        .expect("pem utf8");
        let jwks = serde_json::from_value(json!({
            "keys": [{ "kty": "RSA", "alg": "RS256", "use": "sig", "kid": KEY_ID, "n": n, "e": e }]
        }))
        .expect("jwks");
        Signing { pem, jwks }
    })
}

/// A token signed with [`signing`] carrying exactly `claims`.
pub fn sign(claims: &Value) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(KEY_ID.into());
    encode(
        &header,
        claims,
        &EncodingKey::from_rsa_pem(signing().pem.as_bytes()).unwrap(),
    )
    .unwrap()
}

/// Valid claims for `subject` with `scope`, expiring in an hour.
pub fn claims(subject: &str, scope: &str) -> Value {
    json!({
        "sub": subject,
        "iss": ISSUER,
        "aud": AUDIENCE,
        "exp": ax_mcp::remote::credentials::now() + 3600,
        "scope": scope,
    })
}

/// POSTs `body` to the project's MCP endpoint. An empty body reads as
/// `Value::Null`, a body that is not JSON as a `Value::String`.
pub async fn post(
    app: &Router,
    project: &str,
    bearer: Option<&str>,
    session: Option<&str>,
    body: Value,
) -> (StatusCode, HeaderMap, Value) {
    let mut request = Request::post(format!("/projects/{project}/mcp"))
        .header("accept", "application/json, text/event-stream")
        .header("content-type", "application/json");
    if let Some(bearer) = bearer {
        request = request.header("authorization", format!("Bearer {bearer}"));
    }
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
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    };
    (status, headers, value)
}

/// The `initialize` request every session starts with.
pub fn initialize_request(client: &str) -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":client,"version":"1"}}})
}
