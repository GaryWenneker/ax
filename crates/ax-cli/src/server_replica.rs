//! Replica pull and staging. No local/private memory records are overwritten.
//!
//! Snapshots are verified in memory before disk I/O. The replica keeps each
//! revision in a separate namespace, with a durable pointer to the selected
//! revision. Policy activation always requires separate review.

use crate::{
    server_cli,
    server_config::{NodeRole, ProjectRegistration, ServerConfig},
    server_snapshot::{self, Snapshot},
};
use std::{fs, io::Write, path::Path, time::Duration};

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
        "status": if staged { "staged_pending_review" } else { "unchanged" },
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

fn stage(config: &ServerConfig, project: &ProjectRegistration, snapshot: &Snapshot) -> Result<String, String> {
    server_snapshot::validate(snapshot, &config.cluster_id, &project.id)?;
    let root = mirror_directory(config, project, snapshot)?;
    let current = root.join("current.json");
    if let Ok(selected) = fs::read_to_string(&current) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&selected) {
            if value.get("revision").and_then(|x| x.as_str()) == Some(&snapshot.revision) {
                save_selected(&root, &snapshot.revision, false)?;
                return Ok("unchanged".into());
            }
        }
    }
    let revision_path = root.join(&snapshot.revision);
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
    } else if fs::symlink_metadata(&revision_path).map_err(|e| e.to_string())?.file_type().is_symlink() {
        return Err("replica revision directory cannot be a symlink".into());
    }
    save_selected(&root, &snapshot.revision, true)?;
    Ok("staged_pending_review".into())
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
        let result = match fetch_snapshot(&client, &config, project).await {
            Ok(snapshot) => stage(&config, project, &snapshot),
            Err(err) => Err(err),
        };
        match result {
            Ok(status) => println!("Project {}: {}", project.id, status),
            Err(error) => {
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
    let base = project.path.join(".ax/server/replicas").join(&config.cluster_id).join(&project.id);
    let latest = fs::read_dir(base).ok().into_iter().flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| fs::read_to_string(entry.path().join("current.json")).ok())
        .filter_map(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .max_by_key(|value| value.get("last_sync_unix_seconds").and_then(|v| v.as_u64()).unwrap_or(0));
    latest.unwrap_or_else(|| serde_json::json!({"status": "never_synced"}))
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
