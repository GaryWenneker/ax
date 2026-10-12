//! Standalone Ax node synchronization protocol.
//!
//! Node replication is deliberately INDEPENDENT of the user-facing memory
//! export, policy-pack export and their shared/active filters. A project is
//! enrolled explicitly in the node configuration and authenticated per project.
//! Replication mirrors both active and inactive records, retaining their share
//! flags. Replicas stage a separate read-only copy; they never overwrite local
//! policy, memories or a live SQLite database during a pull.

use crate::server_config::{NodeRole, ServerConfig};
use ax_memory::types::MemoryRow;
use ax_policy::types::{PolicyRuleRow, PolicySkillRow};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

pub const MAX_SNAPSHOT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_SNAPSHOT_FILES: usize = 4;
pub const MAX_RECORDS_PER_KIND: usize = 10_000;

const MEMORY_PATH: &str = "state/memories.json";
const RULE_PATH: &str = "state/rules.json";
const SKILL_PATH: &str = "state/skills.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub schema_version: u32,
    pub cluster_id: String,
    pub primary_node_id: String,
    pub project_id: String,
    pub revision: String,
    pub files: BTreeMap<String, String>,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

fn valid_record_key(id: &str) -> bool {
    !id.is_empty() && id.len() <= 512 && id.bytes().all(|b| b != 0 && b != b'\n' && b != b'\r')
}

