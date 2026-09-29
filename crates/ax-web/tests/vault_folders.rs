//! Vault folders: config API (C1–C4) and the `/folders/` DAV area (D1–D7) from docs/specs/vault-folders.md.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use ax_web::WebHub;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tempfile::TempDir;
use tower::ServiceExt;
use tower_http::cors::{Any, CorsLayer};

static HOME: OnceLock<TempDir> = OnceLock::new();

fn isolate_home() {
    HOME.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", dir.path());
        std::env::set_var("AX_HOME_DIR", dir.path());
        std::env::set_var("AX_GLOBAL_DB", dir.path().join("global.db"));
        dir
    });
}

struct Fx {
    dir: TempDir,
    root: PathBuf,
    app: Router,
    hub: WebHub,
}

impl Fx {
    fn notes(&self) -> PathBuf {
        self.dir.path().join("notes").canonicalize().unwrap()
    }
}

async fn fixture(readonly: bool) -> Fx {
    isolate_home();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    std::fs::create_dir_all(dir.path().join("notes/sub")).unwrap();
    std::fs::write(dir.path().join("notes/a.md"), "# Alpha\n\nalpha body").unwrap();
    std::fs::write(dir.path().join("notes/sub/b.txt"), "bravo").unwrap();
    std::fs::write(dir.path().join("secret.md"), "outside").unwrap();
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let hub = WebHub::open(root.clone(), readonly, 0).await.unwrap();
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);
    let app = hub.nest_routers(cors);
    Fx {
        dir,
        root,
        app,
        hub,
    }
}

