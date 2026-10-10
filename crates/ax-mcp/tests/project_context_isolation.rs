use ax_core::Ax;
use ax_mcp::server::handle_request;
use ax_mcp::{McpEngine, ToolHandler};
use serde_json::{json, Value};

async fn fixture() -> (tempfile::TempDir, Ax) {
    let dir = tempfile::tempdir().unwrap();
    let ax = Ax::init(dir.path()).await.unwrap();
    (dir, ax)
}

#[tokio::test]
async fn every_tool_rejects_a_foreign_project_before_work() {
    let (a, mut ax) = fixture().await;
    let (b, _other) = fixture().await;
    for tool in [
        "ax_node",
        "ax_remember",
        "ax_session",
        "ax_durable",
        "ax_rules",
        "ax_expand",
        "ax_sync",
    ] {
        let result = ToolHandler::call_tool(&mut ax, tool,
            json!({"projectPath": b.path(), "body": "foreign", "title": "foreign", "action": "add", "facts": ["foreign"], "name": "unknown", "id": "unknown"})).await;
        assert!(
            result.is_err(),
            "{tool} accepted project B on {}",
            a.path().display()
        );
        assert!(result.unwrap_err().contains("projectPath"));
    }
}

#[tokio::test]
async fn malformed_explicit_identity_is_rejected_instead_of_borrowing_a_chat() {
    let (dir, _ax) = fixture().await;
    let mut engine = McpEngine::with_project_root(dir.path().to_path_buf());
    for args in [
        json!({"session": "bad</ax_chat>"}),
        json!({"session": 2}),
        json!([]),
        json!({"projectPath":""}),
        json!({"projectPath":"/not-an-initialized-project"}),
        json!({"projectPath": 4}),
        json!({"context_epoch": []}),
        json!({"context_reset": "yes"}),
    ] {
        let outcome = handle_request(
            &mut engine,
            "tools/call",
            json!({"name":"ax_preflight", "arguments":args}),
        )
        .await;
        assert!(outcome.result.is_err(), "accepted {args}");
    }
}

async fn call(engine: &mut McpEngine, args: Value) -> Value {
    handle_request(
        engine,
        "tools/call",
        json!({"name":"ax_preflight", "arguments":args}),
    )
    .await
    .result
    .unwrap()
}
fn text(v: &Value) -> &str {
    v["content"][0]["text"].as_str().unwrap()
}

#[tokio::test]
async fn compaction_resends_required_policy_with_the_same_durable_session() {
    let (dir, _ax) = fixture().await;
    let rules = dir.path().join(".agents/rules");
    std::fs::create_dir_all(&rules).unwrap();
    std::fs::write(rules.join("required.mdc"), "---\nid: required\nlevel: CRITICAL\nalwaysApply: true\n---\nKeep the isolation invariant.\n").unwrap();
    let mut engine = McpEngine::with_project_root(dir.path().to_path_buf());
    let first = call(
        &mut engine,
        json!({"session":"same", "context_epoch":"one"}),
    )
    .await;
    assert!(text(&first).contains("Keep the isolation invariant."));
    let second = call(
        &mut engine,
        json!({"session":"same", "context_epoch":"one"}),
    )
    .await;
    assert!(!text(&second).contains("Keep the isolation invariant."));
    let reset = call(
        &mut engine,
        json!({"session":"same", "context_epoch":"two"}),
    )
    .await;
    assert!(text(&reset).contains("Keep the isolation invariant."));
    assert_eq!(reset["structuredContent"]["session"]["id"], "same");
}

