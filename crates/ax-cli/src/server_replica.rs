//! Replica pull and staging. Local project records are never overwritten.
//!
//! Snapshots are verified in memory before disk I/O. The replica keeps each
//! revision in a separate namespace, with a durable pointer to the selected
//! revision. Policy activation always requires separate review.

use crate::{
    server_cli,
    server_config::{NodeRole, ProjectRegistration, ServerConfig},
    server_snapshot::{self, Snapshot},
};
use std::{fs, io::Write, path::{Path, PathBuf}, time::Duration};

const MAX_WIRE_BYTES: usize = server_snapshot::MAX_SNAPSHOT_BYTES + 1024 * 1024;

fn safe_dir(path: &Path) -> Result<(), String> {
    if path.exists() {
        let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err(format!("replica storage must be a real directory: {}", path.display()));
        }
    } else {
        fs::create_dir(path).map_err(|e| format!("cannot create replica directory {}: {e}", path.display()))?;
    }
    Ok(())
}

fn mirror_directory(config: &ServerConfig, project: &ProjectRegistration, snapshot: &Snapshot) -> Result<PathBuf, String> {
    // Each component is a validated identifier; do not use URL or manifest paths.
    let ax_root = project.path.join(".ax");
    safe_dir(&ax_root)?;
    let mut current = ax_root;
    for part in ["server", "replicas", &config.cluster_id, &project.id, &snapshot.primary_node_id] {
        current = current.join(part);
        safe_dir(&current)?;
    }
    Ok(current)
}

fn save_selected(root: &Path, revision: &str, staged: bool) -> Result<(), String> {
    let selected = serde_json::json!({
        "revision": revision,
        "status": if staged { "mirrored_not_applied" } else { "unchanged" },
        "last_sync_unix_seconds": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs(),
    });
    let mut file = tempfile::NamedTempFile::new_in(root).map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(&mut file, &selected).map_err(|e| e.to_string())?;
    file.write_all(b"\n").map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(root.join("current.json")).map_err(|e| e.error.to_string())?;
    Ok(())
}

/// Never trust an on-disk revision just because its directory name matches
/// the incoming digest. Symlinks and tampered files must not be treated as
/// synchronized content.
fn verify_staged_revision(directory: &Path, snapshot: &Snapshot) -> Result<(), String> {
    let meta = fs::symlink_metadata(directory).map_err(|e| e.to_string())?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("existing revision path is not a real directory".into());
    }
    for (path, expected) in &snapshot.files {
        let target = directory.join(path);
        let meta = fs::symlink_metadata(&target).map_err(|e| e.to_string())?;
        if !meta.file_type().is_file() || meta.len() != expected.len() as u64 {
            return Err(format!("staged revision file is missing or unsafe: {path}"));
        }
        let actual = fs::read_to_string(&target).map_err(|e| e.to_string())?;
        if actual != *expected {
            return Err(format!("staged revision content is inconsistent: {path}"));
        }
    }
    Ok(())
}

fn stage(config: &ServerConfig, project: &ProjectRegistration, snapshot: &Snapshot) -> Result<String, String> {
    server_snapshot::validate(snapshot, &config.cluster_id, &project.id)?;
    let root = mirror_directory(config, project, snapshot)?;
    let current = root.join("current.json");
    let revision_path = root.join(&snapshot.revision);
    if let Ok(selected) = fs::read_to_string(&current) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&selected) {
            if value.get("revision").and_then(|x| x.as_str()) == Some(snapshot.revision.as_str()) {
                verify_staged_revision(&revision_path, snapshot)?;
                save_selected(&root, &snapshot.revision, false)?;
                return Ok("unchanged".into());
            }
        }
    }
    if !revision_path.exists() {
        let temp = tempfile::Builder::new().prefix(".staging-").tempdir_in(&root).map_err(|e| e.to_string())?;
        for (relative, contents) in &snapshot.files {
            let target = temp.path().join(relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut file = fs::File::create(&target).map_err(|e| e.to_string())?;
            file.write_all(contents.as_bytes()).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
        }
        fs::rename(temp.path(), &revision_path).map_err(|e| e.to_string())?;
    }
    verify_staged_revision(&revision_path, snapshot)?;
    save_selected(&root, &snapshot.revision, true)?;
    Ok("mirrored_not_applied".into())
}

fn attempt_file(config: &ServerConfig, project: &ProjectRegistration) -> std::path::PathBuf {
    project.path.join(".ax/server/sync-status")
        .join(&config.cluster_id)
        .join(format!("{}.json", project.id))
}

