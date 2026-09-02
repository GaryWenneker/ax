use std::path::Path;

use ax_utils::errors::{AxError, DatabaseError};
use serde::Serialize;
use sqlx::SqlitePool;

use crate::index::{get_rule, get_skill, rule_row_to_doc, skill_row_to_doc};

pub const POLICY_REVISION_CAP: i64 = 20;
pub const SOURCE_SAVE: &str = "save";
pub const SOURCE_RESTORE: &str = "restore";

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PolicyRevision {
    pub id: i64,
    pub kind: String,
    pub item_id: String,
    pub content_hash: String,
    pub body: String,
    pub source: String,
    pub created_at: i64,
}

pub fn content_hash(body: &str) -> String {
    blake3::hash(body.as_bytes()).to_hex().to_string()
}

pub fn parse_written_key(key: &str) -> Option<(&str, &str)> {
    let (kind, id) = key.split_once(':')?;
    if (kind == "rule" || kind == "skill") && !id.is_empty() {
        Some((kind, id))
    } else {
        None
    }
}

/// Insert a revision when `body` hashes differently from the newest row.
/// Returns whether a row was inserted.
pub async fn record_if_changed(
    pool: &SqlitePool,
    kind: &str,
    item_id: &str,
    body: &str,
    source: &str,
) -> Result<bool, AxError> {
    let hash = content_hash(body);
    let latest: Option<String> = sqlx::query_scalar(
        "SELECT content_hash FROM policy_revisions
         WHERE kind = ? AND item_id = ?
         ORDER BY created_at DESC, id DESC
         LIMIT 1",
    )
    .bind(kind)
    .bind(item_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    if latest.as_deref() == Some(hash.as_str()) {
        return Ok(false);
    }
    let now = now_ms();
    sqlx::query(
        "INSERT INTO policy_revisions (kind, item_id, content_hash, body, source, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(kind)
    .bind(item_id)
    .bind(&hash)
    .bind(body)
    .bind(source)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    prune_old(pool, kind, item_id).await?;
    Ok(true)
}

async fn prune_old(pool: &SqlitePool, kind: &str, item_id: &str) -> Result<(), AxError> {
    let sql = format!(
        "DELETE FROM policy_revisions
         WHERE id IN (
           SELECT id FROM policy_revisions
           WHERE kind = ? AND item_id = ?
           ORDER BY created_at DESC, id DESC
           LIMIT -1 OFFSET {POLICY_REVISION_CAP}
         )"
    );
    sqlx::query(&sql)
    .bind(kind)
    .bind(item_id)
    .execute(pool)
    .await
    .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(())
}

pub async fn list_revisions(
    pool: &SqlitePool,
    kind: &str,
    item_id: &str,
) -> Result<Vec<PolicyRevision>, AxError> {
    sqlx::query_as::<_, PolicyRevision>(
        "SELECT id, kind, item_id, content_hash, body, source, created_at
         FROM policy_revisions
         WHERE kind = ? AND item_id = ?
         ORDER BY created_at DESC, id DESC
         LIMIT 20",
    )
    .bind(kind)
    .bind(item_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))
}

pub async fn get_revision(pool: &SqlitePool, id: i64) -> Result<Option<PolicyRevision>, AxError> {
    sqlx::query_as::<_, PolicyRevision>(
        "SELECT id, kind, item_id, content_hash, body, source, created_at
         FROM policy_revisions WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))
}

