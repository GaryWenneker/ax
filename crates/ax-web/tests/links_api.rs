//! `GET /api/links` (A1–A4 in docs/specs/obsidian-links.md).

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
static GLOBAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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

const RULE: &str =
    "---\nid: review-rule\nlevel: WARNING\nalwaysApply: true\ntags: [review, azure]\n---\n\n\
Use [[pr]], [[memories/Use SQLite|the DB note]] and [[nope#Part]].\n\nNot a link: `[[code]]`.";
const SKILL: &str =
    "---\nname: pr\ndescription: Open a PR\ntags: [git]\n---\n\nFollow [[review-rule]].";
const GLOBAL_SKILL: &str = "Back to [[rules/review-rule]] and [[pr]].";

struct Fx {
    _dir: TempDir,
    app: Router,
    memory_id: String,
    global_pid: i64,
}

async fn fixture_with(readonly: bool) -> Fx {
    let global = isolate();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let memory_id = {
        let seed = WebHub::open(root.clone(), false, 0).await.unwrap();
        let ws = seed.read().await;
        let rule = ax_policy::parse_rule_file(Path::new("x.mdc"), RULE).unwrap();
        ws.policy
            .store
            .save_rule(rule.frontmatter, rule.body)
            .await
            .unwrap();
        let skill = ax_policy::parse_skill_file(Path::new("SKILL.md"), SKILL).unwrap();
        ws.policy
            .store
            .save_skill(skill.frontmatter, skill.body)
            .await
            .unwrap();
        ax_memory::remember(
            &ws.graph_pool,
            ax_memory::RememberInput {
                title: "raw prompt".into(),
                body: "a raw chat prompt, stored as a turn".into(),
                kind: Some(ax_memory::TURN_KIND.into()),
                tags: vec![],
                files: vec![],
                source: None,
            },
        )
        .await
        .unwrap();
        ax_memory::remember(
            &ws.graph_pool,
            ax_memory::RememberInput {
                title: "Use SQLite".into(),
                body: "SQLite is the truth, see [[global/skills/gl-skill]].".into(),
                kind: Some("decision".into()),
                tags: vec!["db".into()],
                files: vec![],
                source: None,
            },
        )
        .await
        .unwrap()
        .id
    };
    let pool = ax_global_db::open_and_init(&global).await.unwrap();
    let global_pid =
        ax_global_db::policy::ensure_project(&pool, &std::env::temp_dir().join("ax-links-api"))
            .await
            .unwrap();
    let raw = format!("---\nname: gl-skill\ndescription: g\ntags: [shared]\n---\n\n{GLOBAL_SKILL}");
    let doc = ax_policy::parse_skill_file(Path::new("SKILL.md"), &raw).unwrap();
    let flat = ax_global_db::policy::flatten_list_item(
        serde_json::to_value(&doc).unwrap(),
        PolicyKind::Skills,
        "gl-skill",
    );
    ax_global_db::policy::upsert_policy_item(
        &pool,
        global_pid,
        PolicyKind::Skills,
        "gl-skill",
        &flat,
    )
    .await
    .unwrap();
    pool.close().await;
    let hub = WebHub::open(root, readonly, 0).await.unwrap();
    let app = hub.nest_routers(
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any),
    );
    Fx {
        _dir: dir,
        app,
        memory_id,
        global_pid,
    }
}

async fn get(app: &Router, path: &str) -> (StatusCode, Value) {
    let res = app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
    (status, body)
}

#[tokio::test]
async fn a1_outgoing_links_are_parsed_and_resolved() {
    let _global = GLOBAL.lock().await;
    let fx = fixture_with(false).await;
    let (status, body) = get(&fx.app, "/api/links?kind=rule&id=review-rule").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["outgoing"],
        json!([
            {"text": "[[pr]]", "target": "pr", "heading": null, "label": null,
             "resolved": {"kind": "skill", "id": "pr", "origin": "project"}},
            {"text": "[[memories/Use SQLite|the DB note]]", "target": "memories/Use SQLite",
             "heading": null, "label": "the DB note",
             "resolved": {"kind": "memory", "id": fx.memory_id, "origin": "project"}},
            {"text": "[[nope#Part]]", "target": "nope", "heading": "Part", "label": null,
             "resolved": null},
        ])
    );
}

#[tokio::test]
async fn a1_global_items_carry_their_project_id() {
    let _global = GLOBAL.lock().await;
    let fx = fixture_with(false).await;
    let path = format!("/api/links?kind=memory&id={}", fx.memory_id);
    let (status, body) = get(&fx.app, &path).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["outgoing"][0]["resolved"],
        json!({"kind": "skill", "id": "gl-skill", "origin": "global", "projectId": fx.global_pid})
    );
    let (status, body) = get(&fx.app, "/api/links?kind=skill&id=gl-skill&origin=global").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let resolved: Vec<&Value> = body["outgoing"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| &l["resolved"])
        .collect();
    assert_eq!(
        resolved,
        vec![
            &json!({"kind": "rule", "id": "review-rule", "origin": "project"}),
            &json!({"kind": "skill", "id": "pr", "origin": "project"}),
        ],
        "no global skill is named pr, so [[pr]] is the project skill: {body}"
    );
}

