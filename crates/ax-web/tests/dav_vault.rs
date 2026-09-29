//! WebDAV vault (`/dav`) behaviors from docs/specs/obsidian-webdav-vault.md.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ax_policy::parse_rule_file;
use ax_web::WebHub;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use axum::Router;
use tempfile::TempDir;
use tower::ServiceExt;
use tower_http::cors::{Any, CorsLayer};

const RULE: &str =
    "---\nid: english-only\nlevel: WARNING\nalwaysApply: true\n---\n\nWrite English.";

static HOME: OnceLock<TempDir> = OnceLock::new();

/// Keep `~/.ax/global.db` of the machine running the tests out of reach.
fn isolate_home() {
    HOME.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", dir.path());
        std::env::set_var("AX_HOME_DIR", dir.path());
        dir
    });
}

struct Fx {
    _dir: TempDir,
    root: PathBuf,
    hub: WebHub,
    app: Router,
}

fn app_for(hub: &WebHub) -> Router {
    hub.nest_routers(
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any),
    )
}

async fn fixture_with(readonly: bool) -> Fx {
    isolate_home();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    {
        let seed = WebHub::open(root.clone(), false, 0).await.unwrap();
        let ws = seed.read().await;
        let doc = parse_rule_file(Path::new("x.mdc"), RULE).unwrap();
        ws.policy
            .store
            .save_rule(doc.frontmatter, doc.body)
            .await
            .unwrap();
        ax_memory::remember(
            &ws.graph_pool,
            ax_memory::RememberInput {
                title: "Use SQLite".into(),
                body: "SQLite is the source of truth.".into(),
                kind: Some("decision".into()),
                tags: vec!["db".into()],
                files: vec!["crates/ax-db/src/lib.rs".into()],
                source: None,
            },
        )
        .await
        .unwrap();
    }
    let hub = WebHub::open(root.clone(), readonly, 0).await.unwrap();
    let app = app_for(&hub);
    Fx {
        _dir: dir,
        root,
        hub,
        app,
    }
}

async fn fixture() -> Fx {
    fixture_with(false).await
}

struct Res {
    status: StatusCode,
    headers: HeaderMap,
    body: String,
}

