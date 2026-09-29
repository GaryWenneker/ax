//! `/dav` sits behind the share token like `/api`. Separate binary: it sets `AX_SHARE_TOKEN`.

use ax_web::WebHub;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use tower_http::cors::CorsLayer;

#[tokio::test]
async fn dav_requires_share_token_when_sharing() {
    let home = tempfile::tempdir().unwrap();
    std::env::set_var("HOME", home.path());
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let hub = WebHub::open(root, true, 0).await.unwrap();
    let app = hub.nest_routers(CorsLayer::new());
    std::env::set_var("AX_SHARE_TOKEN", "s3cret");

    let status = |auth: Option<&'static str>| {
        let app = app.clone();
        async move {
            let mut req = Request::builder()
                .method("PROPFIND")
                .uri("/dav/")
                .header("Depth", "1");
            if let Some(a) = auth {
                req = req.header("Authorization", a);
            }
            app.oneshot(req.body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status()
        }
    };
    assert_ne!(status(None).await, StatusCode::MULTI_STATUS);
    assert_ne!(status(Some("Bearer wrong")).await, StatusCode::MULTI_STATUS);
    assert_eq!(
        status(Some("Bearer s3cret")).await,
        StatusCode::MULTI_STATUS
    );
}
