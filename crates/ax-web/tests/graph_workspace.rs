//! The Graph page shows the selected workspace index, not ~/.ax/global.db.

use std::path::Path;
use std::sync::OnceLock;

use ax_web::WebHub;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::Value;
use tempfile::TempDir;
use tower::ServiceExt;
use tower_http::cors::{Any, CorsLayer};

static HOME: OnceLock<TempDir> = OnceLock::new();

fn isolate() -> std::path::PathBuf {
    let dir = HOME.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", dir.path());
        std::env::set_var("AX_HOME_DIR", dir.path());
        std::env::set_var("AX_GLOBAL_DB", dir.path().join("global.db"));
        dir
    });
    dir.path().join("global.db")
}

async fn seed_node(root: &Path, id: &str, name: &str) {
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, updated_at)
         VALUES (?, 'function', ?, ?, 'src/lib.rs', 'rust', 1, 2, 0, 1, 0)",
    )
    .bind(id)
    .bind(name)
    .bind(name)
    .execute(db.pool())
    .await
    .unwrap();
    db.close().await;
}

async fn get(app: &Router, path: &str) -> (StatusCode, Vec<u8>) {
    let res = app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, bytes.to_vec())
}

#[tokio::test]
async fn graph_shows_only_the_selected_workspace_index() {
    let global = isolate();
    let dir = tempfile::tempdir().unwrap();
    let selected = dir.path().join("vanlanschot");
    let other = dir.path().join("other");
    seed_node(&selected, "local", "LocalOnly").await;
    seed_node(&other, "other", "OtherProject").await;

    let g = ax_global_db::open_and_init(&global).await.unwrap();
    ax_global_db::sync::sync_project(&g, &selected)
        .await
        .unwrap();
    ax_global_db::sync::sync_project(&g, &other).await.unwrap();
    g.close().await;

    let hub = WebHub::open(selected, true, 0).await.unwrap();
    let app = hub.nest_routers(
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any),
    );
    let (status, body) = get(&app, "/api/graph?limit=50").await;
    assert_eq!(status, StatusCode::OK);
    let json: Value = serde_json::from_slice(&body).unwrap();
    let names: Vec<&str> = json["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|n| n["name"].as_str())
        .collect();
    assert!(
        names.contains(&"LocalOnly"),
        "selected workspace node missing: {names:?}"
    );
    assert!(
        !names.iter().any(|n| *n == "OtherProject"),
        "graph included another workspace: {names:?}"
    );
    assert_eq!(json["total_nodes"], 1, "{json}");
    assert!(
        json.get("palette").is_none() || json["palette"].is_null(),
        "palette must not be the cross-project coloring: {json}"
    );
}
