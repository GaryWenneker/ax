//! Startup failures of the remote MCP server leave nothing behind.
//!
//! This file holds a single test because it sets process environment variables
//! (the global database path and the host control credential).
use ax_mcp::remote::{
    config::{OAuthConfig, Project, RemoteConfig},
    server::serve_on,
};

#[tokio::test]
async fn an_invalid_host_control_credential_fails_before_any_lease_or_socket_exists() {
    let dir = tempfile::tempdir().unwrap();
    let global = dir.path().join("global.db");
    std::env::set_var("AX_GLOBAL_DB", &global);
    std::env::set_var("AX_REMOTE_HOST_CONTROL", "too-short");
    let root = dir.path().join("project");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    std::fs::write(root.join(".ax/ax.db"), "").unwrap();
    let config = RemoteConfig {
        public_url: "https://ax.example.com".into(),
        oauth: OAuthConfig {
            built_in: true,
            issuer: "https://ax.example.com/oauth".into(),
            jwks_uri: "https://ax.example.com/oauth/jwks".into(),
            clock_skew_seconds: ax_mcp::remote::config::DEFAULT_CLOCK_SKEW_SECONDS,
        },
        projects: vec![Project {
            id: "demo".into(),
            root,
            subjects: vec!["owner".into()],
            write_subjects: vec![],
            admin_subjects: vec![],
        }],
        api_keys: vec![],
        allowed_origins: vec![],
    };
    let socket = dir.path().join("remote.sock");
    let error = serve_on(
        config,
        std::net::Ipv4Addr::LOCALHOST.into(),
        0,
        Some(socket.clone()),
    )
    .await
    .unwrap_err();
    assert_eq!(error, "Invalid host control credential");
    assert!(!socket.exists(), "the socket file was left behind");
    if global.exists() {
        let pool = ax_global_db::open_and_init(&global).await.unwrap();
        let running: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM remote_mcp_runtime_leases WHERE stopped_at IS NULL",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(running, 0, "the publication lease is still marked running");
    }
}
