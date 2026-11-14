//! One memory, three doors: the library, the CLI, and the MCP tool.

use std::process::Command;

use serde_json::Value;

fn ax_bin() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_ax")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let mut path = std::env::current_exe().unwrap();
            path.pop();
            if path.ends_with("deps") {
                path.pop();
            }
            path.join("ax")
        })
}

fn run_ax(root: &std::path::Path, args: &[&str]) -> Value {
    let out = Command::new(ax_bin())
        .current_dir(root)
        .args(args)
        .output()
        .expect("spawn ax");
    assert!(
        out.status.success(),
        "ax {args:?}\nstdout {}\nstderr {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|error| panic!("{error}: {}", String::from_utf8_lossy(&out.stdout)))
}

#[tokio::test]
async fn cue_lookup_matches_across_library_cli_and_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let init = Command::new(ax_bin())
        .current_dir(root)
        .args(["init"])
        .output()
        .expect("spawn ax init");
    assert!(
        init.status.success(),
        "ax init\nstdout {}\nstderr {}",
        String::from_utf8_lossy(&init.stdout),
        String::from_utf8_lossy(&init.stderr)
    );
    let stored = run_ax(
        root,
        &[
            "remember",
            "Copy every ax binary after a build.",
            "--title",
            "Ship together",
            "--trigger",
            "reinstall-all",
            "--json",
        ],
    );
    let id = stored["id"].as_str().unwrap().to_string();

    let cli = run_ax(root, &["recall", "--cue", "reinstall-all", "--json"]);
    let cli_id = cli["matches"][0]["id"].as_str().unwrap();
    let cli_score = cli["matches"][0]["score"].as_f64().unwrap();
    assert_eq!(cli_id, id);

    let db = ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let library = ax_memory::search(
        db.pool(),
        &ax_memory::SearchRequest {
            cue: Some("reinstall-all".into()),
            limit: 3,
            ..ax_memory::SearchRequest::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(library[0].memory.id, id);
    assert!((library[0].score - cli_score).abs() < 0.001);

    let mut engine = ax_mcp::McpEngine::with_project_root(root.to_path_buf());
    let mcp = ax_mcp::call_tool(
        &mut engine,
        "ax_recall",
        serde_json::json!({ "cue": "reinstall-all", "limit": 3 }),
    )
    .await
    .unwrap();
    let mcp_id = mcp["matches"][0]["id"].as_str().unwrap();
    let mcp_score = mcp["matches"][0]["score"].as_f64().unwrap();
    assert_eq!(mcp_id, id);
    assert!((mcp_score - cli_score).abs() < 0.001);
}

const DAY_MS: i64 = 86_400_000;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

async fn init_project(root: &std::path::Path) -> ax_db::Database {
    let init = Command::new(ax_bin())
        .current_dir(root)
        .args(["init"])
        .output()
        .expect("spawn ax init");
    assert!(
        init.status.success(),
        "ax init\nstderr {}",
        String::from_utf8_lossy(&init.stderr)
    );
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap()
}

/// A fresh project's required policy alone exceeds the default preflight budget,
/// which would drop the memory block and make these checks vacuous.
fn room_for_memories(root: &std::path::Path) {
    let config_path = root.join("ax.json");
    let mut config: Value = std::fs::read_to_string(&config_path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    config["context"]["budgetTokens"] = serde_json::json!(200_000);
    std::fs::write(&config_path, config.to_string()).unwrap();
}

async fn mcp(root: &std::path::Path, tool: &str, args: Value) -> Value {
    let mut engine = ax_mcp::McpEngine::with_project_root(root.to_path_buf());
    ax_mcp::call_tool(&mut engine, tool, args).await.unwrap()
}

fn ids(value: &Value, list: &str) -> Vec<String> {
    value[list]
        .as_array()
        .unwrap_or_else(|| panic!("no {list} in {value}"))
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_string())
        .collect()
}

fn scores(value: &Value) -> Vec<f64> {
    value["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["score"].as_f64().unwrap())
        .collect()
}

async fn demoted_turn(db: &ax_db::Database, root: &std::path::Path) {
    ax_memory::save_turn(
        db.pool(),
        &ax_memory::TurnRecord {
            id: "turn-91".into(),
            title: "Retention policy".into(),
            body: "Retention policy\nfiles: src/cache.rs".into(),
            files: vec!["src/cache.rs".into()],
        },
        now_ms() - 91 * DAY_MS,
    )
    .await
    .unwrap();
    let pruned = ax_memory::prune_turns(db.pool(), now_ms(), 90, &root.join("backups"))
        .await
        .unwrap();
    assert_eq!(pruned, 1);
}

#[tokio::test]
async fn a_demoted_turn_is_fetchable_by_id_and_cue_on_every_door() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let db = init_project(root).await;
    demoted_turn(&db, root).await;
    room_for_memories(root);

    let preflight = mcp(
        root,
        "ax_preflight",
        serde_json::json!({ "prompt": "Retention policy" }),
    )
    .await;
    let omitted = &preflight["contextBudget"]["omitted"];
    assert!(!omitted.to_string().contains("memories"), "{omitted}");
    assert!(!preflight.to_string().contains("turn-91"), "{preflight}");

    let library = ax_memory::search(
        db.pool(),
        &ax_memory::SearchRequest {
            id: Some("turn-91".into()),
            ..ax_memory::SearchRequest::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(library[0].memory.id, "turn-91");
    assert_eq!(library[0].memory.tier, "t3");

    let cli = run_ax(root, &["recall", "--id", "turn-91", "--json"]);
    let by_id = mcp(root, "ax_recall", serde_json::json!({ "id": "turn-91" })).await;
    assert_eq!(ids(&cli, "matches"), vec!["turn-91"]);
    assert_eq!(ids(&by_id, "matches"), vec!["turn-91"]);
    assert_eq!(cli["matches"][0]["tier"], "t3");
    assert_eq!(by_id["matches"][0]["tier"], "t3");
    assert_eq!(cli["cues"], by_id["cues"]);

    let cli_cue = run_ax(root, &["recall", "--cue", "retention-policy", "--json"]);
    let mcp_cue = mcp(
        root,
        "ax_recall",
        serde_json::json!({ "cue": "retention-policy" }),
    )
    .await;
    assert_eq!(ids(&cli_cue, "matches"), vec!["turn-91"]);
    assert_eq!(ids(&mcp_cue, "matches"), vec!["turn-91"]);
}

#[tokio::test]
async fn archive_recall_returns_the_same_ids_and_scores_on_every_door() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let db = init_project(root).await;
    demoted_turn(&db, root).await;
    ax_memory::remember(
        db.pool(),
        ax_memory::RememberInput {
            title: "Retention hot note".into(),
            body: "Retention policy for the hot set.".into(),
            kind: Some("decision".into()),
            tags: vec![],
            files: vec![],
            source: Some("test".into()),
        },
    )
    .await
    .unwrap();

    let library = ax_memory::search(
        db.pool(),
        &ax_memory::SearchRequest {
            query: "Retention".into(),
            depth: ax_memory::Depth::Archive,
            limit: 5,
            ..ax_memory::SearchRequest::default()
        },
    )
    .await
    .unwrap();
    let library_ids: Vec<String> = library.iter().map(|hit| hit.memory.id.clone()).collect();
    assert!(
        library_ids.contains(&"turn-91".to_string()),
        "{library_ids:?}"
    );
    assert!(library.iter().all(|hit| hit.memory.tier == "t3"));

    let cli = run_ax(
        root,
        &["recall", "Retention", "--depth", "archive", "--json"],
    );
    let archive = mcp(
        root,
        "ax_recall",
        serde_json::json!({ "query": "Retention", "depth": "archive", "limit": 5 }),
    )
    .await;
    assert_eq!(ids(&cli, "matches"), library_ids);
    assert_eq!(ids(&archive, "matches"), library_ids);
    for ((cli_score, mcp_score), hit) in scores(&cli).iter().zip(scores(&archive)).zip(&library) {
        assert!((cli_score - hit.score).abs() < 0.001);
        assert!((mcp_score - hit.score).abs() < 0.001);
    }
}

#[tokio::test]
async fn archive_history_lists_the_same_graph_changes_on_every_door() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let db = init_project(root).await;
    sqlx::query(
        "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, signature, updated_at)
         VALUES ('sym-1', 'function', 'save_turn', 'turns::save_turn', 'src/turns.rs', 'rust', 1, 4, 0, 1, 'fn save_turn()', 1)",
    )
    .execute(db.pool())
    .await
    .unwrap();
    ax_memory::capture_graph_episodes(db.pool()).await.unwrap();
    sqlx::query("UPDATE nodes SET signature = 'fn save_turn(now: i64)' WHERE id = 'sym-1'")
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(
        ax_memory::capture_graph_episodes(db.pool()).await.unwrap(),
        1
    );

    let library = ax_memory::history(
        db.pool(),
        root,
        &ax_memory::HistoryQuery {
            query: "save_turn".into(),
            since_ms: None,
            limit: 20,
            archive: true,
        },
    )
    .await
    .unwrap();
    let library_ids: Vec<String> = library.iter().map(|entry| entry.id.clone()).collect();
    assert!(
        library_ids.iter().any(|id| id.starts_with("graph-")),
        "{library_ids:?}"
    );

    let cli = run_ax(
        root,
        &["history", "save_turn", "--depth", "archive", "--json"],
    );
    let tool = mcp(
        root,
        "ax_history",
        serde_json::json!({ "query": "save_turn", "depth": "archive" }),
    )
    .await;
    assert_eq!(ids(&cli, "entries"), library_ids);
    assert_eq!(ids(&tool, "entries"), library_ids);

    let hot = run_ax(root, &["history", "save_turn", "--json"]);
    assert!(ids(&hot, "entries")
        .iter()
        .all(|id| !id.starts_with("graph-")));
}

async fn candidate(db: &ax_db::Database, turn: &str) -> String {
    ax_memory::save_turn(
        db.pool(),
        &ax_memory::TurnRecord {
            id: turn.into(),
            title: format!("Use cues {turn}"),
            body: format!("We decided {turn} fetches by cue because names are exact."),
            files: vec![],
        },
        now_ms(),
    )
    .await
    .unwrap();
    ax_memory::consolidate(db.pool()).await.unwrap();
    sqlx::query_scalar(
        "SELECT id FROM memories WHERE source = 'consolidation' AND status = 'candidate' ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_one(db.pool())
    .await
    .unwrap()
}

async fn status(db: &ax_db::Database, id: &str) -> String {
    sqlx::query_scalar("SELECT status FROM memories WHERE id = ?")
        .bind(id)
        .fetch_one(db.pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn confirm_activates_a_candidate_from_every_door() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let db = init_project(root).await;

    let first = candidate(&db, "turn-cli").await;
    assert_eq!(status(&db, &first).await, "candidate");
    let cli = run_ax(root, &["remember", "--confirm", &first, "--json"]);
    assert_eq!(
        cli,
        serde_json::json!({ "ok": true, "id": first, "confirmed": true })
    );
    assert_eq!(status(&db, &first).await, "active");

    let second = candidate(&db, "turn-mcp").await;
    assert_ne!(second, first);
    let tool = mcp(
        root,
        "ax_remember",
        serde_json::json!({ "action": "confirm", "id": second }),
    )
    .await;
    assert_eq!(
        tool,
        serde_json::json!({ "ok": true, "id": second, "confirmed": true })
    );
    assert_eq!(status(&db, &second).await, "active");

    let third = candidate(&db, "turn-lib").await;
    assert!(ax_memory::confirm_memory(db.pool(), &third).await.unwrap());
    assert_eq!(status(&db, &third).await, "active");
}

fn first_position(text: &str, needles: &[&str]) -> Vec<Option<usize>> {
    needles.iter().map(|needle| text.find(needle)).collect()
}

#[tokio::test]
async fn preflight_lists_the_cue_hit_first_on_cli_and_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let db = init_project(root).await;
    ax_memory::remember(
        db.pool(),
        ax_memory::RememberInput {
            title: "Cache eviction notes".into(),
            body: "The build cache is slow. Cache eviction keeps the build cache small.".into(),
            kind: Some("note".into()),
            tags: vec![],
            files: vec![],
            source: Some("test".into()),
        },
    )
    .await
    .unwrap();
    ax_memory::remember_with_triggers(
        db.pool(),
        ax_memory::RememberInput {
            title: "Ship together".into(),
            body: "Copy every ax binary after a build.".into(),
            kind: Some("decision".into()),
            tags: vec![],
            files: vec![],
            source: Some("test".into()),
        },
        &["reinstall-all".to_string()],
    )
    .await
    .unwrap();

    room_for_memories(root);
    let prompt = "reinstall-all after the build cache is slow and cache eviction";
    let cli = run_ax(root, &["preflight", "--prompt", prompt, "--json"]).to_string();
    let tool = mcp(
        root,
        "ax_preflight",
        serde_json::json!({ "prompt": prompt }),
    )
    .await
    .to_string();
    for text in [&cli, &tool] {
        let found = first_position(text, &["Ship together", "Cache eviction notes"]);
        let (Some(cue), Some(other)) = (found[0], found[1]) else {
            panic!("both memories should be listed: {text}");
        };
        assert!(cue < other, "the cue hit must come first: {text}");
    }
}