async fn send(
    app: &Router,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: impl Into<Body>,
) -> Res {
    let mut req = Request::builder().method(method).uri(path);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let res = app
        .clone()
        .oneshot(req.body(body.into()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    Res {
        status,
        headers,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

async fn get(app: &Router, path: &str) -> Res {
    send(app, "GET", path, &[], Body::empty()).await
}

async fn put(app: &Router, path: &str, body: &str) -> Res {
    send(app, "PUT", path, &[], body.to_string()).await
}

async fn propfind(app: &Router, path: &str) -> Res {
    send(app, "PROPFIND", path, &[("Depth", "1")], Body::empty()).await
}

async fn mv(app: &Router, from: &str, to: &str) -> Res {
    let dest = format!("http://localhost{to}");
    send(
        app,
        "MOVE",
        from,
        &[("Destination", dest.as_str())],
        Body::empty(),
    )
    .await
}

async fn rule_json(app: &Router, id: &str) -> Res {
    get(app, &format!("/api/policy/rules/{id}")).await
}

async fn memory_by_title(fx: &Fx, title: &str) -> Option<ax_memory::MemoryRow> {
    let ws = fx.hub.read().await;
    let (rows, _) = ax_memory::list(&ws.graph_pool, 1000, 0).await.unwrap();
    rows.into_iter().find(|m| m.title == title)
}

fn assert_ok(res: &Res) {
    assert!(
        res.status.is_success(),
        "status {}: {}",
        res.status,
        res.body
    );
}

// ── Reading ──

#[tokio::test]
async fn b01_root_lists_areas_and_drafts() {
    let fx = fixture().await;
    let res = propfind(&fx.app, "/dav/").await;
    assert_eq!(res.status, StatusCode::MULTI_STATUS, "{}", res.body);
    for href in [
        "/dav/rules/",
        "/dav/skills/",
        "/dav/memories/",
        "/dav/DRAFTS.md",
    ] {
        assert!(res.body.contains(href), "missing {href} in {}", res.body);
    }
}

#[tokio::test]
async fn b02_rules_folder_lists_rules_with_real_size() {
    let fx = fixture().await;
    let res = propfind(&fx.app, "/dav/rules/").await;
    assert_eq!(res.status, StatusCode::MULTI_STATUS, "{}", res.body);
    assert!(
        res.body.contains("/dav/rules/english-only.md"),
        "{}",
        res.body
    );
    let page = get(&fx.app, "/dav/rules/english-only.md").await;
    let len = format!("getcontentlength>{}<", page.body.len());
    assert!(res.body.contains(&len), "expected {len} in {}", res.body);
}

#[tokio::test]
async fn b03_get_rule_is_serialized_rule() {
    let fx = fixture().await;
    let res = get(&fx.app, "/dav/rules/english-only.md").await;
    assert_eq!(res.status, StatusCode::OK);
    let ws = fx.hub.read().await;
    let doc = ws
        .policy
        .store
        .get_rule_doc("english-only")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        res.body,
        ax_policy::serialize_rule(&doc.frontmatter, &doc.body)
    );
}

#[tokio::test]
async fn b04_get_memory_has_id_in_frontmatter() {
    let fx = fixture().await;
    let row = memory_by_title(&fx, "Use SQLite").await.unwrap();
    let res = get(&fx.app, "/dav/memories/Use%20SQLite.md").await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert!(res.body.starts_with("---\n"), "{}", res.body);
    assert!(
        res.body.contains(&format!("id: {}", row.id)),
        "{}",
        res.body
    );
    assert!(res.body.contains("SQLite is the source of truth."));
}

// ── Saving an existing page ──

#[tokio::test]
async fn b05_put_valid_rule_saves_and_records_revision() {
    let fx = fixture().await;
    let revisions = |fx: &Fx| {
        let hub = fx.hub.clone();
        async move {
            let ws = hub.read().await;
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM policy_revisions WHERE kind = 'rule' AND item_id = 'english-only'",
            )
            .fetch_one(ws.policy.store.pool())
            .await
            .unwrap()
        }
    };
    let before = revisions(&fx).await;
    let text = RULE.replace("Write English.", "Always write English.");
    assert_ok(&put(&fx.app, "/dav/rules/english-only.md", &text).await);
    let json = rule_json(&fx.app, "english-only").await;
    assert!(json.body.contains("Always write English."), "{}", json.body);
    assert!(revisions(&fx).await > before);
}

#[tokio::test]
async fn b06_put_body_only_keeps_frontmatter() {
    let fx = fixture().await;
    assert_ok(
        &put(
            &fx.app,
            "/dav/rules/english-only.md",
            "Only the body changed.",
        )
        .await,
    );
    let page = get(&fx.app, "/dav/rules/english-only.md").await;
    assert!(page.body.contains("level: WARNING"), "{}", page.body);
    assert!(page.body.contains("alwaysApply: true"), "{}", page.body);
    assert!(
        page.body.ends_with("Only the body changed."),
        "{}",
        page.body
    );
}

#[tokio::test]
async fn b07_invalid_frontmatter_becomes_draft_and_keeps_rule() {
    let fx = fixture().await;
    let bad = "---\nid: english-only\nlevel: LOUD\nalwaysApply: true\n---\n\nBroken.";
    assert_ok(&put(&fx.app, "/dav/rules/english-only.md", bad).await);
    assert_eq!(get(&fx.app, "/dav/rules/english-only.md").await.body, bad);
    let drafts = get(&fx.app, "/dav/DRAFTS.md").await.body;
    assert!(drafts.contains("[[rules/english-only]]"), "{drafts}");
    assert!(drafts.contains("level"), "{drafts}");
    let json = rule_json(&fx.app, "english-only").await;
    assert!(json.body.contains("Write English."), "{}", json.body);
    assert!(!json.body.contains("Broken."), "{}", json.body);
}

#[tokio::test]
async fn b08_id_mismatch_becomes_draft_and_touches_neither_rule() {
    let fx = fixture().await;
    let other = RULE
        .replace("id: english-only", "id: other-rule")
        .replace("Write English.", "Hijack.");
    assert_ok(&put(&fx.app, "/dav/rules/english-only.md", &other).await);
    assert_eq!(
        rule_json(&fx.app, "other-rule").await.status,
        StatusCode::NOT_FOUND
    );
    assert!(rule_json(&fx.app, "english-only")
        .await
        .body
        .contains("Write English."));
    let drafts = get(&fx.app, "/dav/DRAFTS.md").await.body;
    assert!(drafts.contains("id does not match file name"), "{drafts}");
}

#[tokio::test]
async fn b09_valid_save_clears_draft() {
    let fx = fixture().await;
    put(
        &fx.app,
        "/dav/rules/english-only.md",
        "---\nid: english-only\nlevel: LOUD\n---\nx",
    )
    .await;
    assert_ok(&put(&fx.app, "/dav/rules/english-only.md", RULE).await);
    let drafts = get(&fx.app, "/dav/DRAFTS.md").await.body;
    assert!(!drafts.contains("english-only"), "{drafts}");
    assert!(drafts.contains("No drafts."), "{drafts}");
    let ws = fx.hub.read().await;
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM dav_files")
        .fetch_one(ws.policy.store.pool())
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

#[tokio::test]
async fn b10_put_memory_updates_fields() {
    let fx = fixture().await;
    let row = memory_by_title(&fx, "Use SQLite").await.unwrap();
    let text = format!(
        "---\nid: {}\nkind: convention\ntags: [db, obsidian]\nfiles: [a.rs]\n---\n\nNew body.",
        row.id
    );
    assert_ok(&put(&fx.app, "/dav/memories/Use%20SQLite.md", &text).await);
    let ws = fx.hub.read().await;
    let after = ax_memory::get(&ws.graph_pool, &row.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.body, "New body.");
    assert_eq!(after.kind, "convention");
    assert_eq!(after.tags, vec!["db".to_string(), "obsidian".to_string()]);
    assert_eq!(after.files, vec!["a.rs".to_string()]);
    assert_eq!(after.title, "Use SQLite");
}

// ── Creating ──

#[tokio::test]
async fn b11_body_only_page_creates_rule() {
    let fx = fixture().await;
    assert_eq!(
        put(&fx.app, "/dav/rules/new-rule.md", "Do the thing.")
            .await
            .status,
        StatusCode::CREATED
    );
    let ws = fx.hub.read().await;
    let doc = ws
        .policy
        .store
        .get_rule_doc("new-rule")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(doc.frontmatter.level, "INFO");
    assert_eq!(doc.frontmatter.triggers, vec!["new-rule".to_string()]);
    assert_eq!(doc.body, "Do the thing.");
}

#[tokio::test]
async fn b12_obsidian_new_note_flow_ends_in_rule() {
    let fx = fixture().await;
    assert_ok(&put(&fx.app, "/dav/rules/Untitled.md", "").await);
    assert!(get(&fx.app, "/dav/DRAFTS.md")
        .await
        .body
        .contains("[[rules/Untitled]]"));
    assert_ok(&mv(&fx.app, "/dav/rules/Untitled.md", "/dav/rules/my-rule.md").await);
    assert_eq!(
        get(&fx.app, "/dav/rules/Untitled.md").await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&fx.app, "/dav/rules/my-rule.md").await.status,
        StatusCode::OK
    );
    assert_ok(&put(&fx.app, "/dav/rules/my-rule.md", "Typed text.").await);
    assert!(rule_json(&fx.app, "my-rule")
        .await
        .body
        .contains("Typed text."));
    assert!(get(&fx.app, "/dav/DRAFTS.md")
        .await
        .body
        .contains("No drafts."));
}

#[tokio::test]
async fn b13_new_memory_page_then_update_same_memory() {
    let fx = fixture().await;
    assert_ok(&put(&fx.app, "/dav/memories/Some%20idea.md", "First.").await);
    assert_ok(&put(&fx.app, "/dav/memories/Some%20idea.md", "Second.").await);
    let ws = fx.hub.read().await;
    let (rows, _) = ax_memory::list(&ws.graph_pool, 1000, 0).await.unwrap();
    let matching: Vec<_> = rows.iter().filter(|m| m.title == "Some idea").collect();
    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0].body, "Second.");
}

