//! A writable Command Center beyond loopback needs a login or a share token.
//! Separate process: this test sets environment variables.

use std::time::Duration;

use ax_web::ServeOptions;

#[tokio::test]
async fn an_anonymous_lan_bind_refuses_to_start() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    std::env::set_var("AX_HOME_DIR", dir.path());
    std::env::set_var("AX_GLOBAL_DB", dir.path().join("global.db"));
    std::env::remove_var("AX_SHARE_TOKEN");
    std::env::remove_var("AX_WEB_READONLY");
    std::env::remove_var("AX_WEB_ALLOW_ANONYMOUS_LAN");
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let serve = ax_web::serve_with(ServeOptions {
        root,
        port,
        bind: "0.0.0.0".into(),
        ..ServeOptions::default()
    });
    let outcome = tokio::time::timeout(Duration::from_secs(20), serve).await;
    let error = match outcome {
        Ok(Err(error)) => error,
        Ok(Ok(())) => panic!("server started and stopped"),
        Err(_) => panic!("server started on 0.0.0.0 without a login or share token"),
    };
    assert!(error.contains("--allow-anonymous-lan"), "{error}");
}