#[tokio::test]
async fn a2_backlinks_cover_every_kind_sorted_by_kind_then_id() {
    let _global = GLOBAL.lock().await;
    let fx = fixture_with(false).await;
    let (_, body) = get(&fx.app, "/api/links?kind=rule&id=review-rule").await;
    assert_eq!(
        body["backlinks"],
        json!([
            {"kind": "skill", "id": "gl-skill", "title": "gl-skill", "origin": "global", "projectId": fx.global_pid},
            {"kind": "skill", "id": "pr", "title": "pr", "origin": "project"},
        ])
    );
    let path = format!("/api/links?kind=memory&id={}", fx.memory_id);
    let (_, body) = get(&fx.app, &path).await;
    assert_eq!(
        body["backlinks"],
        json!([{"kind": "rule", "id": "review-rule", "title": "review-rule", "origin": "project"}])
    );
    let (_, body) = get(&fx.app, "/api/links?kind=skill&id=gl-skill&origin=global").await;
    assert_eq!(
        body["backlinks"],
        json!([{"kind": "memory", "id": fx.memory_id, "title": "Use SQLite", "origin": "project"}])
    );
}

#[tokio::test]
async fn a3_unknown_item_is_404_and_bad_kind_is_400() {
    let _global = GLOBAL.lock().await;
    let fx = fixture_with(false).await;
    let (status, body) = get(&fx.app, "/api/links?kind=rule&id=missing").await;
    assert_eq!(
        (status, body),
        (StatusCode::NOT_FOUND, json!({"error": "not found"}))
    );
    let (status, _) = get(&fx.app, "/api/links?kind=skill&id=pr&origin=global").await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "pr is a project skill, not a global one"
    );
    let (status, _) = get(&fx.app, "/api/links?kind=page&id=pr").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = get(&fx.app, "/api/links?kind=rule").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a4_works_in_readonly_mode() {
    let _global = GLOBAL.lock().await;
    let fx = fixture_with(true).await;
    let (status, body) = get(&fx.app, "/api/links?kind=skill&id=pr").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["outgoing"][0]["resolved"]["id"], "review-rule");
}

#[tokio::test]
async fn g1_graph_lists_every_item_and_each_resolved_link() {
    let _global = GLOBAL.lock().await;
    let fx = fixture_with(true).await;
    let (status, body) = get(&fx.app, "/api/links/graph").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let mem = format!("memory:project:{}", fx.memory_id);
    let keys: Vec<&str> = body["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["key"].as_str().unwrap())
        .collect();
    for k in [
        "rule:project:review-rule",
        "skill:project:pr",
        mem.as_str(),
        "skill:global:gl-skill",
    ] {
        assert!(keys.contains(&k), "missing node {k}: {body}");
    }
    let memory_node = body["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["key"] == json!(mem))
        .unwrap();
    assert_eq!(memory_node["label"], "Use SQLite");
    assert_eq!(memory_node["kind"], "memory");
    assert_eq!(memory_node["target"], "memories/Use SQLite");
    let target_of = |key: &str| {
        body["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["key"] == json!(key))
            .unwrap()["target"]
            .clone()
    };
    assert_eq!(target_of("skill:project:pr"), "pr");
    assert_eq!(target_of("skill:global:gl-skill"), "gl-skill");
    let mut edges: Vec<(String, String)> = body["edges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["source"].as_str().unwrap().to_string(),
                e["target"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    edges.sort();
    let mut want = vec![
        (
            "rule:project:review-rule".to_string(),
            "skill:project:pr".to_string(),
        ),
        ("rule:project:review-rule".to_string(), mem.clone()),
        (
            "skill:project:pr".to_string(),
            "rule:project:review-rule".to_string(),
        ),
        (mem.clone(), "skill:global:gl-skill".to_string()),
        (
            "skill:global:gl-skill".to_string(),
            "rule:project:review-rule".to_string(),
        ),
        (
            "skill:global:gl-skill".to_string(),
            "skill:project:pr".to_string(),
        ),
    ];
    want.sort();
    assert_eq!(edges, want, "{body}");
}

#[tokio::test]
async fn g2_graph_leaves_out_turn_memories() {
    let _global = GLOBAL.lock().await;
    let fx = fixture_with(true).await;
    let (status, body) = get(&fx.app, "/api/links/graph").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let memories: Vec<&str> = body["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["kind"] == "memory")
        .map(|n| n["label"].as_str().unwrap())
        .collect();
    assert_eq!(memories, vec!["Use SQLite"], "{body}");
}

#[tokio::test]
async fn g3_graph_nodes_carry_tags() {
    let _global = GLOBAL.lock().await;
    let fx = fixture_with(true).await;
    let (status, body) = get(&fx.app, "/api/links/graph").await;
    assert_eq!(status, StatusCode::OK);
    let tags = |label: &str| -> Value {
        body["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["label"] == label)
            .unwrap_or_else(|| panic!("no node {label}"))["tags"]
            .clone()
    };
    assert_eq!(tags("review-rule"), json!(["review", "azure"]));
    assert_eq!(tags("pr"), json!(["git"]));
    assert_eq!(tags("Use SQLite"), json!(["db"]));
    assert_eq!(tags("gl-skill"), json!(["shared"]));
}
