use std::path::Path;

use serde_json::json;
use sqlx::SqlitePool;

use crate::policy::{self, PolicyKind};
use crate::{open_and_init, open_pool, sync};

async fn project_with_skills(root: &Path, skills: &[(&str, &str)]) {
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let p = open_pool(&root.join(".ax").join("ax.db"), true)
        .await
        .unwrap();
    for ddl in [
        "CREATE TABLE nodes (id TEXT PRIMARY KEY, kind TEXT NOT NULL, name TEXT NOT NULL,
            qualified_name TEXT NOT NULL, file_path TEXT NOT NULL, language TEXT NOT NULL,
            start_line INTEGER NOT NULL, end_line INTEGER NOT NULL, start_column INTEGER NOT NULL,
            end_column INTEGER NOT NULL, updated_at INTEGER NOT NULL)",
        "CREATE TABLE files (path TEXT PRIMARY KEY, content_hash TEXT NOT NULL, language TEXT NOT NULL,
            size INTEGER NOT NULL, modified_at INTEGER NOT NULL, indexed_at INTEGER NOT NULL,
            node_count INTEGER DEFAULT 0)",
        "CREATE TABLE edges (id INTEGER PRIMARY KEY AUTOINCREMENT, source TEXT NOT NULL,
            target TEXT NOT NULL, kind TEXT NOT NULL)",
        "CREATE TABLE policy_skills (name TEXT PRIMARY KEY, description TEXT NOT NULL,
            always_apply INTEGER NOT NULL DEFAULT 0, triggers TEXT NOT NULL DEFAULT '[]',
            tags TEXT NOT NULL DEFAULT '[]', priority INTEGER NOT NULL DEFAULT 50, body TEXT NOT NULL,
            source_path TEXT NOT NULL, content_hash TEXT NOT NULL, updated_at INTEGER NOT NULL)",
    ] {
        sqlx::query(ddl).execute(&p).await.unwrap();
    }
    for (name, body) in skills {
        sqlx::query(
            "INSERT INTO policy_skills (name, description, body, source_path, content_hash, updated_at)
             VALUES (?, 'd', ?, 's/SKILL.md', 'h', 0)",
        )
        .bind(name)
        .bind(body)
        .execute(&p)
        .await
        .unwrap();
    }
    p.close().await;
}

async fn level_of(g: &SqlitePool, item_id: &str) -> Vec<(i64, String)> {
    sqlx::query_as(
        "SELECT project_id, level FROM global_policy_skills WHERE item_id = ? ORDER BY project_id",
    )
    .bind(item_id)
    .fetch_all(g)
    .await
    .unwrap()
}

#[tokio::test]
async fn existing_global_rows_become_level_global() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("global.db");
    let old = open_pool(&path, true).await.unwrap();
    sqlx::query(
        "CREATE TABLE global_policy_skills (project_id INTEGER NOT NULL, item_id TEXT NOT NULL,
            payload TEXT NOT NULL, synced_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (project_id, item_id))",
    )
    .execute(&old)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO global_policy_skills (project_id, item_id, payload) VALUES (1, 'noti', '{}')",
    )
    .execute(&old)
    .await
    .unwrap();
    old.close().await;

    let g = open_and_init(&path).await.unwrap();
    assert_eq!(level_of(&g, "noti").await, vec![(1, "global".to_string())]);
    let rules_level: Vec<(String,)> = sqlx::query_as(
        "SELECT name FROM pragma_table_info('global_policy_rules') WHERE name = 'level'",
    )
    .fetch_all(&g)
    .await
    .unwrap();
    assert_eq!(rules_level.len(), 1);
    assert!(policy::list_revisions(&g, PolicyKind::Skills, "noti")
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn global_sync_writes_mirrors_and_skips_global_names() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("proj");
    project_with_skills(
        &project,
        &[("noti", "project copy"), ("local-only", "mine")],
    )
    .await;
    let g = open_and_init(&dir.path().join("global.db")).await.unwrap();
    let machine = policy::ensure_project(&g, &dir.path().join("machine"))
        .await
        .unwrap();
    policy::upsert_policy_item(
        &g,
        machine,
        PolicyKind::Skills,
        "noti",
        &json!({"name":"noti","body":"global copy"}),
    )
    .await
    .unwrap();

    sync::sync_project(&g, &project).await.unwrap();

    assert_eq!(
        level_of(&g, "noti").await,
        vec![(machine, "global".to_string())]
    );
    let local = level_of(&g, "local-only").await;
    assert_eq!(local.len(), 1);
    assert_eq!(local[0].1, "mirror");
    let loaded: Vec<String> = policy::list_skill_payloads(&g)
        .await
        .unwrap()
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(loaded, vec!["noti".to_string()]);
}