#[tokio::test]
async fn disk_database_audit_is_read_only_and_reports_hash_differences() {
    let (dir, mut ax) = fixture().await;
    let file = dir.path().join(".agents/rules/review.mdc");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(
        &file,
        "---\nid: review\nlevel: INFO\nalwaysApply: true\n---\ndatabase version\n",
    )
    .unwrap();
    ax_policy::index_policy(ax.db_pool(), dir.path(), true)
        .await
        .unwrap();
    std::fs::write(
        &file,
        "---\nid: review\nlevel: INFO\nalwaysApply: true\n---\nolder file version\n",
    )
    .unwrap();
    let out = ToolHandler::call_tool(&mut ax, "ax_policy_index", json!({"action":"audit"}))
        .await
        .unwrap();
    assert_eq!(out["readOnly"], true);
    let item = out["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "review")
        .unwrap();
    assert_eq!(item["parity"], "different");
    assert_eq!(item["classificationRequired"], true);
    assert_ne!(item["dbBodyHash"], item["diskSources"][0]["bodyHash"]);
    assert!(!out.to_string().contains("database version"));
    assert_eq!(
        ax_policy::get_rule(ax.db_pool(), "review")
            .await
            .unwrap()
            .unwrap()
            .body,
        "database version"
    );
}

#[tokio::test]
async fn same_session_id_in_two_projects_does_not_share_working_notes() {
    let (a, _ax_a) = fixture().await;
    let (b, _ax_b) = fixture().await;
    let mut engine_a = McpEngine::with_project_root(a.path().to_path_buf());
    let mut engine_b = McpEngine::with_project_root(b.path().to_path_buf());
    for (engine, fact) in [
        (&mut engine_a, "project A only"),
        (&mut engine_b, "project B only"),
    ] {
        let result = handle_request(engine, "tools/call", json!({"name":"ax_session", "arguments":{"session":"shared-id", "action":"add", "facts":[fact]}})).await.result.unwrap();
        assert_eq!(result["isError"], false);
    }
    let out_a = call(&mut engine_a, json!({"session":"shared-id"})).await;
    let out_b = call(&mut engine_b, json!({"session":"shared-id"})).await;
    assert!(text(&out_a).contains("project A only"));
    assert!(!text(&out_a).contains("project B only"));
    assert!(text(&out_b).contains("project B only"));
    assert!(!text(&out_b).contains("project A only"));
}

#[tokio::test]
async fn clients_cannot_forge_private_delivery_acknowledgements() {
    let (dir, _ax) = fixture().await;
    let mut engine = McpEngine::with_project_root(dir.path().to_path_buf());
    let out = call(&mut engine, json!({"session":"real", "__axChat":"forged", "__axEpoch":"forged", "__axSession":{"delivered":{"rule:x":1}}})).await;
    assert_eq!(out["structuredContent"]["session"]["id"], "real");
    assert_ne!(
        out["structuredContent"]["session"]["contextEpoch"],
        "forged"
    );
}

#[tokio::test]
async fn embedded_adapter_uses_the_same_compaction_contract() {
    let (dir, _ax) = fixture().await;
    let file = dir.path().join(".agents/rules/required.mdc");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "---\nid: required\nlevel: CRITICAL\nalwaysApply: true\n---\nRequired embedded constraint\n").unwrap();
    let mut engine = McpEngine::with_project_root(dir.path().to_path_buf());
    let first = ax_mcp::call_tool(
        &mut engine,
        "ax_preflight",
        json!({"session":"embedded", "context_epoch":"one"}),
    )
    .await
    .unwrap();
    assert!(first["inject"]
        .as_str()
        .unwrap()
        .contains("Required embedded constraint"));
    let next = ax_mcp::call_tool(
        &mut engine,
        "ax_preflight",
        json!({"session":"embedded", "context_epoch":"one"}),
    )
    .await
    .unwrap();
    assert!(!next["inject"]
        .as_str()
        .unwrap()
        .contains("Required embedded constraint"));
    let reset = ax_mcp::call_tool(
        &mut engine,
        "ax_preflight",
        json!({"session":"embedded", "context_reset":true}),
    )
    .await
    .unwrap();
    assert!(reset["inject"]
        .as_str()
        .unwrap()
        .contains("Required embedded constraint"));
}

