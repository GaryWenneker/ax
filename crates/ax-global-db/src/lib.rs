//! Global multi-tenant database (`~/.ax/global.db`).

pub mod database;
pub mod graph;
pub mod policy;
pub mod sync;

#[cfg(test)]
mod policy_level_tests;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

use ax_db::{busy_timeout, connect_options};

pub use ax_utils::paths::AX_GLOBAL_DB_ENV;

pub fn global_db_path() -> Result<PathBuf> {
    ax_utils::paths::resolve_global_db_path(dirs::home_dir())
        .context("HOME not set; cannot resolve ~/.ax/global.db")
}

pub async fn open_pool(path: &Path, create_if_missing: bool) -> Result<SqlitePool> {
    if create_if_missing {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
        }
    }
    let options = connect_options(path, create_if_missing);
    let timeout = busy_timeout();
    let pool = SqlitePoolOptions::new()
        .max_connections(3)
        .acquire_timeout(timeout)
        .connect_with(options)
        .await
        .with_context(|| format!("open sqlite {}", path.display()))?;
    Ok(pool)
}

pub async fn open_and_init(path: &Path) -> Result<SqlitePool> {
    let pool = open_pool(path, true).await?;
    database::initialize_schema(&pool).await?;
    Ok(pool)
}

pub async fn init_default() -> Result<PathBuf> {
    let path = global_db_path()?;
    let pool = open_and_init(&path).await?;
    pool.close().await;
    Ok(path)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GlobalStatus {
    pub path: String,
    pub project_count: i64,
    pub node_count: i64,
    pub document_count: i64,
    pub shared_knowledge_count: i64,
}

pub async fn status_at(path: &Path) -> Result<GlobalStatus> {
    let pool = open_and_init(path).await?;
    let project_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM projects")
        .fetch_one(&pool)
        .await?;
    let node_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM global_nodes")
        .fetch_one(&pool)
        .await?;
    let document_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM global_documents")
        .fetch_one(&pool)
        .await?;
    let shared_knowledge_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM shared_knowledge")
        .fetch_one(&pool)
        .await?;
    pool.close().await;
    Ok(GlobalStatus {
        path: path.display().to_string(),
        project_count: project_count.0,
        node_count: node_count.0,
        document_count: document_count.0,
        shared_knowledge_count: shared_knowledge_count.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    async fn project_pool(dir: &Path) -> SqlitePool {
        let ax_dir = dir.join(".ax");
        std::fs::create_dir_all(&ax_dir).unwrap();
        let db = ax_dir.join("ax.db");
        let pool = open_pool(&db, true).await.unwrap();
        sqlx::query(
            "CREATE TABLE nodes (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                name TEXT NOT NULL,
                qualified_name TEXT NOT NULL,
                file_path TEXT NOT NULL,
                language TEXT NOT NULL,
                start_line INTEGER NOT NULL,
                end_line INTEGER NOT NULL,
                start_column INTEGER NOT NULL,
                end_column INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE files (
                path TEXT PRIMARY KEY,
                content_hash TEXT NOT NULL,
                language TEXT NOT NULL,
                size INTEGER NOT NULL,
                modified_at INTEGER NOT NULL,
                indexed_at INTEGER NOT NULL,
                node_count INTEGER DEFAULT 0
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE edges (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source TEXT NOT NULL,
                target TEXT NOT NULL,
                kind TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn g1_init_creates_tables() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("global.db");
        let pool = open_and_init(&path).await.unwrap();
        let names: Vec<(String,)> =
            sqlx::query_as("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .fetch_all(&pool)
                .await
                .unwrap();
        let set: Vec<String> = names.into_iter().map(|r| r.0).collect();
        for need in [
            "projects",
            "global_nodes",
            "global_documents",
            "global_edges",
            "cross_project_refs",
            "shared_knowledge",
            "global_policy_rules",
            "global_policy_skills",
        ] {
            assert!(set.iter().any(|n| n == need), "missing table {need}: {set:?}");
        }
    }

    #[tokio::test]
    async fn g2_sync_copies_node() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("proj");
        std::fs::create_dir_all(&project).unwrap();
        let p = project_pool(&project).await;
        sqlx::query(
            "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, updated_at)
             VALUES ('n1', 'function', 'Foo', 'Foo', 'src/lib.rs', 'rust', 1, 2, 0, 1, 0)",
        )
        .execute(&p)
        .await
        .unwrap();
        p.close().await;

        let global = dir.path().join("global.db");
        let g = open_and_init(&global).await.unwrap();
        let result = sync::sync_project(&g, &project).await.unwrap();
        assert_eq!(result.synced_count, 1);
        let name: (String,) = sqlx::query_as("SELECT name FROM global_nodes")
            .fetch_one(&g)
            .await
            .unwrap();
        assert_eq!(name.0, "Foo");
    }

    #[tokio::test]
    async fn g3_shared_knowledge_from_same_file_hash() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["a", "b"] {
            let project = dir.path().join(name);
            std::fs::create_dir_all(&project).unwrap();
            let p = project_pool(&project).await;
            sqlx::query(
                "INSERT INTO files (path, content_hash, language, size, modified_at, indexed_at)
                 VALUES ('src/dup.rs', 'abc123', 'rust', 10, 0, 0)",
            )
            .execute(&p)
            .await
            .unwrap();
            p.close().await;
        }
        let global = dir.path().join("global.db");
        let g = open_and_init(&global).await.unwrap();
        sync::sync_project(&g, &dir.path().join("a")).await.unwrap();
        sync::sync_project(&g, &dir.path().join("b")).await.unwrap();
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM shared_knowledge")
            .fetch_one(&g)
            .await
            .unwrap();
        assert_eq!(count.0, 1);
        let projects: (String,) = sqlx::query_as("SELECT projects FROM shared_knowledge")
            .fetch_one(&g)
            .await
            .unwrap();
        assert!(projects.0.contains("a"));
        assert!(projects.0.contains("b"));
    }

    #[tokio::test]
    async fn g4_missing_ax_db_errors() {
        let dir = tempfile::tempdir().unwrap();
        let global = dir.path().join("global.db");
        let g = open_and_init(&global).await.unwrap();
        let err = sync::sync_project(&g, dir.path()).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("ax.db"), "{msg}");
    }

    #[tokio::test]
    async fn g6_graph_slice_marks_selected_and_shared() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["a", "b"] {
            let project = dir.path().join(name);
            std::fs::create_dir_all(&project).unwrap();
            let p = project_pool(&project).await;
            sqlx::query(
                "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, updated_at)
                 VALUES ('n1', 'function', 'Foo', 'Foo', 'src/dup.rs', 'rust', 1, 2, 0, 1, 0)",
            )
            .execute(&p)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO files (path, content_hash, language, size, modified_at, indexed_at)
                 VALUES ('src/dup.rs', 'same', 'rust', 10, 0, 0)",
            )
            .execute(&p)
            .await
            .unwrap();
            p.close().await;
        }
        let global = dir.path().join("global.db");
        let g = open_and_init(&global).await.unwrap();
        sync::sync_project(&g, &dir.path().join("a")).await.unwrap();
        sync::sync_project(&g, &dir.path().join("b")).await.unwrap();
        let slice = graph::graph_slice(&g, 50, &dir.path().join("a"))
            .await
            .unwrap();
        assert_eq!(slice.nodes.len(), 2);
        let ids: std::collections::HashSet<_> =
            slice.nodes.iter().map(|n| n.project_name.as_str()).collect();
        assert!(ids.contains("a") && ids.contains("b"));
        assert!(slice.nodes.iter().any(|n| n.selected && n.project_name == "a"));
        assert!(slice.nodes.iter().all(|n| n.shared));
    }

    #[tokio::test]
    async fn p1_sync_copies_policy_rules_and_skills() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("proj");
        std::fs::create_dir_all(&project).unwrap();
        let p = project_pool(&project).await;
        sqlx::query(
            "CREATE TABLE policy_rules (
                id TEXT PRIMARY KEY, level TEXT NOT NULL, always_apply INTEGER NOT NULL DEFAULT 0,
                globs TEXT NOT NULL DEFAULT '[]', triggers TEXT NOT NULL DEFAULT '[]',
                tags TEXT NOT NULL DEFAULT '[]', priority INTEGER NOT NULL DEFAULT 50,
                body TEXT NOT NULL, source_path TEXT NOT NULL, content_hash TEXT NOT NULL,
                updated_at INTEGER NOT NULL, enabled INTEGER NOT NULL DEFAULT 1,
                status TEXT NOT NULL DEFAULT 'approved', scope TEXT NOT NULL DEFAULT 'project'
            )",
        )
        .execute(&p)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE policy_skills (
                name TEXT PRIMARY KEY, description TEXT NOT NULL, always_apply INTEGER NOT NULL DEFAULT 0,
                triggers TEXT NOT NULL DEFAULT '[]', tags TEXT NOT NULL DEFAULT '[]',
                priority INTEGER NOT NULL DEFAULT 50, body TEXT NOT NULL, source_path TEXT NOT NULL,
                content_hash TEXT NOT NULL, updated_at INTEGER NOT NULL
            )",
        )
        .execute(&p)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO policy_rules (id, level, always_apply, globs, triggers, tags, priority, body, source_path, content_hash, updated_at)
             VALUES ('foo', 'INFO', 0, '[]', '[]', '[]', 50, 'b', 'r.mdc', 'h', 0)",
        )
        .execute(&p)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO policy_skills (name, description, always_apply, triggers, tags, priority, body, source_path, content_hash, updated_at)
             VALUES ('bar', 'd', 0, '[]', '[]', 50, 'b', 's/SKILL.md', 'h', 0)",
        )
        .execute(&p)
        .await
        .unwrap();
        p.close().await;

        let global = dir.path().join("global.db");
        let g = open_and_init(&global).await.unwrap();
        sync::sync_project(&g, &project).await.unwrap();
        let rc: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM global_policy_rules")
            .fetch_one(&g)
            .await
            .unwrap();
        let sc: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM global_policy_skills")
            .fetch_one(&g)
            .await
            .unwrap();
        assert_eq!(rc.0, 1);
        assert_eq!(sc.0, 1);
    }

    #[tokio::test]
    async fn p3_foreign_policy_skips_selected_project() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["alpha", "beta"] {
            let project = dir.path().join(name);
            std::fs::create_dir_all(&project).unwrap();
            let p = project_pool(&project).await;
            sqlx::query(
                "CREATE TABLE policy_rules (
                    id TEXT PRIMARY KEY, level TEXT NOT NULL, always_apply INTEGER NOT NULL DEFAULT 0,
                    globs TEXT NOT NULL DEFAULT '[]', triggers TEXT NOT NULL DEFAULT '[]',
                    tags TEXT NOT NULL DEFAULT '[]', priority INTEGER NOT NULL DEFAULT 50,
                    body TEXT NOT NULL, source_path TEXT NOT NULL, content_hash TEXT NOT NULL,
                    updated_at INTEGER NOT NULL
                )",
            )
            .execute(&p)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO policy_rules (id, level, always_apply, globs, triggers, tags, priority, body, source_path, content_hash, updated_at)
                 VALUES (?, 'INFO', 0, '[]', '[]', '[]', 50, 'b', 'r.mdc', 'h', 0)",
            )
            .bind(name)
            .execute(&p)
            .await
            .unwrap();
            p.close().await;
        }
        let global = dir.path().join("global.db");
        let g = open_and_init(&global).await.unwrap();
        sync::sync_project(&g, &dir.path().join("alpha")).await.unwrap();
        sync::sync_project(&g, &dir.path().join("beta")).await.unwrap();
        let mut local = vec![policy::annotate_local(
            serde_json::json!({"id":"alpha"}),
            "alpha",
            policy::PolicyKind::Rules,
        )];
        policy::extend_with_foreign_policy(
            &g,
            &dir.path().join("alpha"),
            policy::PolicyKind::Rules,
            &mut local,
        )
        .await
        .unwrap();
        assert_eq!(local.len(), 2);
        assert_eq!(local[0]["origin"], "project");
        assert_eq!(local[1]["origin"], "global");
        assert_eq!(local[1]["id"], "beta");
    }

    #[tokio::test]
    async fn p5_lists_this_project_global_only_copy() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("solo");
        std::fs::create_dir_all(&project).unwrap();
        let p = project_pool(&project).await;
        sqlx::query(
            "CREATE TABLE policy_rules (
                id TEXT PRIMARY KEY, level TEXT NOT NULL, always_apply INTEGER NOT NULL DEFAULT 0,
                globs TEXT NOT NULL DEFAULT '[]', triggers TEXT NOT NULL DEFAULT '[]',
                tags TEXT NOT NULL DEFAULT '[]', priority INTEGER NOT NULL DEFAULT 50,
                body TEXT NOT NULL, source_path TEXT NOT NULL, content_hash TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            )",
        )
        .execute(&p)
        .await
        .unwrap();
        p.close().await;
        let global = dir.path().join("global.db");
        let g = open_and_init(&global).await.unwrap();
        sync::sync_project(&g, &project).await.unwrap();
        let pid = policy::ensure_project(&g, &project).await.unwrap();
        policy::upsert_policy_item(
            &g,
            pid,
            policy::PolicyKind::Rules,
            "parked",
            &serde_json::json!({"id":"parked","body":"x"}),
        )
        .await
        .unwrap();
        let mut local = vec![];
        policy::extend_with_foreign_policy(&g, &project, policy::PolicyKind::Rules, &mut local)
            .await
            .unwrap();
        assert_eq!(local.len(), 1);
        assert_eq!(local[0]["id"], "parked");
        assert_eq!(local[0]["origin"], "global");
        assert_eq!(local[0]["home"], true);
    }

    #[tokio::test]
    async fn p6_sync_does_not_drop_parked_policy() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("keep");
        std::fs::create_dir_all(&project).unwrap();
        let p = project_pool(&project).await;
        sqlx::query(
            "CREATE TABLE policy_rules (
                id TEXT PRIMARY KEY, level TEXT NOT NULL, always_apply INTEGER NOT NULL DEFAULT 0,
                globs TEXT NOT NULL DEFAULT '[]', triggers TEXT NOT NULL DEFAULT '[]',
                tags TEXT NOT NULL DEFAULT '[]', priority INTEGER NOT NULL DEFAULT 50,
                body TEXT NOT NULL, source_path TEXT NOT NULL, content_hash TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            )",
        )
        .execute(&p)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO policy_rules (id, level, always_apply, globs, triggers, tags, priority, body, source_path, content_hash, updated_at)
             VALUES ('live', 'INFO', 0, '[]', '[]', '[]', 50, 'b', 'r.mdc', 'h', 0)",
        )
        .execute(&p)
        .await
        .unwrap();
        p.close().await;
        let global = dir.path().join("global.db");
        let g = open_and_init(&global).await.unwrap();
        sync::sync_project(&g, &project).await.unwrap();
        let pid = policy::ensure_project(&g, &project).await.unwrap();
        policy::upsert_policy_item(
            &g,
            pid,
            policy::PolicyKind::Rules,
            "parked",
            &serde_json::json!({"id":"parked"}),
        )
        .await
        .unwrap();
        sync::sync_project(&g, &project).await.unwrap();
        let n: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM global_policy_rules")
            .fetch_one(&g)
            .await
            .unwrap();
        assert_eq!(n.0, 2);
    }

    #[test]
    fn p7_flattens_nested_frontmatter_for_list_ui() {
        let v = policy::flatten_list_item(
            serde_json::json!({
                "frontmatter": { "id": "utf8-no-bom", "tags": ["encoding"] },
                "body": "never utf-16"
            }),
            policy::PolicyKind::Rules,
            "utf8-no-bom",
        );
        assert_eq!(v["id"], "utf8-no-bom");
        assert_eq!(v["body"], "never utf-16");
        assert!(v["globs"].is_array());
        assert_eq!(v["tags"][0], "encoding");
    }

    #[test]
    fn p5_flattens_nested_skill_doc_for_list_ui() {
        let v = policy::flatten_list_item(
            serde_json::json!({
                "frontmatter": { "name": "systematic-debugging", "tags": ["debugging"] },
                "body": "root cause first"
            }),
            policy::PolicyKind::Skills,
            "systematic-debugging",
        );
        assert_eq!(v["name"], "systematic-debugging");
        assert!(v["tags"].is_array());
        assert_eq!(v["triggers"], serde_json::json!([]));
    }
}
