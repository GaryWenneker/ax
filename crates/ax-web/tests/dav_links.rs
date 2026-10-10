//! Vault follow-ups from docs/specs/obsidian-links.md: `app.json` defaults (V) and the
//! `global/` folders (G).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ax_global_db::policy::PolicyKind;
use ax_web::WebHub;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tempfile::TempDir;
use tower::ServiceExt;
use tower_http::cors::{Any, CorsLayer};

static HOME: OnceLock<TempDir> = OnceLock::new();
/// The tests share one `global.db`; each test that uses it holds this for its whole run.
static GLOBAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Point `HOME` and `AX_GLOBAL_DB` at a temp dir shared by every test in this binary.
fn isolate() -> PathBuf {
    let dir = HOME.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", dir.path());
        std::env::set_var("AX_HOME_DIR", dir.path());
        std::env::set_var("AX_GLOBAL_DB", dir.path().join("global.db"));
        dir
    });
    dir.path().join("global.db")
}

struct Fx {
    _dir: TempDir,
    root: PathBuf,
    app: Router,
}

async fn fixture_with(readonly: bool) -> Fx {
    isolate();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let hub = WebHub::open(root.clone(), readonly, 0).await.unwrap();
    let app = hub.nest_routers(
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any),
    );
    Fx {
        _dir: dir,
        root,
        app,
    }
}

async fn fixture() -> Fx {
    fixture_with(false).await
}

struct Res {
    status: StatusCode,
    body: String,
}