#[tokio::test]
async fn global_revisions_count_up_and_cap_at_20() {
    let dir = tempfile::tempdir().unwrap();
    let g = open_and_init(&dir.path().join("global.db")).await.unwrap();
    let pid = policy::ensure_project(&g, &dir.path().join("machine"))
        .await
        .unwrap();
    for i in 1..=25 {
        policy::upsert_policy_item(
            &g,
            pid,
            PolicyKind::Skills,
            "noti",
            &json!({"name":"noti","body":format!("v{i}")}),
        )
        .await
        .unwrap();
    }
    policy::upsert_policy_item(
        &g,
        pid,
        PolicyKind::Skills,
        "noti",
        &json!({"name":"noti","body":"v25"}),
    )
    .await
    .unwrap();

    let revs = policy::list_revisions(&g, PolicyKind::Skills, "noti")
        .await
        .unwrap();
    let versions: Vec<i64> = revs.iter().map(|r| r.version).collect();
    assert_eq!(versions, (6..=25).rev().collect::<Vec<i64>>());
    assert_eq!(revs[0].payload["body"], "v25");
    assert_eq!(revs[0].source, "save");
}

#[tokio::test]
async fn a_row_without_history_keeps_its_old_body_as_version_1() {
    let dir = tempfile::tempdir().unwrap();
    let g = open_and_init(&dir.path().join("global.db")).await.unwrap();
    let pid = policy::ensure_project(&g, &dir.path().join("machine"))
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO global_policy_skills (project_id, item_id, payload) VALUES (?, 'noti', ?)",
    )
    .bind(pid)
    .bind(json!({"name":"noti","body":"old"}).to_string())
    .execute(&g)
    .await
    .unwrap();
    assert_eq!(
        policy::next_version(&g, PolicyKind::Skills, "noti")
            .await
            .unwrap(),
        2
    );

    let version = policy::upsert_policy_item_from(
        &g,
        pid,
        PolicyKind::Skills,
        "noti",
        &json!({"name":"noti","body":"new"}),
        "dedup-promote",
    )
    .await
    .unwrap();

    assert_eq!(version, Some(2));
    let revs = policy::list_revisions(&g, PolicyKind::Skills, "noti")
        .await
        .unwrap();
    let got: Vec<_> = revs
        .iter()
        .map(|r| {
            (
                r.version,
                r.source.as_str(),
                r.payload["body"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![(2, "dedup-promote", "new"), (1, "baseline", "old")]
    );
}

#[tokio::test]
async fn deleting_a_global_row_records_a_revision() {
    let dir = tempfile::tempdir().unwrap();
    let g = open_and_init(&dir.path().join("global.db")).await.unwrap();
    let pid = policy::ensure_project(&g, &dir.path().join("machine"))
        .await
        .unwrap();
    policy::upsert_policy_item(
        &g,
        pid,
        PolicyKind::Rules,
        "utf8",
        &json!({"id":"utf8","body":"x"}),
    )
    .await
    .unwrap();

    assert!(
        policy::delete_policy_item(&g, pid, PolicyKind::Rules, "utf8")
            .await
            .unwrap()
    );

    let revs = policy::list_revisions(&g, PolicyKind::Rules, "utf8")
        .await
        .unwrap();
    assert_eq!(
        revs.iter().map(|r| r.version).collect::<Vec<_>>(),
        vec![2, 1]
    );
    assert_eq!(revs[0].source, "delete");
    assert_eq!(revs[0].payload["body"], "x");
}
