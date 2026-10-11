//! Versioned, bounded snapshots of explicitly shareable knowledge.
//!
//! A snapshot deliberately contains no database, credentials, private policy,
//! local cache or working context. All paths are from a strict allow-list.

use crate::server_config::ServerConfig;
use ax_memory::types::MemoryRow;
use ax_policy::pack::PackManifest;
use serde::{Deserialize, Serialize};
use std::{collections::{BTreeMap, HashSet}, fs, path::Path};

pub const MAX_SNAPSHOT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_SNAPSHOT_FILES: usize = 512;
pub const MAX_MEMORY_ROWS: usize = 10_000;

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
    !id.is_empty() && id.len() <= 128
        && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

fn allowed_path(path: &str) -> bool {
    if path == "policy/manifest.json" || path == "memory/shared.jsonl" {
        return true;
    }
    if let Some(id) = path.strip_prefix("policy/rules/").and_then(|p| p.strip_suffix(".mdc")) {
        return valid_id(id);
    }
    if let Some(id) = path.strip_prefix("policy/skills/").and_then(|p| p.strip_suffix("/SKILL.md")) {
        return valid_id(id);
    }
    false
}

fn content_revision(snapshot: &Snapshot) -> Result<String, String> {
    let bytes = serde_json::to_vec(&(
        snapshot.schema_version,
        &snapshot.cluster_id,
        &snapshot.primary_node_id,
        &snapshot.project_id,
        &snapshot.files,
    )).map_err(|e| e.to_string())?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn shared_memory(row: &MemoryRow) -> bool {
    row.enabled
        && !matches!(row.kind.as_str(), "turn")
        && row.tags.iter().any(|t| t.eq_ignore_ascii_case("shared"))
        && !row.tags.iter().any(|t| t.eq_ignore_ascii_case("local") || t.eq_ignore_ascii_case("noshare"))
}

fn validate_memory_jsonl(raw: &str) -> Result<(), String> {
    if raw.lines().count() > MAX_MEMORY_ROWS {
        return Err("too many shared memories".into());
    }
    let mut ids = HashSet::new();
    for (index, line) in raw.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let row: MemoryRow = serde_json::from_str(line)
            .map_err(|e| format!("invalid memory row {}: {e}", index + 1))?;
        if !shared_memory(&row) || row.id.is_empty() || !ids.insert(row.id) {
            return Err("snapshot includes an unshareable or duplicate memory".into());
        }
    }
    Ok(())
}

/// Validate all metadata and hashes before any materialization or import.
pub fn validate(snapshot: &Snapshot, expected_cluster: &str, expected_project: &str) -> Result<(), String> {
    if snapshot.schema_version != 1
        || snapshot.cluster_id != expected_cluster
        || snapshot.project_id != expected_project
        || !valid_id(&snapshot.primary_node_id)
    {
        return Err("snapshot identity or schema mismatch".into());
    }
    if snapshot.files.len() > MAX_SNAPSHOT_FILES {
        return Err("too many snapshot files".into());
    }
    let mut total = 0usize;
    for (path, body) in &snapshot.files {
        if !allowed_path(path) {
            return Err(format!("unsafe snapshot file path: {path}"));
        }
        total = total.checked_add(body.len()).ok_or("snapshot size overflow")?;
        if total > MAX_SNAPSHOT_BYTES {
            return Err("snapshot exceeds size limit".into());
        }
    }
    if snapshot.revision != content_revision(snapshot)? {
        return Err("snapshot revision mismatch".into());
    }
    let raw_manifest = snapshot.files.get("policy/manifest.json").ok_or("missing policy manifest")?;
    let manifest: PackManifest = serde_json::from_str(raw_manifest)
        .map_err(|e| format!("invalid policy manifest: {e}"))?;
    if manifest.schema_version != 1 || manifest.tag != "shared" {
        return Err("unsupported policy manifest".into());
    }
    let mut expected_files: HashSet<String> = HashSet::from([
        "policy/manifest.json".into(),
        "memory/shared.jsonl".into(),
    ]);
    for (items, is_rule) in [(&manifest.rules, true), (&manifest.skills, false)] {
        for item in items {
            if !valid_id(&item.id) || item.kind != if is_rule { "rule" } else { "skill" } {
                return Err("unsafe policy manifest item".into());
            }
            let path = if is_rule {
                format!("policy/rules/{}.mdc", item.id)
            } else {
                format!("policy/skills/{}/SKILL.md", item.id)
            };
            if !expected_files.insert(path.clone()) {
                return Err("duplicate policy manifest item".into());
            }
            let raw = snapshot.files.get(&path).ok_or("missing policy file")?;
            if blake3::hash(raw.as_bytes()).to_hex().to_string() != item.content_hash {
                return Err(format!("policy content hash mismatch: {path}"));
            }
        }
    }
    if snapshot.files.keys().any(|p| !expected_files.contains(p)) {
        return Err("snapshot includes undeclared files".into());
    }
    let memory = snapshot.files.get("memory/shared.jsonl").ok_or("missing memory JSONL")?;
    validate_memory_jsonl(memory)?;
    Ok(())
}

fn read_bounded(path: &Path) -> Result<String, String> {
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.file_type().is_file() || meta.len() > MAX_SNAPSHOT_BYTES as u64 {
        return Err("snapshot export is not a bounded regular file".into());
    }
    fs::read_to_string(path).map_err(|e| e.to_string())
}