// ── Renaming and deleting ──

#[tokio::test]
async fn b14_move_renames_rule() {
    let fx = fixture().await;
    assert_ok(
        &mv(
            &fx.app,
            "/dav/rules/english-only.md",
            "/dav/rules/english-always.md",
        )
        .await,
    );
    assert_eq!(
        rule_json(&fx.app, "english-only").await.status,
        StatusCode::NOT_FOUND
    );
    assert!(rule_json(&fx.app, "english-always")
        .await
        .body
        .contains("Write English."));
}

#[tokio::test]
async fn b15_move_renames_memory() {
    let fx = fixture().await;
    let row = memory_by_title(&fx, "Use SQLite").await.unwrap();
    assert_ok(
        &mv(
            &fx.app,
            "/dav/memories/Use%20SQLite.md",
            "/dav/memories/SQLite%20is%20truth.md",
        )
        .await,
    );
    let ws = fx.hub.read().await;
    let after = ax_memory::get(&ws.graph_pool, &row.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.title, "SQLite is truth");
    assert_eq!(after.body, "SQLite is the source of truth.");
    assert_eq!(after.tags, vec!["db".to_string()]);
}

#[tokio::test]
async fn b16_delete_rule() {
    let fx = fixture().await;
    assert_ok(
        &send(
            &fx.app,
            "DELETE",
            "/dav/rules/english-only.md",
            &[],
            Body::empty(),
        )
        .await,
    );
    assert_eq!(
        rule_json(&fx.app, "english-only").await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn b17_top_level_folders_cannot_be_deleted_or_moved() {
    let fx = fixture().await;
    for dir in ["/dav/rules/", "/dav/skills/", "/dav/memories/", "/dav/"] {
        let del = send(&fx.app, "DELETE", dir, &[], Body::empty()).await;
        assert!(del.status.is_client_error(), "DELETE {dir}: {}", del.status);
    }
    let moved = mv(&fx.app, "/dav/rules/", "/dav/old-rules/").await;
    assert!(moved.status.is_client_error(), "MOVE: {}", moved.status);
    assert!(rule_json(&fx.app, "english-only").await.status.is_success());
    assert!(memory_by_title(&fx, "Use SQLite").await.is_some());
}

// ── Obsidian and macOS housekeeping ──

#[tokio::test]
async fn b18_obsidian_config_persists_across_restart() {
    let fx = fixture().await;
    assert_ok(&send(&fx.app, "MKCOL", "/dav/.obsidian/", &[], Body::empty()).await);
    assert_ok(&put(&fx.app, "/dav/.obsidian/appearance.json", "{\"a\":1}").await);
    assert_ok(
        &send(
            &fx.app,
            "MKCOL",
            "/dav/.obsidian/plugins/",
            &[],
            Body::empty(),
        )
        .await,
    );
    let root = propfind(&fx.app, "/dav/").await;
    assert!(root.body.contains("/dav/.obsidian/"), "{}", root.body);
    let restarted = WebHub::open(fx.root.clone(), false, 0).await.unwrap();
    let app = app_for(&restarted);
    assert_eq!(
        get(&app, "/dav/.obsidian/appearance.json").await.body,
        "{\"a\":1}"
    );
    assert!(propfind(&app, "/dav/.obsidian/")
        .await
        .body
        .contains("/dav/.obsidian/plugins/"));
}

#[tokio::test]
async fn b19_macos_junk_is_accepted_and_dropped() {
    let fx = fixture().await;
    assert_ok(&put(&fx.app, "/dav/rules/._english-only.md", "junk").await);
    assert_ok(&put(&fx.app, "/dav/.DS_Store", "junk").await);
    assert_eq!(
        get(&fx.app, "/dav/.DS_Store").await.status,
        StatusCode::NOT_FOUND
    );
    let ws = fx.hub.read().await;
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM dav_files")
        .fetch_one(ws.policy.store.pool())
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

#[tokio::test]
async fn b20_options_advertises_locking_and_lock_works() {
    let fx = fixture().await;
    let opts = send(&fx.app, "OPTIONS", "/dav/", &[], Body::empty()).await;
    let dav = opts
        .headers
        .get("dav")
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    assert!(dav.contains('2'), "DAV header: {dav:?}");
    let lock_body = r#"<?xml version="1.0" encoding="utf-8"?><D:lockinfo xmlns:D="DAV:"><D:lockscope><D:exclusive/></D:lockscope><D:locktype><D:write/></D:locktype><D:owner>test</D:owner></D:lockinfo>"#;
    let lock = send(
        &fx.app,
        "LOCK",
        "/dav/rules/english-only.md",
        &[("Timeout", "Second-60")],
        lock_body,
    )
    .await;
    assert_ok(&lock);
    let token = lock
        .headers
        .get("lock-token")
        .expect("lock token")
        .to_str()
        .unwrap()
        .to_string();
    let unlock = send(
        &fx.app,
        "UNLOCK",
        "/dav/rules/english-only.md",
        &[("Lock-Token", token.as_str())],
        Body::empty(),
    )
    .await;
    assert_ok(&unlock);
}

#[tokio::test]
async fn skill_page_round_trip_and_body_only_save() {
    let fx = fixture().await;
    assert_eq!(
        put(&fx.app, "/dav/skills/deploy.md", "Deploy steps.")
            .await
            .status,
        StatusCode::CREATED
    );
    assert!(propfind(&fx.app, "/dav/skills/")
        .await
        .body
        .contains("/dav/skills/deploy.md"));
    assert_ok(&put(&fx.app, "/dav/skills/deploy.md", "New steps.").await);
    let page = get(&fx.app, "/dav/skills/deploy.md").await;
    assert!(page.body.contains("name: deploy"), "{}", page.body);
    assert!(page.body.ends_with("New steps."), "{}", page.body);
}

// ── Invariants ──

fn disk_files(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let rel = path.strip_prefix(root).unwrap().to_path_buf();
            if rel.starts_with(".ax") && rel.to_string_lossy().contains("ax.db") {
                continue;
            }
            if path.is_dir() {
                walk(&path, root, out);
            } else {
                out.push(rel);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

#[tokio::test]
async fn n1_vault_writes_leave_no_files_on_disk() {
    let fx = fixture().await;
    let before = disk_files(&fx.root);
    send(&fx.app, "MKCOL", "/dav/.obsidian/", &[], Body::empty()).await;
    assert_ok(&put(&fx.app, "/dav/.obsidian/workspace.json", "{}").await);
    assert_ok(&put(&fx.app, "/dav/memories/Disk%20check.md", "No file.").await);
    assert_ok(
        &put(
            &fx.app,
            "/dav/rules/english-only.md",
            "---\nid: english-only\nlevel: LOUD\n---\nx",
        )
        .await,
    );
    assert_eq!(disk_files(&fx.root), before);
}

#[tokio::test]
async fn n3_path_traversal_is_rejected() {
    let fx = fixture().await;
    for path in [
        "/dav/../api/version",
        "/dav/rules/%2e%2e/%2e%2e/escape.md",
        "/dav/rules/..%2f..%2fescape.md",
    ] {
        let res = put(&fx.app, path, "x").await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST, "PUT {path}");
    }
    let ws = fx.hub.read().await;
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM dav_files")
        .fetch_one(ws.policy.store.pool())
        .await
        .unwrap();
    assert_eq!(rows, 0);
    assert!(!fx.root.join("escape.md").exists());
}

#[tokio::test]
async fn n4_dav_sends_no_cors_headers() {
    let fx = fixture().await;
    let preflight = [
        ("Origin", "https://evil.example"),
        ("Access-Control-Request-Method", "PUT"),
    ];
    let api = send(
        &fx.app,
        "OPTIONS",
        "/api/version",
        &preflight,
        Body::empty(),
    )
    .await;
    assert!(
        api.headers.contains_key("access-control-allow-origin"),
        "control: /api must have CORS"
    );
    let dav = send(
        &fx.app,
        "OPTIONS",
        "/dav/rules/english-only.md",
        &preflight,
        Body::empty(),
    )
    .await;
    assert!(
        !dav.headers.contains_key("access-control-allow-origin"),
        "{:?}",
        dav.headers
    );
    let get = send(
        &fx.app,
        "GET",
        "/dav/rules/english-only.md",
        &preflight[..1],
        Body::empty(),
    )
    .await;
    assert!(!get.headers.contains_key("access-control-allow-origin"));
}

#[tokio::test]
async fn n5_readonly_blocks_every_write() {
    let fx = fixture_with(true).await;
    assert_eq!(
        get(&fx.app, "/dav/rules/english-only.md").await.status,
        StatusCode::OK
    );
    let writes = [
        ("PUT", "/dav/rules/english-only.md"),
        ("PUT", "/dav/.obsidian/app.json"),
        ("DELETE", "/dav/rules/english-only.md"),
        ("MKCOL", "/dav/.obsidian/"),
        ("PROPPATCH", "/dav/rules/english-only.md"),
    ];
    for (method, path) in writes {
        let res = send(&fx.app, method, path, &[], "x").await;
        assert_eq!(res.status, StatusCode::FORBIDDEN, "{method} {path}");
    }
    let moved = mv(&fx.app, "/dav/rules/english-only.md", "/dav/rules/x.md").await;
    assert_eq!(moved.status, StatusCode::FORBIDDEN, "MOVE");
    assert!(rule_json(&fx.app, "english-only")
        .await
        .body
        .contains("Write English."));
}

#[tokio::test]
async fn n6_put_over_5mb_is_rejected() {
    let fx = fixture().await;
    let big = "a".repeat(5 * 1024 * 1024 + 1);
    let res = put(&fx.app, "/dav/.obsidian/big.json", &big).await;
    assert_eq!(res.status, StatusCode::PAYLOAD_TOO_LARGE);
    let res = put(&fx.app, "/dav/rules/english-only.md", &big).await;
    assert_eq!(res.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(rule_json(&fx.app, "english-only")
        .await
        .body
        .contains("Write English."));
}