/// After zip restore + `index_policy`, record each written item as source `restore`.
pub async fn record_restore_writes(
    pool: &SqlitePool,
    project_root: &Path,
    written: &[String],
) -> Result<(), AxError> {
    for key in written {
        let Some((kind, id)) = parse_written_key(key) else {
            continue;
        };
        let raw = match kind {
            "rule" => {
                let row = get_rule(pool, id).await?.ok_or_else(|| {
                    AxError::Other(format!("restore revision: missing indexed rule {id}"))
                })?;
                rule_row_to_doc(&row, project_root).raw
            }
            "skill" => {
                let row = get_skill(pool, id).await?.ok_or_else(|| {
                    AxError::Other(format!("restore revision: missing indexed skill {id}"))
                })?;
                skill_row_to_doc(&row, project_root).raw
            }
            _ => continue,
        };
        record_if_changed(pool, kind, id, &raw, SOURCE_RESTORE).await?;
    }
    Ok(())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ax_db::Database;
    use std::path::PathBuf;

    fn scratch_db(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "ax-revisions-{name}-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_file(&p);
        p
    }

    fn cleanup(path: &std::path::Path) {
        for suffix in ["", "-wal", "-shm"] {
            let mut p = path.to_path_buf();
            if !suffix.is_empty() {
                p.set_file_name(format!(
                    "{}{suffix}",
                    path.file_name().unwrap().to_string_lossy()
                ));
            }
            let _ = std::fs::remove_file(&p);
        }
    }

    #[tokio::test]
    async fn first_write_identical_then_changed() {
        let path = scratch_db("b1");
        let db = Database::open(&path).await.expect("open");
        let pool = db.pool();

        assert!(record_if_changed(pool, "rule", "rev-a", "body_a", SOURCE_SAVE)
            .await
            .unwrap());
        let rows = list_revisions(pool, "rule", "rev-a").await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].body, "body_a");
        assert_eq!(rows[0].content_hash, content_hash("body_a"));
        assert_eq!(rows[0].source, SOURCE_SAVE);

        assert!(!record_if_changed(pool, "rule", "rev-a", "body_a", SOURCE_SAVE)
            .await
            .unwrap());
        assert_eq!(list_revisions(pool, "rule", "rev-a").await.unwrap().len(), 1);

        assert!(record_if_changed(pool, "rule", "rev-a", "body_b", SOURCE_SAVE)
            .await
            .unwrap());
        let rows = list_revisions(pool, "rule", "rev-a").await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].body, "body_b");
        assert_eq!(rows[0].content_hash, content_hash("body_b"));

        cleanup(&path);
    }

    #[tokio::test]
    async fn cap_keeps_twenty_newest() {
        let path = scratch_db("cap");
        let db = Database::open(&path).await.expect("open");
        let pool = db.pool();
        for i in 0..21 {
            record_if_changed(pool, "skill", "cap-me", &format!("body-{i}"), SOURCE_SAVE)
                .await
                .unwrap();
        }
        let rows = list_revisions(pool, "skill", "cap-me").await.unwrap();
        assert_eq!(rows.len(), 20);
        assert_eq!(rows[0].body, "body-20");
        assert!(rows.iter().all(|r| r.body != "body-0"));
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM policy_revisions WHERE kind = 'skill' AND item_id = 'cap-me'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(count, 20);
        cleanup(&path);
    }

    #[tokio::test]
    async fn restore_source_is_recorded() {
        let path = scratch_db("restore");
        let db = Database::open(&path).await.expect("open");
        record_if_changed(db.pool(), "rule", "x", "pkg", SOURCE_RESTORE)
            .await
            .unwrap();
        let rows = list_revisions(db.pool(), "rule", "x").await.unwrap();
        assert_eq!(rows[0].source, "restore");
        cleanup(&path);
    }

    #[test]
    fn parse_written_key_rule_and_skill() {
        assert_eq!(parse_written_key("rule:alpha"), Some(("rule", "alpha")));
        assert_eq!(parse_written_key("skill:startup"), Some(("skill", "startup")));
        assert!(parse_written_key("nope").is_none());
        assert!(parse_written_key("file:x").is_none());
    }

    fn test_rule_fm(id: &str) -> crate::types::RuleFrontmatter {
        crate::types::RuleFrontmatter {
            id: id.into(),
            level: "INFO".into(),
            always_apply: true,
            globs: vec![],
            triggers: vec![],
            tags: vec!["test".into()],
            priority: 50,
            enabled: true,
            status: "approved".into(),
            share: false,
            scope: "project".into(),
            storage: Some("database".into()),
            source: None,
            root_id: None,
            group: None,
        }
    }

    #[tokio::test]
    async fn save_rule_skips_noop_and_records_change() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("ax.db");
        let db = Database::open(&db_path).await.expect("open");
        let store = crate::store::PolicyStore::new(db.pool().clone(), dir.path().to_path_buf());
        let fm = test_rule_fm("rev-a");
        store.save_rule(fm.clone(), "one".into()).await.expect("save 1");
        store.save_rule(fm.clone(), "one".into()).await.expect("save 2");
        store.save_rule(fm, "two".into()).await.expect("save 3");
        let rows = list_revisions(db.pool(), "rule", "rev-a").await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].source, SOURCE_SAVE);
    }
}