/// Export only from an explicitly registered project, with a private temp directory.
pub async fn export(config: &ServerConfig, project_id: &str) -> Result<Snapshot, String> {
    if config.role != crate::server_config::NodeRole::Primary {
        return Err("replicas cannot publish snapshots".into());
    }
    let project = config.projects.iter().find(|p| p.id == project_id)
        .ok_or("project is not registered")?;
    let ax = ax_core::Ax::open(&project.path).await.map_err(|e| e.to_string())?;
    let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let policy_root = temp.path().join("policy");
    ax_policy::export_pack(ax.db_pool(), &project.path, "shared", Some(&policy_root))
        .await.map_err(|e| e.to_string())?;

    // Pack export timestamps would make every otherwise unchanged snapshot
    // appear different. Normalize timestamps, preserving content hashes.
    let manifest_path = policy_root.join("manifest.json");
    let mut manifest: PackManifest = serde_json::from_str(&read_bounded(&manifest_path)?)
        .map_err(|e| e.to_string())?;
    manifest.exported_at = 0;
    for item in manifest.rules.iter_mut().chain(manifest.skills.iter_mut()) {
        item.updated_at = 0;
    }
    fs::write(&manifest_path, format!("{}\n", serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?))
        .map_err(|e| e.to_string())?;

    let memory_path = temp.path().join("memory/shared.jsonl");
    ax_memory::sync::export_shared(ax.db_pool(), &project.path, "shared", Some(&memory_path))
        .await.map_err(|e| e.to_string())?;
    let mut rows: Vec<MemoryRow> = read_bounded(&memory_path)?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<MemoryRow>(line).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    rows.retain(shared_memory);
    rows.sort_by(|a, b| a.id.cmp(&b.id));
    if rows.len() > MAX_MEMORY_ROWS {
        return Err("too many shared memories".into());
    }
    let mut memory = String::new();
    for row in rows {
        memory.push_str(&serde_json::to_string(&row).map_err(|e| e.to_string())?);
        memory.push('\n');
    }
    fs::write(&memory_path, memory).map_err(|e| e.to_string())?;

    let mut files = BTreeMap::new();
    files.insert("policy/manifest.json".into(), read_bounded(&manifest_path)?);
    for rule in &manifest.rules {
        if !valid_id(&rule.id) { return Err("invalid exported rule identifier".into()); }
        let relative = format!("policy/rules/{}.mdc", rule.id);
        files.insert(relative, read_bounded(&policy_root.join("rules").join(format!("{}.mdc", rule.id)))?);
    }
    for skill in &manifest.skills {
        if !valid_id(&skill.id) { return Err("invalid exported skill identifier".into()); }
        let relative = format!("policy/skills/{}/SKILL.md", skill.id);
        files.insert(relative, read_bounded(&policy_root.join("skills").join(&skill.id).join("SKILL.md"))?);
    }
    files.insert("memory/shared.jsonl".into(), read_bounded(&memory_path)?);
    let mut snapshot = Snapshot {
        schema_version: 1,
        cluster_id: config.cluster_id.clone(),
        primary_node_id: config.node_id.clone(),
        project_id: project_id.into(),
        revision: String::new(),
        files,
    };
    snapshot.revision = content_revision(&snapshot)?;
    validate(&snapshot, &config.cluster_id, project_id)?;
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> Snapshot {
        let manifest = PackManifest {
            schema_version: 1,
            exported_at: 0,
            tag: "shared".into(),
            rules: vec![],
            skills: vec![],
        };
        let files = BTreeMap::from([
            ("policy/manifest.json".into(), serde_json::to_string(&manifest).unwrap()),
            ("memory/shared.jsonl".into(), String::new()),
        ]);
        let mut snapshot = Snapshot {
            schema_version: 1,
            cluster_id: "cluster".into(),
            primary_node_id: "primary".into(),
            project_id: "project".into(),
            revision: String::new(),
            files,
        };
        snapshot.revision = content_revision(&snapshot).unwrap();
        snapshot
    }

    #[test]
    fn unchanged_content_has_stable_revision() {
        let a = snapshot();
        let b = snapshot();
        assert_eq!(a.revision, b.revision);
        validate(&a, "cluster", "project").unwrap();
    }

    #[test]
    fn refuses_path_traversal_even_with_matching_revision() {
        let mut s = snapshot();
        s.files.insert("policy/rules/../../outside.mdc".into(), "secret".into());
        s.revision = content_revision(&s).unwrap();
        assert!(validate(&s, "cluster", "project").is_err());
    }

    #[test]
    fn refuses_tampering_and_cross_cluster_content() {
        let mut s = snapshot();
        s.files.insert("memory/shared.jsonl".into(), "{}\n".into());
        assert!(validate(&s, "cluster", "project").is_err());
        assert!(validate(&snapshot(), "wrong", "project").is_err());
    }

    #[test]
    fn rejects_invalid_manifest_ids_and_undeclared_files() {
        let mut s = snapshot();
        s.files.insert("policy/rules/orphan.mdc".into(), "body".into());
        s.revision = content_revision(&s).unwrap();
        assert!(validate(&s, "cluster", "project").is_err());
    }
}