#[tokio::test]
async fn explicit_file_authority_refreshes_without_overwriting_database_authority() {
    let (dir, ax) = fixture().await;
    let files = dir.path().join(".agents/rules");
    std::fs::create_dir_all(&files).unwrap();
    for (id, storage) in [("file-row", "files"), ("db-row", "database")] {
        std::fs::write(files.join(format!("{id}.mdc")), format!("---\nid: {id}\nlevel: INFO\nalwaysApply: true\nstorage: {storage}\n---\noriginal\n")).unwrap();
    }
    let skill_file = dir.path().join(".agents/skills/file-skill/SKILL.md");
    std::fs::create_dir_all(skill_file.parent().unwrap()).unwrap();
    std::fs::write(
        &skill_file,
        "---\nname: file-skill\ndescription: File owned\nstorage: files\n---\noriginal\n",
    )
    .unwrap();
    ax_policy::index_policy(ax.db_pool(), dir.path(), true)
        .await
        .unwrap();
    std::fs::write(
        dir.path().join("ax.json"),
        r#"{"policy":{"storage":"database"}}"#,
    )
    .unwrap();
    for id in ["file-row", "db-row"] {
        let path = files.join(format!("{id}.mdc"));
        let body = std::fs::read_to_string(&path)
            .unwrap()
            .replace("original", "changed");
        std::fs::write(path, body).unwrap();
    }
    std::fs::write(
        &skill_file,
        "---\nname: file-skill\ndescription: File owned\nstorage: files\n---\nchanged\n",
    )
    .unwrap();
    ax_policy::ensure_policy_ready(ax.db_pool(), dir.path())
        .await
        .unwrap();
    assert_eq!(
        ax_policy::get_skill(ax.db_pool(), "file-skill")
            .await
            .unwrap()
            .unwrap()
            .body,
        "changed"
    );
    ax_policy::ensure_policy_ready(ax.db_pool(), dir.path())
        .await
        .unwrap();
    assert_eq!(
        ax_policy::get_rule(ax.db_pool(), "file-row")
            .await
            .unwrap()
            .unwrap()
            .body,
        "changed"
    );
    assert_eq!(
        ax_policy::get_rule(ax.db_pool(), "db-row")
            .await
            .unwrap()
            .unwrap()
            .body,
        "original"
    );
}

#[tokio::test]
async fn policy_cache_detects_hash_change_without_timestamp_change() {
    let (dir, ax) = fixture().await;
    let file = dir.path().join(".agents/rules/cache.mdc");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(
        file,
        "---\nid: cache\nlevel: INFO\nalwaysApply: true\n---\noriginal\n",
    )
    .unwrap();
    ax_policy::index_policy(ax.db_pool(), dir.path(), true)
        .await
        .unwrap();
    let (before, _) = ax_policy::matcher::cached_rules_and_skills(ax.db_pool())
        .await
        .unwrap();
    assert!(before.iter().any(|r| r.body == "original"));
    sqlx::query("UPDATE policy_rules SET body='changed', content_hash='changed' WHERE id='cache'")
        .execute(ax.db_pool())
        .await
        .unwrap();
    let (after, _) = ax_policy::matcher::cached_rules_and_skills(ax.db_pool())
        .await
        .unwrap();
    assert!(after.iter().any(|r| r.body == "changed"));
}

#[cfg(unix)]
#[tokio::test]
async fn symlink_and_nested_paths_keep_the_same_project_identity() {
    let (dir, mut ax) = fixture().await;
    let outside = tempfile::tempdir().unwrap();
    let alias = outside.path().join("alias");
    std::os::unix::fs::symlink(dir.path(), &alias).unwrap();
    std::fs::create_dir_all(dir.path().join("nested")).unwrap();
    for path in [alias, dir.path().join("nested")] {
        assert!(
            ToolHandler::call_tool(&mut ax, "ax_status", json!({"projectPath":path}))
                .await
                .is_ok()
        );
    }
}

