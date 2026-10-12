//! Authenticated, additive node exchange. Native data is never replaced by an
//! empty peer. Every accepted source revision remains in the project database;
//! absent records retain their last value and different origins retain both.
use crate::{
    server_config::{NodeRole, ServerConfig},
    server_snapshot::{self, Snapshot},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize)]
pub struct ExchangeReply {
    pub snapshot: Snapshot,
    pub contributed_revision: String,
    pub memory_conflicts: usize,
}
use sqlx::{Sqlite, Transaction};
use std::collections::BTreeMap;

pub const ORIGIN: &str = "axNodeOrigin";
pub const CONTENT: &str = "axNodeContentHash";
const MAX_ARCHIVE_BYTES: i64 = 64 * 1024 * 1024;
const MAX_NODES: i64 = 64;

pub fn policy_fingerprint(value: &Value) -> String {
    let mut value = value.clone();
    if let Some(row) = value.as_object_mut() {
        if let Some(body) = row
            .get_mut("body")
            .and_then(|v| v.as_str().map(str::to_owned))
        {
            row.insert("body".into(), Value::String(body.trim().to_owned()));
        }
        // Import IDs, storage and review state are local projections, not edits.
        for key in [
            "id",
            "name",
            "status",
            "share",
            "sourcePath",
            "stubPath",
            "rootId",
            "source",
            "storage",
            "effectiveStorage",
            "storageIsOverride",
        ] {
            row.remove(key);
        }
        if let Some(properties) = row.get_mut("properties").and_then(Value::as_object_mut) {
            properties.remove(ORIGIN);
            properties.remove(CONTENT);
            if properties.is_empty() {
                row.remove("properties");
            }
        }
    }
    blake3::hash(
        serde_json::to_string(&value)
            .expect("JSON serializes")
            .as_bytes(),
    )
    .to_hex()
    .to_string()
}

pub fn unedited_policy_import(value: &Value) -> bool {
    value
        .get("properties")
        .and_then(|p| p.get(ORIGIN))
        .and_then(Value::as_str)
        .is_some()
        && value
            .get("properties")
            .and_then(|p| p.get(CONTENT))
            .and_then(Value::as_str)
            == Some(policy_fingerprint(value).as_str())
}

fn merge(histories: &[Snapshot], config: &ServerConfig, project: &str) -> Result<Snapshot, String> {
    let mut files = BTreeMap::new();
    for (path, key) in [
        ("state/memories.json", "id"),
        ("state/rules.json", "id"),
        ("state/skills.json", "name"),
    ] {
        let mut records = BTreeMap::<String, Value>::new();
        for history in histories {
            let rows: Vec<Value> =
                serde_json::from_str(history.files.get(path).ok_or("missing state file")?)
                    .map_err(|e| e.to_string())?;
            for mut row in rows {
                let normalized = match path {
                    "state/memories.json" => serde_json::to_value(
                        serde_json::from_value::<ax_memory::types::MemoryRow>(row.clone())
                            .map_err(|e| e.to_string())?,
                    ),
                    "state/rules.json" => serde_json::to_value(
                        serde_json::from_value::<ax_policy::types::PolicyRuleRow>(row.clone())
                            .map_err(|e| e.to_string())?,
                    ),
                    _ => serde_json::to_value(
                        serde_json::from_value::<ax_policy::types::PolicySkillRow>(row.clone())
                            .map_err(|e| e.to_string())?,
                    ),
                }
                .map_err(|e| e.to_string())?;
                for (key, value) in normalized.as_object().ok_or("invalid record")? {
                    row.as_object_mut()
                        .ok_or("invalid record")?
                        .entry(key.clone())
                        .or_insert_with(|| value.clone());
                }
                let original = row
                    .get(key)
                    .and_then(Value::as_str)
                    .ok_or("invalid record key")?;
                let id = format!(
                    "cluster-{}",
                    blake3::hash(
                        format!("{}\0{path}\0{original}", history.primary_node_id).as_bytes()
                    )
                    .to_hex()
                );
                let object = row.as_object_mut().ok_or("invalid state record")?;
                object.insert(key.into(), Value::String(id.clone()));
                if path == "state/memories.json" {
                    object.insert(
                        "source".into(),
                        Value::String(format!("ax-cluster:{}", history.primary_node_id)),
                    );
                } else {
                    let fingerprint = policy_fingerprint(&row);
                    let properties = row
                        .as_object_mut()
                        .unwrap()
                        .entry("properties")
                        .or_insert_with(|| serde_json::json!({}))
                        .as_object_mut()
                        .ok_or("invalid policy properties")?;
                    properties.insert(
                        ORIGIN.into(),
                        Value::String(history.primary_node_id.clone()),
                    );
                    properties.insert(CONTENT.into(), Value::String(fingerprint));
                }
                // No remove operation: omission cannot erase an earlier record.
                records.insert(id, row);
            }
        }
        files.insert(
            path.into(),
            serde_json::to_string(&records.into_values().collect::<Vec<_>>())
                .map_err(|e| e.to_string())?,
        );
    }
    let mut result = Snapshot {
        schema_version: 2,
        cluster_id: config.cluster_id.clone(),
        primary_node_id: config.node_id.clone(),
        project_id: project.into(),
        revision: String::new(),
        files,
    };
    result.revision = server_snapshot::revision(&result)?;
    server_snapshot::validate(&result, &config.cluster_id, project)?;
    Ok(result)
}