async fn send(app: &Router, method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> Res {
    let mut req = Request::builder().method(method).uri(path);
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
    Res {
        status,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

async fn get(app: &Router, path: &str) -> Res {
    send(app, "GET", path, &[], "").await
}

async fn put(app: &Router, path: &str, body: &str) -> Res {
    send(app, "PUT", path, &[], body).await
}

async fn propfind(app: &Router, path: &str) -> Res {
    send(app, "PROPFIND", path, &[("Depth", "1")], "").await
}

async fn with_destination(app: &Router, method: &str, from: &str, to: &str) -> Res {
    let dest = format!("http://localhost{to}");
    send(app, method, from, &[("Destination", dest.as_str())], "").await
}

fn assert_ok(res: &Res) {
    assert!(
        res.status.is_success(),
        "status {}: {}",
        res.status,
        res.body
    );
}

/// Size listed for `href` in a PROPFIND response.
fn listed_size(propfind: &str, href: &str) -> usize {
    let at = propfind
        .find(&format!("{href}<"))
        .unwrap_or_else(|| panic!("{href} not listed in {propfind}"));
    let rest = &propfind[at..];
    let start = rest.find("getcontentlength>").expect("no size") + "getcontentlength>".len();
    let end = start + rest[start..].find('<').unwrap();
    rest[start..end].parse().unwrap()
}

// ── global.db helpers ──

async fn global_pool() -> sqlx::SqlitePool {
    ax_global_db::open_and_init(&isolate()).await.unwrap()
}

async fn other_project(pool: &sqlx::SqlitePool, name: &str) -> i64 {
    let dir = std::env::temp_dir().join(format!("ax-links-{name}"));
    ax_global_db::policy::ensure_project(pool, &dir)
        .await
        .unwrap()
}

async fn seed_skill(pid: i64, name: &str, body: &str) {
    let raw = format!("---\nname: {name}\ndescription: global {name}\n---\n\n{body}");
    let doc = ax_policy::parse_skill_file(Path::new("SKILL.md"), &raw).unwrap();
    let flat = ax_global_db::policy::flatten_list_item(
        serde_json::to_value(&doc).unwrap(),
        PolicyKind::Skills,
        name,
    );
    let pool = global_pool().await;
    ax_global_db::policy::upsert_policy_item(&pool, pid, PolicyKind::Skills, name, &flat)
        .await
        .unwrap();
}

async fn seed_rule(pid: i64, id: &str, body: &str) {
    let raw = format!("---\nid: {id}\nlevel: WARNING\nalwaysApply: true\n---\n\n{body}");
    let doc = ax_policy::parse_rule_file(Path::new("x.mdc"), &raw).unwrap();
    let flat = ax_global_db::policy::flatten_list_item(
        serde_json::to_value(&doc).unwrap(),
        PolicyKind::Rules,
        id,
    );
    let pool = global_pool().await;
    ax_global_db::policy::upsert_policy_item(&pool, pid, PolicyKind::Rules, id, &flat)
        .await
        .unwrap();
}

async fn pid_for(name: &str) -> i64 {
    other_project(&global_pool().await, name).await
}

async fn stored(pid: i64, kind: PolicyKind, id: &str) -> Option<Value> {
    ax_global_db::policy::load_policy_item(&global_pool().await, pid, kind, id)
        .await
        .unwrap()
}

async fn rows_named(kind: PolicyKind, id: &str) -> usize {
    let table = match kind {
        PolicyKind::Rules => "global_policy_rules",
        PolicyKind::Skills => "global_policy_skills",
    };
    let n: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table} WHERE item_id = ?"))
        .bind(id)
        .fetch_one(&global_pool().await)
        .await
        .unwrap();
    n as usize
}

fn body_of(v: &Value) -> &str {
    v.get("body").and_then(|b| b.as_str()).unwrap_or("")
}

// ── V: app.json ──

const APP_JSON: &str = "/dav/.obsidian/app.json";

#[tokio::test]
async fn n2_tests_use_a_temp_global_db() {
    let path = isolate();
    assert_eq!(ax_global_db::global_db_path().unwrap(), path);
    let home = ax_utils::paths::home_dir().unwrap();
    assert_eq!(home, HOME.get().unwrap().path());
    assert!(path.starts_with(&home), "{path:?} not under {home:?}");
    assert!(home
        .canonicalize()
        .unwrap()
        .starts_with(std::env::temp_dir().canonicalize().unwrap()));
}

#[tokio::test]
async fn v1_app_json_defaults_when_absent() {
    let fx = fixture().await;
    let res = get(&fx.app, APP_JSON).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    let v: Value = serde_json::from_str(&res.body).unwrap();
    assert_eq!(
        v,
        json!({"newFileLocation": "folder", "newFileFolderPath": "memories", "alwaysUpdateLinks": true})
    );
}

#[tokio::test]
async fn v2_app_json_adds_only_missing_keys() {
    let fx = fixture().await;
    assert_ok(
        &put(
            &fx.app,
            APP_JSON,
            r#"{"newFileLocation":"root","promptDelete":false}"#,
        )
        .await,
    );
    let v: Value = serde_json::from_str(&get(&fx.app, APP_JSON).await.body).unwrap();
    assert_eq!(v["newFileLocation"], "root");
    assert_eq!(v["promptDelete"], false);
    assert_eq!(v["newFileFolderPath"], "memories");
    assert_eq!(v["alwaysUpdateLinks"], true);
}

#[tokio::test]
async fn v2_complete_app_json_is_served_byte_for_byte() {
    let fx = fixture().await;
    let text = "{\n\t\"newFileLocation\": \"current\",\n\t\"newFileFolderPath\": \"inbox\",\n\t\"alwaysUpdateLinks\": false\n}\n";
    assert_ok(&put(&fx.app, APP_JSON, text).await);
    assert_eq!(get(&fx.app, APP_JSON).await.body, text);
}

#[tokio::test]
async fn v3_app_json_that_is_not_an_object_is_served_unchanged() {
    let fx = fixture().await;
    for text in ["[1, 2]", "{oops", ""] {
        assert_ok(&put(&fx.app, APP_JSON, text).await);
        assert_eq!(get(&fx.app, APP_JSON).await.body, text);
    }
}

#[tokio::test]
async fn v4_listed_size_matches_served_size() {
    let fx = fixture().await;
    assert_ok(&send(&fx.app, "MKCOL", "/dav/.obsidian", &[], "").await);
    let served = get(&fx.app, APP_JSON).await.body.len();
    let listing = propfind(&fx.app, "/dav/.obsidian/").await;
    assert_eq!(
        listed_size(&listing.body, APP_JSON),
        served,
        "absent app.json"
    );

    assert_ok(&put(&fx.app, APP_JSON, r#"{"a":1}"#).await);
    let served = get(&fx.app, APP_JSON).await.body.len();
    assert!(served > r#"{"a":1}"#.len());
    let listing = propfind(&fx.app, "/dav/.obsidian/").await;
    assert_eq!(
        listed_size(&listing.body, APP_JSON),
        served,
        "partial app.json"
    );
}

#[tokio::test]
async fn v5_new_note_in_memories_becomes_a_memory() {
    let fx = fixture().await;
    assert_ok(&put(&fx.app, "/dav/memories/Untitled.md", "").await);
    assert!(get(&fx.app, "/dav/DRAFTS.md")
        .await
        .body
        .contains("[[memories/Untitled]]: empty page"));
    assert_ok(
        &with_destination(
            &fx.app,
            "MOVE",
            "/dav/memories/Untitled.md",
            "/dav/memories/My%20note.md",
        )
        .await,
    );
    assert_ok(&put(&fx.app, "/dav/memories/My%20note.md", "Remember this.").await);
    let page = get(&fx.app, "/dav/memories/My%20note.md").await;
    assert!(page.body.contains("Remember this."), "{}", page.body);
    assert!(
        page.body.contains("id: "),
        "not a saved memory: {}",
        page.body
    );
    assert!(get(&fx.app, "/dav/DRAFTS.md")
        .await
        .body
        .contains("No drafts."));
    assert_eq!(
        get(&fx.app, "/dav/memories/Untitled.md").await.status,
        StatusCode::NOT_FOUND
    );
}

// ── G: global rules and skills ──

#[tokio::test]
async fn g1_global_folders_list_the_leading_global_items() {
    let _global = GLOBAL.lock().await;
    let (a, b) = (pid_for("g1-a").await, pid_for("g1-b").await);
    seed_skill(a, "g1-skill", "short").await;
    seed_skill(b, "g1-skill", "the longer body wins").await;
    seed_rule(a, "g1-rule", "Rule body.").await;
    let fx = fixture().await;

    assert!(propfind(&fx.app, "/dav/")
        .await
        .body
        .contains("/dav/global/"));
    let global = propfind(&fx.app, "/dav/global/").await;
    assert_eq!(global.status, StatusCode::MULTI_STATUS, "{}", global.body);
    assert!(
        global.body.contains("/dav/global/rules/"),
        "{}",
        global.body
    );
    assert!(
        global.body.contains("/dav/global/skills/"),
        "{}",
        global.body
    );

    let skills = propfind(&fx.app, "/dav/global/skills/").await;
    let page = get(&fx.app, "/dav/global/skills/g1-skill.md").await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.body.starts_with("---\n"), "{}", page.body);
    assert!(page.body.contains("name: g1-skill"), "{}", page.body);
    assert!(page.body.contains("the longer body wins"), "{}", page.body);
    assert_eq!(
        listed_size(&skills.body, "/dav/global/skills/g1-skill.md"),
        page.body.len()
    );

    let rule = get(&fx.app, "/dav/global/rules/g1-rule.md").await;
    assert!(
        rule.body.contains("id: g1-rule") && rule.body.contains("Rule body."),
        "{}",
        rule.body
    );
}

#[tokio::test]
async fn g2_saving_a_global_page_updates_the_leader_in_global_db() {
    let _global = GLOBAL.lock().await;
    let (a, b) = (pid_for("g2-a").await, pid_for("g2-b").await);
    seed_skill(a, "g2-skill", "short").await;
    seed_skill(b, "g2-skill", "the longer original body").await;
    seed_rule(a, "g2-rule", "Old rule.").await;
    let fx = fixture().await;

    let page = get(&fx.app, "/dav/global/skills/g2-skill.md").await.body;
    let edited = page.replace(
        "the longer original body",
        "Edited in Obsidian, see [[pr]].",
    );
    assert_ok(&put(&fx.app, "/dav/global/skills/g2-skill.md", &edited).await);
    let drafts = get(&fx.app, "/dav/DRAFTS.md").await.body;
    assert!(drafts.contains("No drafts."), "{drafts}");
    assert_eq!(
        body_of(&stored(b, PolicyKind::Skills, "g2-skill").await.unwrap()),
        "Edited in Obsidian, see [[pr]]."
    );
    let short = stored(a, PolicyKind::Skills, "g2-skill").await;
    assert_ne!(
        short.as_ref().map(body_of),
        Some("Edited in Obsidian, see [[pr]]."),
        "saved to the shadowed copy"
    );
    let api = get(
        &fx.app,
        &format!("/api/policy/skills/g2-skill?origin=global&projectId={b}"),
    )
    .await;
    assert!(api.body.contains("Edited in Obsidian"), "{}", api.body);
    assert_eq!(
        get(&fx.app, "/api/policy/skills/g2-skill").await.status,
        StatusCode::NOT_FOUND
    );

    assert_ok(&put(&fx.app, "/dav/global/rules/g2-rule.md", "New rule body.").await);
    let rule = stored(a, PolicyKind::Rules, "g2-rule").await.unwrap();
    assert_eq!(body_of(&rule), "New rule body.");
    assert_eq!(
        rule["level"], "WARNING",
        "body-only save keeps frontmatter: {rule}"
    );
    assert!(get(&fx.app, "/dav/DRAFTS.md")
        .await
        .body
        .contains("No drafts."));
}

#[tokio::test]
async fn g3_invalid_global_page_stays_a_draft() {
    let _global = GLOBAL.lock().await;
    let a = pid_for("g3").await;
    seed_skill(a, "g3-skill", "Original.").await;
    let fx = fixture().await;
    let wrong = "---\nname: other-name\ndescription: d\n---\n\nBody";
    assert_ok(&put(&fx.app, "/dav/global/skills/g3-skill.md", wrong).await);
    assert_eq!(
        body_of(&stored(a, PolicyKind::Skills, "g3-skill").await.unwrap()),
        "Original."
    );
    let drafts = get(&fx.app, "/dav/DRAFTS.md").await.body;
    assert!(
        drafts.contains("[[global/skills/g3-skill]]: name does not match"),
        "{drafts}"
    );
    assert_eq!(
        get(&fx.app, "/dav/global/skills/g3-skill.md").await.body,
        wrong
    );
    assert_eq!(rows_named(PolicyKind::Skills, "other-name").await, 0);
}

#[tokio::test]
async fn g4_new_global_page_creates_a_global_item() {
    let _global = GLOBAL.lock().await;
    let fx = fixture().await;
    assert_ok(
        &put(
            &fx.app,
            "/dav/global/skills/g4-new-skill.md",
            "How to do it.",
        )
        .await,
    );
    assert_ok(&put(&fx.app, "/dav/global/rules/g4-new-rule.md", "Always do it.").await);
    let pool = global_pool().await;
    let pid = ax_global_db::policy::ensure_project(&pool, &fx.root)
        .await
        .unwrap();
    assert_eq!(
        body_of(
            &stored(pid, PolicyKind::Skills, "g4-new-skill")
                .await
                .unwrap()
        ),
        "How to do it."
    );
    assert_eq!(
        body_of(&stored(pid, PolicyKind::Rules, "g4-new-rule").await.unwrap()),
        "Always do it."
    );
    let level: String =
        sqlx::query_scalar("SELECT level FROM global_policy_skills WHERE item_id = 'g4-new-skill'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(level, "global");
    assert!(get(&fx.app, "/dav/global/skills/g4-new-skill.md")
        .await
        .body
        .contains("How to do it."));
}

#[tokio::test]
async fn g5_saved_global_pages_cannot_be_deleted_moved_or_copied() {
    let _global = GLOBAL.lock().await;
    let a = pid_for("g5").await;
    seed_skill(a, "g5-skill", "Keep me.").await;
    seed_rule(a, "g5-rule", "Keep me too.").await;
    let fx = fixture().await;
    let page = "/dav/global/skills/g5-skill.md";
    assert_eq!(
        send(&fx.app, "DELETE", page, &[], "").await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(&fx.app, "DELETE", "/dav/global/rules/g5-rule.md", &[], "")
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    for (method, to) in [
        ("MOVE", "/dav/global/skills/g5-renamed.md"),
        ("MOVE", "/dav/skills/g5-skill.md"),
        ("MOVE", "/dav/other.md"),
        ("COPY", "/dav/global/skills/g5-copy.md"),
        ("COPY", "/dav/skills/g5-skill.md"),
    ] {
        let res = with_destination(&fx.app, method, page, to).await;
        assert_eq!(
            res.status,
            StatusCode::FORBIDDEN,
            "{method} to {to}: {}",
            res.body
        );
    }
    assert_eq!(
        body_of(&stored(a, PolicyKind::Skills, "g5-skill").await.unwrap()),
        "Keep me."
    );
    assert_eq!(
        body_of(&stored(a, PolicyKind::Rules, "g5-rule").await.unwrap()),
        "Keep me too."
    );
    for id in ["g5-renamed", "g5-copy"] {
        assert_eq!(rows_named(PolicyKind::Skills, id).await, 0, "{id}");
    }
    assert_eq!(
        get(&fx.app, "/api/policy/skills/g5-skill").await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&fx.app, "/dav/other.md").await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn g5_unsaved_global_draft_follows_the_new_note_flow() {
    let _global = GLOBAL.lock().await;
    let fx = fixture().await;
    assert_ok(&put(&fx.app, "/dav/global/skills/Untitled.md", "").await);
    assert_ok(
        &with_destination(
            &fx.app,
            "MOVE",
            "/dav/global/skills/Untitled.md",
            "/dav/global/skills/g5-flow.md",
        )
        .await,
    );
    assert_ok(&put(&fx.app, "/dav/global/skills/g5-flow.md", "Steps.").await);
    assert_eq!(rows_named(PolicyKind::Skills, "g5-flow").await, 1);
    assert!(get(&fx.app, "/dav/DRAFTS.md")
        .await
        .body
        .contains("No drafts."));

    assert_ok(&put(&fx.app, "/dav/global/rules/Scratch.md", "not kebab").await);
    assert_ok(&send(&fx.app, "DELETE", "/dav/global/rules/Scratch.md", &[], "").await);
    assert!(get(&fx.app, "/dav/DRAFTS.md")
        .await
        .body
        .contains("No drafts."));
}

#[tokio::test]
async fn g6_global_folders_are_protected() {
    let _global = GLOBAL.lock().await;
    let a = pid_for("g6").await;
    seed_skill(a, "g6-skill", "Keep.").await;
    let fx = fixture().await;
    for path in [
        "/dav/global",
        "/dav/global/",
        "/dav/global/skills/",
        "/dav/global/rules",
    ] {
        assert_eq!(
            send(&fx.app, "DELETE", path, &[], "").await.status,
            StatusCode::FORBIDDEN,
            "DELETE {path}"
        );
    }
    for (from, to) in [
        ("/dav/global/", "/dav/elsewhere/"),
        ("/dav/global/skills/", "/dav/skills2/"),
        ("/dav/memories/", "/dav/global/"),
    ] {
        let res = with_destination(&fx.app, "MOVE", from, to).await;
        assert_eq!(res.status, StatusCode::FORBIDDEN, "MOVE {from} to {to}");
    }
    assert_eq!(rows_named(PolicyKind::Skills, "g6-skill").await, 1);
    assert_eq!(
        get(&fx.app, "/dav/global/skills/g6-skill.md").await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn g7_readonly_blocks_global_writes() {
    let _global = GLOBAL.lock().await;
    let a = pid_for("g7").await;
    seed_skill(a, "g7-skill", "Read only.").await;
    let fx = fixture_with(true).await;
    assert_eq!(
        put(&fx.app, "/dav/global/skills/g7-skill.md", "changed")
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        put(&fx.app, "/dav/global/skills/g7-other.md", "new")
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        get(&fx.app, "/dav/global/skills/g7-skill.md").await.status,
        StatusCode::OK
    );
    assert_eq!(
        body_of(&stored(a, PolicyKind::Skills, "g7-skill").await.unwrap()),
        "Read only."
    );
    assert_eq!(rows_named(PolicyKind::Skills, "g7-other").await, 0);
}