async fn send(
    app: &Router,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> (StatusCode, String) {
    let mut req = Request::builder().method(method).uri(path);
    if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("host")) {
        req = req.header("host", "127.0.0.1:7070");
    }
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let res = app
        .clone()
        .oneshot(req.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

async fn put_folders(app: &Router, list: Value) -> (StatusCode, Value) {
    let (status, body) = send(
        app,
        "PUT",
        "/api/vault/folders",
        &[("content-type", "application/json")],
        &list.to_string(),
    )
    .await;
    (status, serde_json::from_str(&body).unwrap_or(Value::Null))
}

async fn configure(fx: &Fx, index: bool) {
    let (status, body) = put_folders(
        &fx.app,
        json!([{ "name": "notes", "path": fx.dir.path().join("notes"), "index": index }]),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

async fn doc_count(fx: &Fx) -> i64 {
    let pool = fx.hub.read().await.graph_pool.clone();
    sqlx::query_scalar("SELECT COUNT(*) FROM memories WHERE kind = 'doc'")
        .fetch_one(&pool)
        .await
        .unwrap()
}

fn config_file(root: &Path) -> PathBuf {
    root.join(".ax").join("vault-folders.json")
}

#[tokio::test]
async fn c1_list_round_trips_and_starts_empty() {
    let fx = fixture(false).await;
    let (status, body) = send(&fx.app, "GET", "/api/vault/folders", &[], "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["folders"],
        json!([])
    );

    configure(&fx, false).await;
    let (_, body) = send(&fx.app, "GET", "/api/vault/folders", &[], "").await;
    let got: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(got["folders"][0]["name"], "notes");
    assert_eq!(got["folders"][0]["index"], false);
    assert_eq!(got["folders"][0]["path"], json!(fx.notes()));
    assert!(config_file(&fx.root).is_file());
}

#[tokio::test]
async fn c2_bad_paths_are_rejected() {
    let fx = fixture(false).await;
    for path in [
        json!("relative/notes"),
        json!("src"),
        json!(fx.dir.path().join("missing")),
        json!(fx.dir.path().join("secret.md")),
    ] {
        let (status, body) = put_folders(&fx.app, json!([{ "name": "x", "path": path }])).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}");
        assert!(
            body["error"].as_str().is_some_and(|e| !e.is_empty()),
            "{body}"
        );
    }
    assert!(!config_file(&fx.root).exists());
}

#[tokio::test]
async fn c3_bad_or_duplicate_names_are_rejected() {
    let fx = fixture(false).await;
    let notes = fx.dir.path().join("notes");
    for list in [
        json!([{ "name": "a/b", "path": notes }]),
        json!([{ "name": "", "path": notes }]),
        json!([{ "name": "n", "path": notes }, { "name": "n", "path": notes }]),
    ] {
        let (status, _) = put_folders(&fx.app, list.clone()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{list}");
    }
}

#[tokio::test]
async fn c4_sync_delete_and_readonly() {
    let fx = fixture(false).await;
    configure(&fx, true).await;
    let (status, body) = send(&fx.app, "POST", "/api/vault/folders/notes/sync", &[], "").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let report: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        report["report"],
        json!({ "added": 0, "updated": 0, "removed": 0, "skipped": 0 })
    );
    assert_eq!(doc_count(&fx).await, 2);

    let (status, _) = send(&fx.app, "POST", "/api/vault/folders/nope/sync", &[], "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = send(&fx.app, "DELETE", "/api/vault/folders/notes", &[], "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(doc_count(&fx).await, 0);
    let (_, body) = send(&fx.app, "GET", "/api/vault/folders", &[], "").await;
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["folders"],
        json!([])
    );

    let ro = fixture(true).await;
    let (status, _) = put_folders(&ro.app, json!([])).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(&ro.app, "POST", "/api/vault/folders/notes/sync", &[], "").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(&ro.app, "DELETE", "/api/vault/folders/notes", &[], "").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn c4_turning_index_on_imports_and_off_removes() {
    let fx = fixture(false).await;
    configure(&fx, true).await;
    assert_eq!(doc_count(&fx).await, 2);
    configure(&fx, false).await;
    assert_eq!(doc_count(&fx).await, 0);
}

#[tokio::test]
async fn c4_saving_an_unchanged_folder_does_not_resync_it() {
    let fx = fixture(false).await;
    configure(&fx, true).await;
    assert_eq!(doc_count(&fx).await, 2);
    std::fs::write(fx.dir.path().join("notes/c.md"), "charlie").unwrap();
    configure(&fx, true).await;
    assert_eq!(doc_count(&fx).await, 2);
    let (status, _) = send(&fx.app, "POST", "/api/vault/folders/notes/sync", &[], "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(doc_count(&fx).await, 3);
}

#[tokio::test]
async fn d1_d2_folders_are_listed_and_readable() {
    let fx = fixture(false).await;
    configure(&fx, false).await;
    let (status, body) = send(&fx.app, "PROPFIND", "/dav/", &[("Depth", "1")], "").await;
    assert_eq!(status.as_u16(), 207);
    assert!(body.contains("/dav/folders/"), "{body}");

    let (status, body) = send(&fx.app, "PROPFIND", "/dav/folders/", &[("Depth", "1")], "").await;
    assert_eq!(status.as_u16(), 207);
    assert!(body.contains("/dav/folders/notes/"), "{body}");

    let (status, body) = send(
        &fx.app,
        "PROPFIND",
        "/dav/folders/notes/",
        &[("Depth", "1")],
        "",
    )
    .await;
    assert_eq!(status.as_u16(), 207);
    assert!(body.contains("/dav/folders/notes/a.md"), "{body}");
    assert!(body.contains("/dav/folders/notes/sub/"), "{body}");

    let (status, body) = send(&fx.app, "GET", "/dav/folders/notes/sub/b.txt", &[], "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "bravo");
}

#[tokio::test]
async fn d3_writes_reach_the_disk() {
    let fx = fixture(false).await;
    configure(&fx, false).await;
    let notes = fx.notes();

    let (status, _) = send(&fx.app, "PUT", "/dav/folders/notes/new.md", &[], "fresh").await;
    assert!(status.is_success(), "{status}");
    assert_eq!(
        std::fs::read_to_string(notes.join("new.md")).unwrap(),
        "fresh"
    );

    let (status, _) = send(&fx.app, "PUT", "/dav/folders/notes/a.md", &[], "changed").await;
    assert!(status.is_success(), "{status}");
    assert_eq!(
        std::fs::read_to_string(notes.join("a.md")).unwrap(),
        "changed"
    );

    let (status, _) = send(&fx.app, "MKCOL", "/dav/folders/notes/dir", &[], "").await;
    assert!(status.is_success(), "{status}");
    assert!(notes.join("dir").is_dir());

    let (status, _) = send(
        &fx.app,
        "MOVE",
        "/dav/folders/notes/new.md",
        &[("Destination", "/dav/folders/notes/dir/moved.md")],
        "",
    )
    .await;
    assert!(status.is_success(), "{status}");
    assert!(!notes.join("new.md").exists());
    assert_eq!(
        std::fs::read_to_string(notes.join("dir/moved.md")).unwrap(),
        "fresh"
    );

    let (status, _) = send(&fx.app, "DELETE", "/dav/folders/notes/dir", &[], "").await;
    assert!(status.is_success(), "{status}");
    assert!(!notes.join("dir").exists());
}

#[tokio::test]
async fn d4_escapes_are_refused() {
    let fx = fixture(false).await;
    configure(&fx, false).await;
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(fx.dir.path().join("secret.md"), fx.notes().join("link.md"))
            .unwrap();
        std::os::unix::fs::symlink(fx.dir.path(), fx.notes().join("up")).unwrap();
        let (status, body) = send(&fx.app, "GET", "/dav/folders/notes/link.md", &[], "").await;
        assert!(!status.is_success(), "{status} {body}");
        assert_ne!(body, "outside");
        let (status, body) = send(&fx.app, "GET", "/dav/folders/notes/up/secret.md", &[], "").await;
        assert!(!status.is_success(), "{status} {body}");
        let (status, _) = send(
            &fx.app,
            "PUT",
            "/dav/folders/notes/up/secret.md",
            &[],
            "pwned",
        )
        .await;
        assert!(!status.is_success(), "{status}");
        let (status, _) = send(&fx.app, "PUT", "/dav/folders/notes/link.md", &[], "pwned").await;
        assert!(!status.is_success(), "{status}");
        let (_, listing) = send(
            &fx.app,
            "PROPFIND",
            "/dav/folders/notes/",
            &[("Depth", "1")],
            "",
        )
        .await;
        assert!(
            !listing.contains("link.md") && !listing.contains("/up/"),
            "{listing}"
        );
    }
    let (status, _) = send(
        &fx.app,
        "GET",
        "/dav/folders/notes/%2E%2E/secret.md",
        &[],
        "",
    )
    .await;
    assert!(!status.is_success(), "{status}");
    let (status, _) = send(
        &fx.app,
        "PUT",
        "/dav/folders/notes/%2E%2E/secret.md",
        &[],
        "pwned",
    )
    .await;
    assert!(!status.is_success(), "{status}");
    assert_eq!(
        std::fs::read_to_string(fx.dir.path().join("secret.md")).unwrap(),
        "outside"
    );
    let (status, _) = send(
        &fx.app,
        "MOVE",
        "/dav/folders/notes/a.md",
        &[("Destination", "/dav/memories/a.md")],
        "",
    )
    .await;
    assert!(!status.is_success(), "{status}");
    assert!(fx.notes().join("a.md").exists());
}

#[tokio::test]
async fn d5_d6_folder_roots_are_protected_and_unknown_names_missing() {
    let fx = fixture(false).await;
    configure(&fx, false).await;
    let (status, _) = send(&fx.app, "DELETE", "/dav/folders/notes", &[], "").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(&fx.app, "DELETE", "/dav/folders", &[], "").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(
        &fx.app,
        "MOVE",
        "/dav/folders/notes",
        &[("Destination", "/dav/folders/other")],
        "",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(fx.notes().join("a.md").exists());

    let (status, _) = send(&fx.app, "GET", "/dav/folders/nope/a.md", &[], "").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&fx.app, "PUT", "/dav/folders/nope/a.md", &[], "x").await;
    assert!(!status.is_success(), "{status}");
}

#[tokio::test]
async fn d7_writes_resync_an_indexed_folder() {
    let fx = fixture(false).await;
    configure(&fx, true).await;
    assert_eq!(doc_count(&fx).await, 2);
    let (status, _) = send(&fx.app, "PUT", "/dav/folders/notes/c.md", &[], "charlie").await;
    assert!(status.is_success(), "{status}");
    let mut count = 0;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        count = doc_count(&fx).await;
        if count == 3 {
            break;
        }
    }
    assert_eq!(count, 3);
}

#[tokio::test]
async fn d_readonly_refuses_folder_writes() {
    let fx = fixture(false).await;
    configure(&fx, false).await;
    let hub = WebHub::open(fx.root.clone(), true, 0).await.unwrap();
    let app = hub.nest_routers(CorsLayer::new().allow_origin(Any));
    let (status, _) = send(&app, "PUT", "/dav/folders/notes/a.md", &[], "x").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        std::fs::read_to_string(fx.notes().join("a.md")).unwrap(),
        "# Alpha\n\nalpha body"
    );
}

#[tokio::test]
async fn c4_only_the_local_browser_may_use_the_folder_api() {
    let fx = fixture(false).await;
    configure(&fx, true).await;
    let foreign = [("origin", "https://evil.example")];
    let remote = [("host", "ax.example.com")];
    for headers in [&foreign[..], &remote[..]] {
        let (status, _) = send(&fx.app, "GET", "/api/vault/folders", headers, "").await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{headers:?}");
        let body = json!([{ "name": "home", "path": fx.dir.path(), "index": true }]).to_string();
        let mut with_json = headers.to_vec();
        with_json.push(("content-type", "application/json"));
        let (status, _) = send(&fx.app, "PUT", "/api/vault/folders", &with_json, &body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{headers:?}");
        let (status, _) = send(
            &fx.app,
            "POST",
            "/api/vault/folders/notes/sync",
            headers,
            "",
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{headers:?}");
        let (status, _) = send(&fx.app, "DELETE", "/api/vault/folders/notes", headers, "").await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{headers:?}");
    }
    assert_eq!(doc_count(&fx).await, 2);
    assert_eq!(vault_names(&fx.root), vec!["notes".to_string()]);
}

#[tokio::test]
async fn d_folders_are_local_only_in_the_drive() {
    let fx = fixture(false).await;
    configure(&fx, false).await;
    let remote = [("host", "ax.example.com")];
    let (status, _) = send(&fx.app, "GET", "/dav/folders/notes/a.md", &remote, "").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(
        &fx.app,
        "PROPFIND",
        "/dav/folders/",
        &[("host", "ax.example.com"), ("Depth", "1")],
        "",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(&fx.app, "PUT", "/dav/folders/notes/a.md", &remote, "x").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(
        &fx.app,
        "MOVE",
        "/dav/memories/x.md",
        &[
            ("host", "ax.example.com"),
            ("Destination", "/dav/folders/notes/x.md"),
        ],
        "",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(
        &fx.app,
        "GET",
        "/dav/folders/notes/a.md",
        &[("origin", "https://evil.example")],
        "",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, body) = send(&fx.app, "GET", "/dav/folders/notes/a.md", &[], "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("alpha body"));
}

fn vault_names(root: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(config_file(root)).unwrap();
    serde_json::from_str::<Vec<Value>>(&text)
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap().to_string())
        .collect()
}