/// Retain successful sync metadata when a subsequent network attempt fails.
/// Status must describe the current attempt, not claim the listener is running.
fn record_attempt(
    config: &ServerConfig,
    project: &ProjectRegistration,
    outcome: &Result<(String, String), String>,
) -> Result<(), String> {
    let dir = project.path.join(".ax");
    safe_dir(&dir)?;
    let dir = dir.join("server");
    safe_dir(&dir)?;
    let dir = dir.join("sync-status");
    safe_dir(&dir)?;
    let dir = dir.join(&config.cluster_id);
    safe_dir(&dir)?;
    let file_path = attempt_file(config, project);
    let previous: serde_json::Value = fs::read_to_string(&file_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    let mut state = serde_json::json!({
        "last_attempt_unix_seconds": now,
        "last_success_revision": previous.get("last_success_revision").cloned(),
        "last_success_unix_seconds": previous.get("last_success_unix_seconds").cloned(),
        "last_error": serde_json::Value::Null,
    });
    match outcome {
        Ok((status, revision)) => {
            state["status"] = serde_json::json!(status);
            state["last_success_revision"] = serde_json::json!(revision);
            state["last_success_unix_seconds"] = serde_json::json!(now);
        }
        Err(error) => {
            state["status"] = serde_json::json!("failed");
            state["last_error"] = serde_json::json!(error);
        }
    }
    let mut temp = tempfile::NamedTempFile::new_in(&dir).map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(&mut temp, &state).map_err(|e| e.to_string())?;
    temp.write_all(b"\n").map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(&file_path).map_err(|e| e.error.to_string())?;
    Ok(())
}

async fn fetch_snapshot(
    client: &reqwest::Client,
    config: &ServerConfig,
    project: &ProjectRegistration,
) -> Result<Snapshot, String> {
    let token = std::env::var(&project.token_env)
        .map_err(|_| format!("missing project token in environment variable {}", project.token_env))?;
    if token.len() < 32 || !token.is_ascii() || token.bytes().any(|b| b.is_ascii_whitespace()) {
        return Err(format!("{} must contain at least 32 non-whitespace ASCII characters", project.token_env));
    }
    let base = config.primary_url.as_deref().ok_or("replica requires primary_url")?;
    let url = reqwest::Url::parse(base).map_err(|e| e.to_string())?
        .join(&format!("/v1/projects/{}/snapshot", project.id)).map_err(|e| e.to_string())?;
    let mut response = client.get(url).bearer_auth(token).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("snapshot request returned HTTP {}", response.status()));
    }
    if response.content_length().is_some_and(|n| n > MAX_WIRE_BYTES as u64) {
        return Err("snapshot response too large".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if bytes.len().checked_add(chunk.len()).is_none_or(|n| n > MAX_WIRE_BYTES) {
            return Err("snapshot response exceeded wire limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let snapshot: Snapshot = serde_json::from_slice(&bytes).map_err(|e| format!("invalid snapshot JSON: {e}"))?;
    server_snapshot::validate(&snapshot, &config.cluster_id, &project.id)?;
    Ok(snapshot)
}

pub async fn sync(config_path: &Path) -> Result<(), String> {
    sync_with_apply(config_path, false).await
}

/// Projection is opt-in; transport always stages a verified separate mirror.
pub async fn sync_with_apply(config_path: &Path, apply_memories: bool) -> Result<(), String> {
    let config = server_cli::load(config_path)?;
    if config.role != NodeRole::Replica {
        return Err("only replicas can run ax server sync".into());
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let mut errors = Vec::new();
    for project in &config.projects {
        let result: Result<(String, String), String> =
            match fetch_snapshot(&client, &config, project).await {
                Ok(snapshot) => match stage(&config, project, &snapshot) {
                    Ok(status) if apply_memories => {
                        match crate::server_apply::apply(&config, &snapshot).await {
                            Ok(report) if report.conflicts == 0 => Ok((
                                format!(
                                    "{status}; memories: +{} updated {} removed {} unchanged {}",
                                    report.inserted, report.updated, report.removed, report.unchanged
                                ),
                                snapshot.revision,
                            )),
                            Ok(report) => Err(format!(
                                "memory projection found {} locally edited/conflicting records; staged mirror is safe but must be reviewed",
                                report.conflicts
                            )),
                            Err(error) => Err(error),
                        }
                    }
                    Ok(status) => Ok((status, snapshot.revision)),
                    Err(error) => Err(error),
                },
                Err(error) => Err(error),
            };
        let recorded = record_attempt(&config, project, &result);
        match (result, recorded) {
            (Ok((status, _)), Ok(())) => println!("Project {}: {}", project.id, status),
            (Err(error), _) | (_, Err(error)) => {
                eprintln!("Project {}: sync failed: {}", project.id, error);
                errors.push(project.id.clone());
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!("synchronization failed for {} project(s)", errors.len()))
    }
}

pub fn status(config: &ServerConfig, project: &ProjectRegistration) -> serde_json::Value {
    fs::read_to_string(attempt_file(config, project))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| serde_json::json!({"status": "never_synced"}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn no_unvalidated_content_is_staged() {
        let tmp = tempfile::tempdir().unwrap();
        let project_path = tmp.path().join("project");
        fs::create_dir_all(project_path.join(".ax")).unwrap();
        let project = ProjectRegistration {id:"sample".into(), path:project_path.clone(), token_env:"AX_SERVER_SAMPLE_TOKEN".into()};
        let config = ServerConfig {
            schema_version:1, cluster_id:"cluster".into(), node_id:"replica".into(),
            role:NodeRole::Replica, primary_url:Some("https://example.com".into()), projects:vec![project.clone()]
        };
        let snapshot = Snapshot {
            schema_version:1, cluster_id:"cluster".into(), primary_node_id:"primary".into(), project_id:"sample".into(),
            revision:"invalid".into(), files:BTreeMap::new(),
        };
        assert!(stage(&config, &project, &snapshot).is_err());
        assert!(!project_path.join(".ax/server").exists());
    }
}