pub(crate) fn revision(snapshot: &Snapshot) -> Result<String, String> {
    let bytes = serde_json::to_vec(&(
        snapshot.schema_version,
        &snapshot.cluster_id,
        &snapshot.primary_node_id,
        &snapshot.project_id,
        &snapshot.files,
    ))
    .map_err(|e| e.to_string())?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn serialize<T: Serialize>(records: &[T]) -> Result<String, String> {
    serde_json::to_string(records).map_err(|e| e.to_string())
}

fn decode<T: for<'de> Deserialize<'de>>(snapshot: &Snapshot, path: &str) -> Result<Vec<T>, String> {
    let body = snapshot
        .files
        .get(path)
        .ok_or_else(|| format!("missing {path}"))?;
    serde_json::from_str(body).map_err(|e| format!("invalid {path}: {e}"))
}

/// Fully validate a remote payload before creating any replica directories.
/// Internal enabled and shared/local/private flags are data to replicate,
/// not export filters. Only the registered project and cluster are accepted.
pub fn validate(
    snapshot: &Snapshot,
    expected_cluster: &str,
    expected_project: &str,
) -> Result<(), String> {
    if !matches!(snapshot.schema_version, 2 | 3)
        || snapshot.cluster_id != expected_cluster
        || snapshot.project_id != expected_project
        || !valid_id(&snapshot.primary_node_id)
    {
        return Err("snapshot identity or protocol version mismatch".into());
    }
    let expected_files = if snapshot.schema_version == 3 {
        MAX_SNAPSHOT_FILES
    } else {
        3
    };
    if snapshot.files.len() != expected_files
        || snapshot.files.keys().any(|p| {
            !matches!(p.as_str(), MEMORY_PATH | RULE_PATH | SKILL_PATH)
                && !(snapshot.schema_version == 3 && p == ax_db::replication::DATA_PATH)
        })
    {
        return Err("snapshot contains missing or undeclared paths".into());
    }
    let mut total = 0usize;
    for body in snapshot.files.values() {
        total = total
            .checked_add(body.len())
            .ok_or("snapshot size overflow")?;
        if total > MAX_SNAPSHOT_BYTES {
            return Err("snapshot exceeds size limit".into());
        }
    }
    if snapshot.revision != revision(snapshot)? {
        return Err("snapshot revision mismatch".into());
    }

    if snapshot.schema_version == 3 {
        let items = crate::server_data::manifests(snapshot)?;
        if items.is_empty() {
            return Err("snapshot has no complete datasets".into());
        }
    }
    let memories: Vec<MemoryRow> = decode(snapshot, MEMORY_PATH)?;
    let rules: Vec<PolicyRuleRow> = decode(snapshot, RULE_PATH)?;
    let skills: Vec<PolicySkillRow> = decode(snapshot, SKILL_PATH)?;
    if [memories.len(), rules.len(), skills.len()]
        .iter()
        .any(|count| *count > MAX_RECORDS_PER_KIND)
    {
        return Err("snapshot exceeds record count limit".into());
    }
    let mut ids = HashSet::new();
    for item in &memories {
        if !valid_record_key(&item.id) || !ids.insert(&item.id) {
            return Err("duplicate or invalid memory ID".into());
        }
        if item
            .files
            .iter()
            .any(|f| std::path::Path::new(f).is_absolute())
        {
            return Err("memory file references must be relative".into());
        }
    }
    ids.clear();
    for item in &rules {
        if !valid_record_key(&item.id) || !ids.insert(&item.id) {
            return Err("duplicate or invalid rule ID".into());
        }
        if !item.source_path.is_empty() || item.stub_path.is_some() || item.root_id.is_some() {
            return Err("node snapshot must not expose rule source paths".into());
        }
    }
    ids.clear();
    for item in &skills {
        if !valid_record_key(&item.name) || !ids.insert(&item.name) {
            return Err("duplicate or invalid skill name".into());
        }
        if !item.source_path.is_empty() || item.stub_path.is_some() || item.root_id.is_some() {
            return Err("node snapshot must not expose skill source paths".into());
        }
    }
    Ok(())
}

/// Build an isolated project-state snapshot directly from the Ax database.
/// No policy-pack or memory-export API is called: the synchronization contract
/// retains disabled, unshared and pending rows and their flags.
pub async fn export(config: &ServerConfig, project_id: &str) -> Result<Snapshot, String> {
    if config.role != NodeRole::Primary {
        return Err("only primaries may publish node synchronization snapshots".into());
    }
    crate::server_exchange::publish(config, project_id, None).await
}

/// Export native and locally edited knowledge for contribution. Unmodified
/// imported copies are excluded using the exact projection ledger, preventing
/// replicas from feeding received records back around the cluster.
pub async fn export_native(config: &ServerConfig, project_id: &str) -> Result<Snapshot, String> {
    let project = config
        .projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or("project is not registered")?;
    let ax = ax_core::Ax::open(&project.path)
        .await
        .map_err(|e| e.to_string())?;
    let pool = ax.db_pool();

    let (mut memories, count) = ax_memory::store::list(pool, MAX_RECORDS_PER_KIND + 1, 0)
        .await
        .map_err(|e| e.to_string())?;
    if count > MAX_RECORDS_PER_KIND as i64 {
        return Err("too many memories for one snapshot".into());
    }
    let memory_ledger: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='ax_replica_memory_ledger')").fetch_one(pool).await.map_err(|e| e.to_string())?;
    if memory_ledger {
        let imported: Vec<(String, String)> =
            sqlx::query_as("SELECT local_id,last_applied FROM ax_replica_memory_ledger")
                .fetch_all(pool)
                .await
                .map_err(|e| e.to_string())?;
        let imported: BTreeMap<_, _> = imported.into_iter().collect();
        memories.retain(|row| {
            imported.get(&row.id).is_none_or(|old| {
                serde_json::from_str::<MemoryRow>(old)
                    .ok()
                    .is_none_or(|before| {
                        serde_json::to_value(&before).ok() != serde_json::to_value(row).ok()
                    })
            })
        });
    }
    memories.sort_by(|a, b| a.id.cmp(&b.id));
    // Never transmit host-absolute file paths, even if an imported memory
    // contains them. The memory body is data controlled by the enrolled owner.
    for item in &mut memories {
        item.files
            .retain(|f| !std::path::Path::new(f).is_absolute());
    }

    let mut rules = ax_policy::list_rules(pool)
        .await
        .map_err(|e| e.to_string())?;
    let mut skills = ax_policy::list_skills(pool)
        .await
        .map_err(|e| e.to_string())?;
    if rules.len() > MAX_RECORDS_PER_KIND || skills.len() > MAX_RECORDS_PER_KIND {
        return Err("too many policy records for one snapshot".into());
    }
    let policy_ledger: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='ax_replica_policy_imports')").fetch_one(pool).await.map_err(|e| e.to_string())?;
    if policy_ledger {
        let imported: Vec<(String, String)> =
            sqlx::query_as("SELECT kind,record_key FROM ax_replica_policy_imports")
                .fetch_all(pool)
                .await
                .map_err(|e| e.to_string())?;
        let imported: HashSet<_> = imported.into_iter().collect();
        rules.retain(|row| {
            !imported.contains(&("rule".into(), row.id.clone()))
                || !crate::server_exchange::unedited_policy_import(
                    &serde_json::to_value(row).expect("rule serializes"),
                )
        });
        skills.retain(|row| {
            !imported.contains(&("skill".into(), row.name.clone()))
                || !crate::server_exchange::unedited_policy_import(
                    &serde_json::to_value(row).expect("skill serializes"),
                )
        });
    }
    rules.sort_by(|a, b| a.id.cmp(&b.id));
    skills.sort_by(|a, b| a.name.cmp(&b.name));

    // Source paths and root IDs belong to the primary machine, not a replica.
    for item in &mut rules {
        item.source_path.clear();
        item.stub_path = None;
        item.root_id = None;
        item.effective_storage.clear();
        item.storage_is_override = false;
    }
    for item in &mut skills {
        item.source_path.clear();
        item.stub_path = None;
        item.root_id = None;
        item.effective_storage.clear();
        item.storage_is_override = false;
    }

    ax_usage::migrate_legacy_log(Some(&project.path)).map_err(|e| e.to_string())?;
    let mut dataset = ax_db::replication::export(&project.path, &config.node_id, pool)
        .await
        .map_err(|e| e.to_string())?;
    let history = ax_usage::replication::export(&project.path).await?;
    let root = project.path.clone();
    let file = history.path.clone();
    dataset.history = Some(
        tokio::task::spawn_blocking(move || ax_db::replication::file_manifest(&root, &file))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?,
    );
    dataset.history_rows = history.rows;
    ax_db::replication::pin(&project.path, &dataset).map_err(|e| e.to_string())?;
    let files = BTreeMap::from([
        (MEMORY_PATH.to_string(), serialize(&memories)?),
        (RULE_PATH.to_string(), serialize(&rules)?),
        (SKILL_PATH.to_string(), serialize(&skills)?),
        (
            ax_db::replication::DATA_PATH.into(),
            serde_json::to_string(&vec![dataset]).map_err(|e| e.to_string())?,
        ),
    ]);
    let mut snapshot = Snapshot {
        schema_version: 3,
        cluster_id: config.cluster_id.clone(),
        primary_node_id: config.node_id.clone(),
        project_id: project_id.to_owned(),
        revision: String::new(),
        files,
    };
    snapshot.revision = revision(&snapshot)?;
    validate(&snapshot, &config.cluster_id, project_id)?;
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_snapshot() -> Snapshot {
        let files = BTreeMap::from([
            (MEMORY_PATH.to_string(), "[]".to_string()),
            (RULE_PATH.to_string(), "[]".to_string()),
            (SKILL_PATH.to_string(), "[]".to_string()),
        ]);
        let mut item = Snapshot {
            schema_version: 2,
            cluster_id: "cluster".into(),
            primary_node_id: "primary".into(),
            project_id: "project".into(),
            revision: String::new(),
            files,
        };
        item.revision = revision(&item).unwrap();
        item
    }

    #[test]
    fn stable_revision_for_identical_state() {
        let a = empty_snapshot();
        let b = empty_snapshot();
        assert_eq!(a.revision, b.revision);
        validate(&a, "cluster", "project").unwrap();
    }

    #[test]
    fn rejects_cross_cluster_and_tampering() {
        let mut item = empty_snapshot();
        assert!(validate(&item, "other-cluster", "project").is_err());
        item.files.insert(MEMORY_PATH.into(), "[{}]".into());
        assert!(validate(&item, "cluster", "project").is_err());
    }

    #[test]
    fn rejects_path_injection_even_with_correct_revision() {
        let mut item = empty_snapshot();
        item.files.insert("../other".into(), "x".into());
        item.revision = revision(&item).unwrap();
        assert!(validate(&item, "cluster", "project").is_err());
    }

    #[test]
    fn reject_unexpected_export_pack_format() {
        let mut item = empty_snapshot();
        item.files.remove(MEMORY_PATH);
        item.files
            .insert("memory/shared.jsonl".into(), String::new());
        item.revision = revision(&item).unwrap();
        assert!(validate(&item, "cluster", "project").is_err());
    }

    #[test]
    fn preserves_disabled_and_unshared_memory_in_node_sync() {
        let mut item = empty_snapshot();
        let rows = vec![MemoryRow {
            id: "private-1".into(),
            kind: "note".into(),
            title: "Private".into(),
            body: "Replicated inside an authenticated node cluster".into(),
            tags: vec!["local".into(), "noshare".into()],
            files: vec![],
            confidence: 1.0,
            source: "manual".into(),
            enabled: false,
            created_at: 1,
            updated_at: 1,
        }];
        item.files
            .insert(MEMORY_PATH.into(), serialize(&rows).unwrap());
        item.revision = revision(&item).unwrap();
        validate(&item, "cluster", "project").unwrap();
        let decoded: Vec<MemoryRow> = decode(&item, MEMORY_PATH).unwrap();
        assert!(!decoded[0].enabled);
        assert!(decoded[0].tags.contains(&"noshare".to_owned()));
    }
}
