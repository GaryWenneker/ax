//! The SPA fallback serves only files inside the web UI build folder.

use ax_web::WebHub;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use tower_http::cors::CorsLayer;

#[tokio::test]
async fn dot_dot_segments_cannot_leave_the_dist_folder() {
    let dir = tempfile::tempdir().unwrap();
    let dist = dir.path().join("dist");
    std::fs::create_dir_all(dist.join("assets")).unwrap();
    std::fs::write(dist.join("assets").join("app.js"), "console.log(1)").unwrap();
    std::fs::write(dir.path().join("secret.txt"), "top secret").unwrap();
    let root = dir.path().join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    // Only this test runs in this binary, so the process environment is ours.
    std::env::set_var("AX_HOME_DIR", dir.path());
    std::env::set_var("AX_GLOBAL_DB", dir.path().join("global.db"));
    std::env::set_var("AX_WEB_DIST", &dist);
    std::env::remove_var("AX_SHARE_TOKEN");
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let hub = WebHub::open(root, false, 7070).await.unwrap();
    let app = hub.nest_routers(CorsLayer::new());

    let get = |uri: &str| {
        Request::builder()
            .uri(uri)
            .header("host", "127.0.0.1:7070")
            .body(Body::empty())
            .unwrap()
    };

    let asset = app.clone().oneshot(get("/assets/app.js")).await.unwrap();
    assert_eq!(asset.status(), StatusCode::OK);

    for uri in [
        "/assets/../secret.txt",
        "/assets/../../secret.txt",
        "/../secret.txt",
        "/assets/..%2fsecret.txt",
    ] {
        let response = app.clone().oneshot(get(uri)).await.unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        assert!(
            !String::from_utf8_lossy(&body).contains("top secret"),
            "{uri} leaked a file outside the dist folder ({status})"
        );
    }
}