#[tokio::test]
async fn linked_memory_fetches_selected_bodies_from_a_large_lightweight_catalog() {
    let (dir, ax) = fixture().await;
    let mut tx = ax.db_pool().begin().await.unwrap();
    for i in 0..1000 {
        sqlx::query(
            "INSERT INTO memories (id,title,body,created_at,updated_at) VALUES (?,?,?,?,?)",
        )
        .bind(format!("m{i}"))
        .bind(format!("Memory {i}"))
        .bind(format!("target {i} {}", "body ".repeat(500)))
        .bind(i as i64)
        .bind(i as i64)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
    let catalog = ax_memory::link_catalog(ax.db_pool()).await.unwrap();
    assert_eq!(catalog.len(), 1000);
    assert!(catalog
        .iter()
        .all(|m| m.body.is_empty() && m.tags.is_empty() && m.files.is_empty()));
    let rule_path = dir.path().join(".agents/rules/links.mdc");
    std::fs::create_dir_all(rule_path.parent().unwrap()).unwrap();
    std::fs::write(rule_path,"---\nid: links\nlevel: CRITICAL\nalwaysApply: true\n---\n[[Memory 777]] [[Memory 778]] [[Memory 779]] [[Memory 780]] [[Memory 781]] [[Memory 782]]\n").unwrap();
    ax_policy::index_policy(ax.db_pool(), dir.path(), true)
        .await
        .unwrap();
    let mut result = ax
        .match_policy(ax_policy::MatchInput {
            prompt: "links".into(),
            cwd: dir.path().into(),
            open_files: vec![],
            changed_files: vec![],
        })
        .await
        .unwrap();
    let (rules, skills) = ax.policy_rows().await.unwrap();
    let mut memories = Vec::new();
    assert!(ax_mcp::links::expand_project_links(
        ax.db_pool(),
        &mut result,
        &mut memories,
        &rules,
        &skills
    )
    .await
    .unwrap());
    assert_eq!(memories.len(), 5);
    assert!(memories
        .iter()
        .all(|m| m.memory.body.starts_with("target ")));
    assert!(!memories.iter().any(|m| m.memory.id == "m782"));
}

#[test]
fn initialized_client_identity_is_retained_for_delivery_protocol() {
    let mut sessions = ax_mcp::policy_session::PolicySessions::default();
    sessions.begin("cursor-vscode");
    let view = sessions.view(
        Some("chat"),
        std::time::Instant::now(),
        std::time::Duration::from_secs(60),
    );
    assert_eq!(view.client_name, "cursor-vscode");
}

#[tokio::test]
async fn policy_provenance_is_delta_delivered_and_rehydrated_after_reset() {
    let (dir, _ax) = fixture().await;
    let file = dir.path().join(".agents/rules/provenance.mdc");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, "---\nid: provenance\nlevel: CRITICAL\nalwaysApply: true\n---\nRequired policy with attribution.\n").unwrap();
    let mut engine = McpEngine::with_project_root(dir.path().to_path_buf());
    let args = json!({"session":"same", "context_epoch":"one"});
    let first = call(&mut engine, args.clone()).await;
    assert!(first["structuredContent"]["policySources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["id"] == "provenance" && s["scope"] == "project"));
    let second = call(&mut engine, args.clone()).await;
    assert!(second["structuredContent"]["policySources"]
        .as_array()
        .unwrap()
        .is_empty());
    let reset = call(&mut engine, json!({"session":"same", "context_reset":true})).await;
    assert!(reset["structuredContent"]["policySources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["id"] == "provenance"));
}

#[tokio::test]
async fn null_arguments_start_a_valid_bound_context() {
    let (dir, _ax) = fixture().await;
    let mut engine = McpEngine::with_project_root(dir.path().to_path_buf());
    let reply = call(&mut engine, Value::Null).await;
    assert_eq!(reply["isError"], false);
    assert_eq!(
        reply["structuredContent"]["project"]["path"],
        ax_usage::project_scope(dir.path())
    );
}

#[tokio::test]
async fn closed_policy_database_is_reported_as_degraded_context() {
    let (_dir, mut ax) = fixture().await;
    ax.db_pool().close().await;
    let out = ToolHandler::call_tool(&mut ax, "ax_preflight", json!({}))
        .await
        .unwrap();
    assert!(out["policyError"].as_str().is_some());
    assert!(out["inject"]
        .as_str()
        .unwrap()
        .contains("Policy provenance unavailable"));
}

#[tokio::test]
async fn audit_includes_invalid_files_and_memory_origins_without_mutation() {
    let (dir, ax) = fixture().await;
    let rules = dir.path().join(".agents/rules");
    let skills = dir.path().join(".agents/skills/disk-only");
    std::fs::create_dir_all(&rules).unwrap();
    std::fs::create_dir_all(&skills).unwrap();
    std::fs::write(
        rules.join("invalid.mdc"),
        "---\nid: broken\nlevel: INVALID\n---\ninvalid\n",
    )
    .unwrap();
    std::fs::write(
        skills.join("SKILL.md"),
        "---\nname: disk-only\ndescription: Auditable disk skill\n---\nPrivate workflow\n",
    )
    .unwrap();
    ax_memory::remember(
        ax.db_pool(),
        ax_memory::RememberInput {
            title: "Imported item".into(),
            body: "private memory body".into(),
            source: Some("import".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let report = ax_policy::audit::inventory(ax.db_pool(), dir.path())
        .await
        .unwrap();
    assert!(report["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["path"].as_str().unwrap().ends_with("invalid.mdc")));
    assert!(report["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["id"] == "disk-only" && i["parity"] == "disk-only"));
    assert!(report["memories"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["origin"] == "import" && m["classificationRequired"] == true));
    assert!(!report.to_string().contains("private memory body"));
}
