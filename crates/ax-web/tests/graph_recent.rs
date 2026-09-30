//! `GET /api/graph/recent` (docs/specs/live-updates.md, revisions 2 and 3).

use std::path::PathBuf;
use std::sync::OnceLock;

use ax_web::WebHub;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tempfile::TempDir;
use tower::ServiceExt;
use tower_http::cors::CorsLayer;

static HOME: OnceLock<TempDir> = OnceLock::new();

fn isolate() {
    HOME.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", dir.path());
        std::env::set_var("AX_HOME_DIR", dir.path());
        std::env::set_var("AX_GLOBAL_DB", dir.path().join("global.db"));
        dir
    });
}

struct Project {
    _dir: TempDir,
    db: PathBuf,
    hub: WebHub,
}

async fn open_project(nodes: &[(&str, i64)]) -> Project {
    isolate();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = root.join(".ax").join("ax.db");
    ax_db::Database::open(&db).await.unwrap();
    let project = Project { hub: WebHub::open(root, false, 0).await.unwrap(), db, _dir: dir };
    write_nodes(&project, nodes, &[]).await;
    project
}

async fn write_nodes(p: &Project, nodes: &[(&str, i64)], edges: &[(&str, &str)]) {
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}", p.db.display()))
        .await
        .unwrap();
    for (id, updated_at) in nodes {
        sqlx::query(
            "INSERT OR REPLACE INTO nodes (id, kind, name, qualified_name, file_path, language, \
             start_line, end_line, start_column, end_column, updated_at) \
             VALUES (?, 'function', ?, ?, 'src/a.rs', 'rust', 1, 2, 0, 0, ?)",
        )
        .bind(id)
        .bind(id)
        .bind(id)
        .bind(updated_at)
        .execute(&pool)
        .await
        .unwrap();
    }
    for (source, target) in edges {
        sqlx::query("INSERT INTO edges (source, target, kind) VALUES (?, ?, 'calls')")
            .bind(source)
            .bind(target)
            .execute(&pool)
            .await
            .unwrap();
    }
    pool.close().await;
}

async fn get(p: &Project, uri: &str) -> (StatusCode, Value) {
    let res = p
        .hub
        .nest_routers(CorsLayer::new())
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn ids(body: &Value, key: &str, field: &str) -> Vec<String> {
    body[key]
        .as_array()
        .unwrap_or_else(|| panic!("no `{key}` array in {body}"))
        .iter()
        .map(|v| v[field].as_str().unwrap().to_string())
        .collect()
}

async fn load_graph(p: &Project) {
    assert_eq!(get(p, "/api/graph?limit=50").await.0, StatusCode::OK);
}

#[tokio::test]
async fn a_node_added_after_the_graph_loaded_is_returned_with_its_edges() {
    let p = open_project(&[("old", 1_000)]).await;
    load_graph(&p).await;
    write_nodes(&p, &[("new", 5_000)], &[("new", "old")]).await;
    let (status, body) = get(&p, "/api/graph/recent?since=0").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&body, "nodes", "id"), vec!["new"]);
    assert_eq!(ids(&body, "edges", "target"), vec!["old"]);
}

#[tokio::test]
async fn a_rewritten_existing_node_is_not_returned() {
    let p = open_project(&[("old", 1_000)]).await;
    load_graph(&p).await;
    write_nodes(&p, &[("old", 9_000)], &[]).await;
    let (_, body) = get(&p, "/api/graph/recent?since=0").await;
    assert_eq!(ids(&body, "nodes", "id"), Vec::<String>::new());
}

#[tokio::test]
async fn the_first_snapshot_marks_nothing_new() {
    let p = open_project(&[("a", 1_000), ("b", 2_000)]).await;
    let (status, body) = get(&p, "/api/graph/recent?since=0").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&body, "nodes", "id"), Vec::<String>::new());
}

#[tokio::test]
async fn limit_is_respected() {
    let p = open_project(&[]).await;
    load_graph(&p).await;
    write_nodes(&p, &[("a", 5_000), ("b", 5_001), ("c", 5_002), ("d", 5_003)], &[]).await;
    let (status, body) = get(&p, "/api/graph/recent?since=0&limit=2").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&body, "nodes", "id").len(), 2);
}

#[tokio::test]
async fn a_missing_or_invalid_since_is_rejected() {
    let p = open_project(&[]).await;
    assert_eq!(get(&p, "/api/graph/recent").await.0, StatusCode::BAD_REQUEST);
    assert_eq!(get(&p, "/api/graph/recent?since=abc").await.0, StatusCode::BAD_REQUEST);
}