async fn retain(tx: &mut Transaction<'_, Sqlite>, root: &std::path::Path, snapshot: &Snapshot) -> Result<(), String> {
    let mut knowledge = snapshot.clone();
    if snapshot.schema_version == 3 {
        for mut item in crate::server_data::manifests(snapshot)? {
            let previous: Option<String> = sqlx::query_scalar("SELECT manifest FROM ax_node_datasets WHERE cluster_id=? AND project_id=? AND node_id=?")
                .bind(&snapshot.cluster_id).bind(&snapshot.project_id).bind(&item.origin).fetch_optional(&mut **tx).await.map_err(|e|e.to_string())?;
            if let Some(previous) = previous {
                let old:ax_db::replication::Dataset=serde_json::from_str(&previous).map_err(|e|e.to_string())?;
                ax_db::replication::retain_logs(root,&old,&mut item).map_err(|e|e.to_string())?;
            }
            ax_db::replication::pin(root,&item).map_err(|e|e.to_string())?;
            sqlx::query("INSERT INTO ax_node_datasets VALUES (?,?,?,?) ON CONFLICT(cluster_id,project_id,node_id) DO UPDATE SET manifest=excluded.manifest")
                .bind(&snapshot.cluster_id).bind(&snapshot.project_id).bind(&item.origin).bind(serde_json::to_string(&item).map_err(|e|e.to_string())?).execute(&mut **tx).await.map_err(|e|e.to_string())?;
        }
        knowledge.files.remove(ax_db::replication::DATA_PATH); knowledge.schema_version=2; knowledge.revision=server_snapshot::revision(&knowledge)?;
    }
    let snapshot=&knowledge;
    sqlx::query("INSERT OR IGNORE INTO ax_node_contributions (cluster_id,project_id,node_id,revision,snapshot) VALUES (?,?,?,?,?)")
        .bind(&snapshot.cluster_id).bind(&snapshot.project_id).bind(&snapshot.primary_node_id).bind(&snapshot.revision)
        .bind(serde_json::to_string(snapshot).map_err(|e| e.to_string())?)
        .execute(&mut **tx).await.map_err(|e| e.to_string())?;
    // Re-selecting a previously accepted revision is a legitimate local
    // rollback. Keep its original payload, but move its selection after this
    // node's other revisions so peers receive the current value. Identical
    // consecutive contributions remain idempotent and omissions retain data.
    sqlx::query("UPDATE ax_node_contributions SET seq=(SELECT MAX(seq)+1 FROM ax_node_contributions) WHERE cluster_id=? AND project_id=? AND node_id=? AND revision=? AND seq<(SELECT MAX(seq) FROM ax_node_contributions WHERE cluster_id=? AND project_id=? AND node_id=?)")
        .bind(&snapshot.cluster_id).bind(&snapshot.project_id).bind(&snapshot.primary_node_id).bind(&snapshot.revision)
        .bind(&snapshot.cluster_id).bind(&snapshot.project_id).bind(&snapshot.primary_node_id)
        .execute(&mut **tx).await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Only the authenticated Primary calls this. Incoming identity is separately
/// bound to the authorized node header by the HTTP handler before this point.
pub async fn publish(
    config: &ServerConfig,
    project_id: &str,
    incoming: Option<&Snapshot>,
) -> Result<Snapshot, String> {
    if config.role != NodeRole::Primary {
        return Err("only primaries coordinate node exchange".into());
    }
    if let Some(snapshot) = incoming {
        server_snapshot::validate(snapshot, &config.cluster_id, project_id)?;
        if snapshot.primary_node_id == config.node_id {
            return Err("a replica cannot contribute as the Primary".into());
        }
    }
    if let Some(snapshot) = incoming.filter(|s|s.schema_version==3) { crate::server_data::verify_contribution(config,project_id,snapshot).await?; }
    let native = server_snapshot::export_native(config, project_id).await?;
    let project = config
        .projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or("project not registered")?;
    let ax = ax_core::Ax::open(&project.path)
        .await
        .map_err(|e| e.to_string())?;
    let pool = ax.db_pool();
    sqlx::query("CREATE TABLE IF NOT EXISTS ax_node_contributions (seq INTEGER PRIMARY KEY AUTOINCREMENT, cluster_id TEXT NOT NULL, project_id TEXT NOT NULL, node_id TEXT NOT NULL, revision TEXT NOT NULL, snapshot TEXT NOT NULL, UNIQUE(cluster_id,project_id,node_id,revision))")
        .execute(pool).await.map_err(|e| e.to_string())?;
    sqlx::query("CREATE TABLE IF NOT EXISTS ax_node_datasets (cluster_id TEXT NOT NULL,project_id TEXT NOT NULL,node_id TEXT NOT NULL,manifest TEXT NOT NULL,PRIMARY KEY(cluster_id,project_id,node_id))").execute(pool).await.map_err(|e|e.to_string())?;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    retain(&mut tx, &project.path, &native).await?;
    if let Some(snapshot) = incoming {
        retain(&mut tx, &project.path, snapshot).await?;
    }
    let (bytes, nodes): (i64, i64) = sqlx::query_as("SELECT COALESCE(SUM(length(CAST(snapshot AS BLOB))),0), COUNT(DISTINCT node_id) FROM ax_node_contributions WHERE cluster_id=? AND project_id=?")
        .bind(&config.cluster_id).bind(project_id).fetch_one(&mut *tx).await.map_err(|e| e.to_string())?;
    if bytes > MAX_ARCHIVE_BYTES || nodes > MAX_NODES {
        return Err("node retention capacity reached; existing history was preserved, no contribution was accepted".into());
    }
    let raw: Vec<(String,)> = sqlx::query_as("SELECT snapshot FROM ax_node_contributions WHERE cluster_id=? AND project_id=? ORDER BY seq")
        .bind(&config.cluster_id).bind(project_id).fetch_all(&mut *tx).await.map_err(|e| e.to_string())?;
    let mut histories = Vec::with_capacity(raw.len());
    for (raw,) in raw {
        let item: Snapshot =
            serde_json::from_str(&raw).map_err(|e| format!("invalid retained snapshot: {e}"))?;
        server_snapshot::validate(&item, &config.cluster_id, project_id)?;
        histories.push(item);
    }
    // Validate merged limits before committing a new contribution. Never prune
    // old history to make space or acknowledge data that cannot be published.
    let merge_config = config.clone();
    let merge_project = project_id.to_owned();
    let mut result =
        tokio::task::spawn_blocking(move || merge(&histories, &merge_config, &merge_project))
            .await
            .map_err(|e| format!("node merge task failed: {e}"))??;
    let raw:Vec<String>=sqlx::query_scalar("SELECT manifest FROM ax_node_datasets WHERE cluster_id=? AND project_id=? ORDER BY node_id").bind(&config.cluster_id).bind(project_id).fetch_all(&mut *tx).await.map_err(|e|e.to_string())?;
    let items:Vec<ax_db::replication::Dataset>=raw.iter().map(|s|serde_json::from_str(s).map_err(|e|e.to_string())).collect::<Result<_,_>>()?;
    result.schema_version=3; result.files.insert(ax_db::replication::DATA_PATH.into(),serde_json::to_string(&items).map_err(|e|e.to_string())?);
    result.revision=server_snapshot::revision(&result)?; server_snapshot::validate(&result,&config.cluster_id,project_id)?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server_config::ProjectRegistration;
    use ax_memory::types::MemoryRow;

    fn snapshot(node: &str, body: Option<&str>) -> Snapshot {
        let rows: Vec<MemoryRow> = body
            .into_iter()
            .map(|body| MemoryRow {
                id: "same-id".into(),
                kind: "note".into(),
                title: "Existing knowledge".into(),
                body: body.into(),
                tags: vec![],
                files: vec![],
                confidence: 1.0,
                source: "manual".into(),
                enabled: true,
                created_at: 1,
                updated_at: 1,
            })
            .collect();
        let mut s = Snapshot {
            schema_version: 2,
            cluster_id: "team".into(),
            primary_node_id: node.into(),
            project_id: "project".into(),
            revision: String::new(),
            files: BTreeMap::from([
                (
                    "state/memories.json".into(),
                    serde_json::to_string(&rows).unwrap(),
                ),
                ("state/rules.json".into(), "[]".into()),
                ("state/skills.json".into(), "[]".into()),
            ]),
        };
        s.revision = server_snapshot::revision(&s).unwrap();
        s
    }
    fn config(path: &std::path::Path) -> ServerConfig {
        ServerConfig {
            schema_version: 1,
            cluster_id: "team".into(),
            node_id: "primary".into(),
            role: NodeRole::Primary,
            primary_url: None,
            projects: vec![ProjectRegistration {
                id: "project".into(),
                path: path.into(),
                token_env: "TEST_TOKEN".into(),
            }],
        }
    }
    #[test]
    fn empty_primary_and_omitted_records_cannot_erase_populated_replica() {
        let c = config(std::path::Path::new("."));
        let result = merge(
            &[
                snapshot("primary", None),
                snapshot("replica", Some("Months of work")),
                snapshot("replica", None),
            ],
            &c,
            "project",
        )
        .unwrap();
        let rows: Vec<MemoryRow> =
            serde_json::from_str(&result.files["state/memories.json"]).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].body, "Months of work");
    }
    #[test]
    fn colliding_ids_from_different_nodes_preserve_both_values() {
        let c = config(std::path::Path::new("."));
        let result = merge(
            &[
                snapshot("primary", Some("Primary decision")),
                snapshot("replica", Some("Replica decision")),
            ],
            &c,
            "project",
        )
        .unwrap();
        let rows: Vec<MemoryRow> =
            serde_json::from_str(&result.files["state/memories.json"]).unwrap();
        assert_eq!(rows.len(), 2);
        assert_ne!(rows[0].id, rows[1].id);
        assert!(rows.iter().any(|r| r.body == "Primary decision"));
        assert!(rows.iter().any(|r| r.body == "Replica decision"));
    }
    #[tokio::test]
    async fn history_is_durable_idempotent_and_rejects_cross_cluster() {
        let dir = tempfile::tempdir().unwrap();
        let ax = ax_core::Ax::init(dir.path()).await.unwrap();
        let c = config(dir.path());
        let original = snapshot("replica", Some("First version"));
        publish(&c, "project", Some(&original)).await.unwrap();
        publish(&c, "project", Some(&original)).await.unwrap();
        let updated = snapshot("replica", Some("Second version"));
        publish(&c, "project", Some(&updated)).await.unwrap();
        let raw: Vec<(String,)> = sqlx::query_as(
            "SELECT snapshot FROM ax_node_contributions WHERE node_id='replica' ORDER BY seq",
        )
        .fetch_all(ax.db_pool())
        .await
        .unwrap();
        assert_eq!(raw.len(), 2);
        assert!(raw[0].0.contains("First version"));
        assert!(raw[1].0.contains("Second version"));
        let mut invalid = original;
        invalid.cluster_id = "other".into();
        invalid.revision = server_snapshot::revision(&invalid).unwrap();
        assert!(publish(&c, "project", Some(&invalid)).await.is_err());
    }

    #[tokio::test]
    async fn returning_to_an_earlier_revision_preserves_history_and_publishes_current_value() {
        let dir = tempfile::tempdir().unwrap();
        let ax = ax_core::Ax::init(dir.path()).await.unwrap();
        let c = config(dir.path());
        let earlier = snapshot("replica", Some("Earlier decision"));
        publish(&c, "project", Some(&earlier)).await.unwrap();
        publish(
            &c,
            "project",
            Some(&snapshot("replica", Some("Later decision"))),
        )
        .await
        .unwrap();
        publish(&c, "project", Some(&earlier)).await.unwrap();
        let result = publish(&c, "project", Some(&snapshot("replica", None)))
            .await
            .unwrap();
        let rows: Vec<MemoryRow> =
            serde_json::from_str(&result.files["state/memories.json"]).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].body, "Earlier decision");
        let raw: Vec<(String,)> =
            sqlx::query_as("SELECT snapshot FROM ax_node_contributions WHERE node_id='replica'")
                .fetch_all(ax.db_pool())
                .await
                .unwrap();
        assert_eq!(raw.len(), 3);
        assert!(raw.iter().any(|r| r.0.contains("Earlier decision")));
        assert!(raw.iter().any(|r| r.0.contains("Later decision")));
    }

    #[tokio::test]
    async fn populated_replica_seeds_empty_primary_and_other_replica_without_echo() {
        let primary_dir = tempfile::tempdir().unwrap();
        let primary_ax = ax_core::Ax::init(primary_dir.path()).await.unwrap();
        let primary = config(primary_dir.path());
        let replica_dir = tempfile::tempdir().unwrap();
        let replica_ax = ax_core::Ax::init(replica_dir.path()).await.unwrap();
        let mut replica = config(replica_dir.path());
        replica.node_id = "replica".into();
        replica.role = NodeRole::Replica;
        let memory = ax_memory::store::remember(
            replica_ax.db_pool(),
            ax_memory::types::RememberInput {
                title: "Months of work".into(),
                body: "Do not lose this existing decision".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let native = server_snapshot::export_native(&replica, "project")
            .await
            .unwrap();
        let cluster = publish(&primary, "project", Some(&native)).await.unwrap();
        crate::server_apply::apply(&primary, &cluster)
            .await
            .unwrap();
        crate::server_apply::apply(&replica, &cluster)
            .await
            .unwrap();
        let unchanged: String = sqlx::query_scalar("SELECT body FROM memories WHERE id=?")
            .bind(&memory.id)
            .fetch_one(replica_ax.db_pool())
            .await
            .unwrap();
        assert_eq!(unchanged, memory.body);
        let primary_memories: Vec<MemoryRow> = ax_memory::store::list(primary_ax.db_pool(), 100, 0)
            .await
            .unwrap()
            .0;
        assert_eq!(primary_memories.len(), 1);
        assert_eq!(primary_memories[0].body, memory.body);
        let other_dir = tempfile::tempdir().unwrap();
        let other_ax = ax_core::Ax::init(other_dir.path()).await.unwrap();
        let mut other = config(other_dir.path());
        other.node_id = "other".into();
        other.role = NodeRole::Replica;
        crate::server_apply::apply(&other, &cluster).await.unwrap();
        for _ in 0..3 {
            let outgoing = server_snapshot::export_native(&other, "project")
                .await
                .unwrap();
            let rows: Vec<MemoryRow> =
                serde_json::from_str(&outgoing.files["state/memories.json"]).unwrap();
            assert!(rows.is_empty());
            let again = publish(&primary, "project", Some(&outgoing)).await.unwrap();
            crate::server_apply::apply(&other, &again).await.unwrap();
            assert_eq!(
                ax_memory::store::list(other_ax.db_pool(), 100, 0)
                    .await
                    .unwrap()
                    .0
                    .len(),
                1
            );
        }
    }

    #[tokio::test]
    async fn empty_snapshot_retains_imports_and_local_edits_are_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let ax = ax_core::Ax::init(dir.path()).await.unwrap();
        let mut replica = config(dir.path());
        replica.node_id = "replica".into();
        replica.role = NodeRole::Replica;
        crate::server_apply::apply(&replica, &snapshot("primary", Some("Imported decision")))
            .await
            .unwrap();
        let report = crate::server_apply::apply(&replica, &snapshot("primary", None))
            .await
            .unwrap();
        assert_eq!(report.retained, 1);
        sqlx::query(
            "UPDATE memories SET body='Locally edited decision' WHERE source LIKE 'ax-replica:%'",
        )
        .execute(ax.db_pool())
        .await
        .unwrap();
        let report = crate::server_apply::apply(
            &replica,
            &snapshot("primary", Some("Different remote decision")),
        )
        .await
        .unwrap();
        assert_eq!(report.conflicts, 1);
        let local: String = sqlx::query_scalar("SELECT body FROM memories")
            .fetch_one(ax.db_pool())
            .await
            .unwrap();
        assert_eq!(local, "Locally edited decision");
        let outgoing = server_snapshot::export_native(&replica, "project")
            .await
            .unwrap();
        let rows: Vec<MemoryRow> =
            serde_json::from_str(&outgoing.files["state/memories.json"]).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].body, "Locally edited decision");
    }

    #[tokio::test]
    async fn received_policy_is_pending_and_unchanged_imports_do_not_loop() {
        let dir = tempfile::tempdir().unwrap();
        let ax = ax_core::Ax::init(dir.path()).await.unwrap();
        let c = config(dir.path());
        let mut incoming = snapshot("replica", None);
        incoming.files.insert("state/rules.json".into(),serde_json::json!([{"id":"older-rule","level":"CRITICAL","alwaysApply":true,"globs":[],"triggers":[],"tags":[],"priority":90,"body":"Existing safety rule","sourcePath":"","enabled":false,"status":"approved","scope":"project"}]).to_string());
        incoming.files.insert("state/skills.json".into(),serde_json::json!([{"name":"older-skill","description":"Existing workflow","alwaysApply":false,"triggers":[],"tags":[],"priority":70,"body":"Existing workflow body","sourcePath":"","enabled":true,"status":"approved","scope":"project"}]).to_string());
        incoming.revision = server_snapshot::revision(&incoming).unwrap();
        let cluster = publish(&c, "project", Some(&incoming)).await.unwrap();
        let report = crate::server_apply::apply(&c, &cluster).await.unwrap();
        assert_eq!(report.policy_queued, 2);
        let pending = ax_policy::paths::pending_rules_dir(&dir.path().join(".ax"));
        let path = std::fs::read_dir(&pending)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let raw = std::fs::read_to_string(&path).unwrap();
        let imported = ax_policy::parse::parse_rule_file(&path, &raw).unwrap();
        assert_eq!(imported.frontmatter.status, "pending");
        assert!(!imported.frontmatter.enabled);
        let native = server_snapshot::export_native(&c, "project").await.unwrap();
        assert!(!native.files["state/rules.json"].contains("Existing safety rule"));
        assert!(!native.files["state/skills.json"].contains("Existing workflow body"));
        ax_policy::review::approve_pending(ax.db_pool(), dir.path(), &imported.frontmatter.id)
            .await
            .unwrap();
        let native = server_snapshot::export_native(&c, "project").await.unwrap();
        assert!(!native.files["state/rules.json"].contains("Existing safety rule"));
        let approved = ax_policy::list_rules(ax.db_pool()).await.unwrap();
        let path = std::path::PathBuf::from(
            &approved
                .iter()
                .find(|r| r.id == imported.frontmatter.id)
                .unwrap()
                .source_path,
        );
        let raw = std::fs::read_to_string(&path).unwrap();
        std::fs::write(
            &path,
            raw.replace("Existing safety rule", "Locally edited safety rule"),
        )
        .unwrap();
        ax.index_policy(true).await.unwrap();
        let native = server_snapshot::export_native(&c, "project").await.unwrap();
        assert!(native.files["state/rules.json"].contains("Locally edited safety rule"));
    }

    #[tokio::test]
    async fn oversized_union_is_rejected_without_pruning_accepted_data() {
        let dir = tempfile::tempdir().unwrap();
        let ax = ax_core::Ax::init(dir.path()).await.unwrap();
        let c = config(dir.path());
        let mut full = snapshot("replica", Some("Retained knowledge"));
        let template: MemoryRow =
            serde_json::from_str::<Vec<MemoryRow>>(&full.files["state/memories.json"])
                .unwrap()
                .remove(0);
        let rows: Vec<_> = (0..server_snapshot::MAX_RECORDS_PER_KIND)
            .map(|i| {
                let mut row = template.clone();
                row.id = format!("known-{i}");
                row
            })
            .collect();
        full.files.insert(
            "state/memories.json".into(),
            serde_json::to_string(&rows).unwrap(),
        );
        full.revision = server_snapshot::revision(&full).unwrap();
        publish(&c, "project", Some(&full)).await.unwrap();
        assert!(publish(
            &c,
            "project",
            Some(&snapshot("replica", Some("New record beyond capacity")))
        )
        .await
        .is_err());
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ax_node_contributions WHERE node_id='replica'",
        )
        .fetch_one(ax.db_pool())
        .await
        .unwrap();
        assert_eq!(count, 1);
        let result = publish(&c, "project", None).await.unwrap();
        let rows: Vec<MemoryRow> =
            serde_json::from_str(&result.files["state/memories.json"]).unwrap();
        assert_eq!(rows.len(), server_snapshot::MAX_RECORDS_PER_KIND);
    }
}
