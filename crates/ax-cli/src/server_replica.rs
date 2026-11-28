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
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

/// One replica synchronization at a time in this process. The periodic poll
/// and a UDP change hint both call [`sync_with_apply`]; without this they post
/// two exchanges back to back and stage the same revision at the same time.
static SYNC_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A `.staging-*` folder older than this belongs to a run that crashed.
const STALE_STAGING: Duration = Duration::from_secs(3600);

const MAX_WIRE_BYTES: usize = server_snapshot::MAX_SNAPSHOT_BYTES + 1024 * 1024;

fn safe_dir(path: &Path) -> Result<(), String> {
    match fs::create_dir(path) {
        Ok(()) => return Ok(()),
        // Another sync may have created it a moment ago; check what is there.
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => {
            return Err(format!(
                "cannot create replica directory {}: {error}",
                path.display()
            ))
        }
    }
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(format!(
            "replica storage must be a real directory: {}",
            path.display()
        ));
    }
    Ok(())
}

fn mirror_directory(
    config: &ServerConfig,
    project: &ProjectRegistration,
    snapshot: &Snapshot,
) -> Result<PathBuf, String> {
    // Each component is a validated identifier; do not use URL or manifest paths.
    let ax_root = project.path.join(".ax");
    safe_dir(&ax_root)?;
    let mut current = ax_root;
    for part in [
        "server",
        "replicas",
        &config.cluster_id,
        &project.id,
        &snapshot.primary_node_id,
    ] {
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
    file.persist(root.join("current.json"))
        .map_err(|e| e.error.to_string())?;
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

/// Remove revision folders other than the selected one, and staging folders
/// left by runs that crashed. Only real directories whose names Ax created are
/// touched. A failure is logged and leaves the folder for the next sync.
fn prune_revisions(root: &Path, selected: &str) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Ok(meta) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if !meta.is_dir() || meta.file_type().is_symlink() || name == selected {
            continue;
        }
        let revision = name.len() == 64 && name.bytes().all(|b| b.is_ascii_hexdigit());
        let stale_staging = name.starts_with(".staging-")
            && meta
                .modified()
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|age| age > STALE_STAGING);
        if revision || stale_staging {
            if let Err(error) = fs::remove_dir_all(entry.path()) {
                tracing::warn!(path = %entry.path().display(), "cannot remove old replica revision: {}", error);
            }
        }
    }
}

fn stage(
    config: &ServerConfig,
    project: &ProjectRegistration,
    snapshot: &Snapshot,
) -> Result<String, String> {
    server_snapshot::validate(snapshot, &config.cluster_id, &project.id)?;
    let root = mirror_directory(config, project, snapshot)?;
    let current = root.join("current.json");
    let revision_path = root.join(&snapshot.revision);
    if let Ok(selected) = fs::read_to_string(&current) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&selected) {
            if value.get("revision").and_then(|x| x.as_str()) == Some(snapshot.revision.as_str()) {
                verify_staged_revision(&revision_path, snapshot)?;
                save_selected(&root, &snapshot.revision, false)?;
                prune_revisions(&root, &snapshot.revision);
                return Ok("unchanged".into());
            }
        }
    }
    if !revision_path.exists() {
        let temp = tempfile::Builder::new()
            .prefix(".staging-")
            .tempdir_in(&root)
            .map_err(|e| e.to_string())?;
        for (relative, contents) in &snapshot.files {
            let target = temp.path().join(relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut file = fs::File::create(&target).map_err(|e| e.to_string())?;
            file.write_all(contents.as_bytes())
                .map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
        }
        // A concurrent stage of the same revision may have won the rename; its
        // folder is accepted when it verifies below.
        if let Err(error) = fs::rename(temp.path(), &revision_path) {
            if verify_staged_revision(&revision_path, snapshot).is_err() {
                return Err(error.to_string());
            }
        }
    }
    verify_staged_revision(&revision_path, snapshot)?;
    save_selected(&root, &snapshot.revision, true)?;
    prune_revisions(&root, &snapshot.revision);
    Ok("mirrored_not_applied".into())
}

fn attempt_file(config: &ServerConfig, project: &ProjectRegistration) -> std::path::PathBuf {
    project
        .path
        .join(".ax/server/sync-status")
        .join(&config.cluster_id)
        .join(format!("{}.json", project.id))
}

/// Retain successful sync metadata when a subsequent network attempt fails.
/// Status must describe the current attempt, not claim the listener is running.
pub(crate) fn record_attempt(
    config: &ServerConfig,
    project: &ProjectRegistration,
    outcome: &Result<(String, String), String>,
    warning: Option<&str>,
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
        "compatibility_warning": warning,
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

/// Optional transport metadata never changes snapshot fingerprints.
pub(crate) fn peer_warning(headers: &reqwest::header::HeaderMap) -> Result<Option<String>, String> {
    let version = headers
        .get("x-ax-version")
        .and_then(|v| v.to_str().ok())
        .filter(|v| {
            v.len() <= 64
                && v.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b".-+_".contains(&c))
        })
        .unwrap_or("unknown");
    if let Some(protocol) = headers.get("x-ax-replication-protocol") {
        if protocol != "3" {
            let value = protocol
                .to_str()
                .ok()
                .filter(|v| v.len() <= 8 && v.bytes().all(|c| c.is_ascii_digit()))
                .unwrap_or("unknown");
            return Err(format!("Replication protocol {value} is incompatible with local protocol 3 (peer Ax {version}, local Ax {}). Update the older node before syncing; local data was preserved.", env!("CARGO_PKG_VERSION")));
        }
    }
    Ok((version != env!("CARGO_PKG_VERSION")).then(|| format!("Peer Ax {version}; local Ax {}. Complete replication protocol 3 is compatible, but versions differ or are unknown. Update and restart the older node for matching features.",env!("CARGO_PKG_VERSION"))))
}

pub(crate) async fn fetch_snapshot(
    client: &reqwest::Client,
    config: &ServerConfig,
    project: &ProjectRegistration,
) -> Result<(Snapshot, usize, Option<String>), String> {
    let token = std::env::var(&project.token_env).map_err(|_| {
        format!(
            "missing project token in environment variable {}",
            project.token_env
        )
    })?;
    if token.len() < 32 || !token.is_ascii() || token.bytes().any(|b| b.is_ascii_whitespace()) {
        return Err(format!(
            "{} must contain at least 32 non-whitespace ASCII characters",
            project.token_env
        ));
    }
    let base = config
        .primary_url
        .as_deref()
        .ok_or("replica requires primary_url")?;
    let url = reqwest::Url::parse(base)
        .map_err(|e| e.to_string())?
        .join(&format!("/v1/projects/{}/snapshot", project.id))
        .map_err(|e| e.to_string())?;
    let response = client
        .get(url)
        .bearer_auth(&token)
        .header("x-ax-node-id", &config.node_id)
        .header("x-ax-version", env!("CARGO_PKG_VERSION"))
        .header("x-ax-replication-protocol", "3")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "snapshot request returned HTTP {}",
            response.status()
        ));
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_WIRE_BYTES as u64)
    {
        return Err("snapshot response too large".into());
    }
    let warning = peer_warning(response.headers())?;
    let snapshot: Snapshot = read_response(response).await?;
    if snapshot.schema_version != 3 {
        return Err(format!("Peer snapshot protocol {} cannot synchronize all datasets; local protocol is 3. Update the older node. Local data was preserved.", snapshot.schema_version));
    }
    server_snapshot::validate(&snapshot, &config.cluster_id, &project.id)?;
    crate::server_data::manifests(&snapshot)?;
    let local = server_snapshot::export_native(config, &project.id).await?;
    crate::server_data::upload(client, config, project, &local).await?;
    let exchange_url = reqwest::Url::parse(base)
        .map_err(|e| e.to_string())?
        .join(&format!("/v1/projects/{}/exchange", project.id))
        .map_err(|e| e.to_string())?;
    let response = client
        .post(exchange_url)
        .bearer_auth(&token)
        .header("x-ax-node-id", &config.node_id)
        .header("x-ax-version", env!("CARGO_PKG_VERSION"))
        .header("x-ax-replication-protocol", "3")
        .json(&local)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if matches!(response.status().as_u16(), 404 | 405) {
        return Err("Primary does not support preserve-and-exchange synchronization; update both nodes. Local records were preserved.".into());
    }
    if !response.status().is_success() {
        return Err(format!(
            "node exchange returned HTTP {}; local data was preserved",
            response.status()
        ));
    }
    let exchange_warning = peer_warning(response.headers())?;
    let reply: crate::server_exchange::ExchangeReply = read_response(response).await?;
    if reply.contributed_revision != local.revision {
        return Err("Primary did not acknowledge the exact contributed revision".into());
    }
    if reply.snapshot.primary_node_id != snapshot.primary_node_id {
        return Err("Primary identity changed during node exchange".into());
    }
    server_snapshot::validate(&reply.snapshot, &config.cluster_id, &project.id)?;
    crate::server_data::download(client, config, project, &reply.snapshot).await?;
    Ok((
        reply.snapshot,
        reply.memory_conflicts,
        exchange_warning.or(warning),
    ))
}

async fn read_response<T: serde::de::DeserializeOwned>(
    mut response: reqwest::Response,
) -> Result<T, String> {
    if response
        .content_length()
        .is_some_and(|n| n > MAX_WIRE_BYTES as u64)
    {
        return Err("node response too large".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if bytes
            .len()
            .checked_add(chunk.len())
            .is_none_or(|n| n > MAX_WIRE_BYTES)
        {
            return Err("node response exceeded wire limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("invalid node response: {e}"))
}

pub async fn sync(config_path: &Path) -> Result<(), String> {
    sync_with_apply(config_path, false).await
}

/// Projection is opt-in; transport always stages a verified separate mirror.
pub async fn sync_with_apply(config_path: &Path, apply_memories: bool) -> Result<(), String> {
    let _one_at_a_time = SYNC_LOCK.lock().await;
    let config = server_cli::load(config_path)?;
    if config.role != NodeRole::Replica {
        return Err("only replicas can run ax server sync".into());
    }
    let access_db =
        ax_global_db::open_and_init(&ax_global_db::global_db_path().map_err(|e| e.to_string())?)
            .await
            .map_err(|e| e.to_string())?;
    ax_global_db::access::record_local_node(
        &access_db,
        &config.node_id,
        "replica",
        config.primary_url.as_deref(),
    )
    .await
    .map_err(|e| e.to_string())?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;
    let mut errors = Vec::new();
    for project in &config.projects {
        let mut warning = None;
        let result: Result<(String, String), String> = match fetch_snapshot(
            &client, &config, project,
        )
        .await
        {
            Ok((snapshot, primary_conflicts, peer_note)) => {
                warning = peer_note;
                match stage(&config, project, &snapshot) {
                    Ok(status) if apply_memories => {
                        match async { crate::server_data::apply(&config,&snapshot).await?; crate::server_apply::apply(&config,&snapshot).await }.await {
                            Ok(report) => Ok((
                                format!("full_data_synced; two_way_preserved; {status}; memories: +{} updated {} retained {} unchanged {}; {}",
                                    report.inserted,report.updated,report.retained,report.unchanged,
                                    if report.conflicts+primary_conflicts>0 { format!("review_required: {} conflicts preserved",report.conflicts+primary_conflicts) } else { "no conflicts".into() }),
                                snapshot.revision,
                            )),
                            Err(error) => Err(error),
                        }
                    }
                    Ok(status) => Ok((
                        format!(
                            "staged_only; two_way_preserved; {status}; {}",
                            if primary_conflicts > 0 {
                                format!("review_required: {primary_conflicts} conflicts preserved")
                            } else {
                                "no conflicts".into()
                            }
                        ),
                        snapshot.revision,
                    )),
                    Err(error) => Err(error),
                }
            }
            Err(error) => Err(error),
        };
        let recorded = record_attempt(&config, project, &result, warning.as_deref());
        match (result, recorded) {
            (Ok((status, _)), Ok(())) => println!("Project {}: {}", project.id, status),
            (Err(error), _) | (_, Err(error)) => {
                eprintln!("Project {}: sync failed: {}", project.id, error);
                errors.push(project.id.clone());
            }
        }
    }
    ax_global_db::access::record_local_sync(&access_db, errors.is_empty())
        .await
        .map_err(|e| e.to_string())?;
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "synchronization failed for {} project(s)",
            errors.len()
        ))
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
        let project = ProjectRegistration {
            id: "sample".into(),
            path: project_path.clone(),
            token_env: "AX_SERVER_SAMPLE_TOKEN".into(),
        };
        let config = ServerConfig {
            schema_version: 1,
            cluster_id: "cluster".into(),
            node_id: "replica".into(),
            role: NodeRole::Replica,
            primary_url: Some("https://example.com".into()),
            projects: vec![project.clone()],
        };
        let snapshot = Snapshot {
            schema_version: 1,
            cluster_id: "cluster".into(),
            primary_node_id: "primary".into(),
            project_id: "sample".into(),
            revision: "invalid".into(),
            files: BTreeMap::new(),
        };
        assert!(stage(&config, &project, &snapshot).is_err());
        assert!(!project_path.join(".ax/server").exists());
    }

    fn staging_fixture() -> (tempfile::TempDir, ServerConfig, ProjectRegistration) {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join(".ax")).unwrap();
        let project = ProjectRegistration {
            id: "project".into(),
            path: tmp.path().into(),
            token_env: "AX_TEST_UNUSED_STAGE_TOKEN".into(),
        };
        let config = ServerConfig {
            schema_version: 1,
            cluster_id: "cluster".into(),
            node_id: "replica".into(),
            role: NodeRole::Replica,
            primary_url: Some("https://example.test".into()),
            projects: vec![project.clone()],
        };
        (tmp, config, project)
    }

    fn snapshot_with_rules(rules: &str) -> Snapshot {
        let mut item = Snapshot {
            schema_version: 2,
            cluster_id: "cluster".into(),
            primary_node_id: "primary".into(),
            project_id: "project".into(),
            revision: String::new(),
            files: BTreeMap::from([
                ("state/memories.json".to_string(), "[]".to_string()),
                ("state/rules.json".to_string(), rules.to_string()),
                ("state/skills.json".to_string(), "[]".to_string()),
            ]),
        };
        item.revision = server_snapshot::revision(&item).unwrap();
        server_snapshot::validate(&item, "cluster", "project").unwrap();
        item
    }

    #[test]
    fn concurrent_stages_of_one_revision_all_succeed() {
        for _ in 0..20 {
            let (tmp, config, project) = staging_fixture();
            let snapshot = snapshot_with_rules("[]");
            let barrier = std::sync::Barrier::new(8);
            let results: Vec<Result<String, String>> = std::thread::scope(|scope| {
                let handles: Vec<_> = (0..8)
                    .map(|_| {
                        scope.spawn(|| {
                            barrier.wait();
                            stage(&config, &project, &snapshot)
                        })
                    })
                    .collect();
                handles.into_iter().map(|h| h.join().unwrap()).collect()
            });
            for result in results {
                assert!(result.is_ok(), "{result:?}");
            }
            drop(tmp);
        }
    }

    #[tokio::test]
    async fn replica_synchronizations_in_one_process_run_one_at_a_time() {
        let guard = SYNC_LOCK.lock().await;
        let missing = std::path::PathBuf::from("/nonexistent/ax-server.json");
        let mut waiting = tokio::spawn(async move { sync_with_apply(&missing, false).await });
        let early = tokio::time::timeout(Duration::from_millis(200), &mut waiting).await;
        assert!(
            early.is_err(),
            "a second synchronization must wait for the first"
        );
        drop(guard);
        let finished = tokio::time::timeout(Duration::from_secs(5), waiting)
            .await
            .unwrap()
            .unwrap();
        assert!(finished.is_err());
    }

    #[test]
    fn staging_a_new_revision_removes_older_revisions_and_staging_leftovers() {
        let (tmp, config, project) = staging_fixture();
        let first = snapshot_with_rules("[]");
        let second = snapshot_with_rules("[ ]");
        let third = snapshot_with_rules("[  ]");
        assert_ne!(first.revision, second.revision);
        let root = tmp
            .path()
            .join(".ax/server/replicas/cluster/project/primary");
        stage(&config, &project, &first).unwrap();
        stage(&config, &project, &second).unwrap();
        // A staging folder left by a crashed run long ago.
        let leftover = root.join(".staging-crashed");
        fs::create_dir(&leftover).unwrap();
        let old = std::time::SystemTime::now() - Duration::from_secs(2 * 3600);
        fs::File::open(&leftover)
            .unwrap()
            .set_modified(old)
            .unwrap();
        // A staging folder another process is writing right now stays.
        let active = root.join(".staging-active");
        fs::create_dir(&active).unwrap();
        stage(&config, &project, &third).unwrap();
        let mut names: Vec<String> = fs::read_dir(&root)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        let mut expected = vec![
            ".staging-active".to_string(),
            third.revision.clone(),
            "current.json".to_string(),
        ];
        expected.sort();
        assert_eq!(names, expected);
        // The selected revision still verifies.
        assert_eq!(stage(&config, &project, &third).unwrap(), "unchanged");
    }
}

#[cfg(test)]
mod compatibility_tests {
    use super::*;
    #[test]
    fn peer_protocol_and_version_are_reported_before_exchange() {
        let mut headers = reqwest::header::HeaderMap::new();
        assert!(peer_warning(&headers).unwrap().unwrap().contains("unknown"));
        headers.insert("x-ax-version", env!("CARGO_PKG_VERSION").parse().unwrap());
        headers.insert("x-ax-replication-protocol", "3".parse().unwrap());
        assert_eq!(peer_warning(&headers).unwrap(), None);
        headers.insert("x-ax-version", "8.0.0".parse().unwrap());
        assert!(peer_warning(&headers).unwrap().unwrap().contains("8.0.0"));
        headers.insert("x-ax-replication-protocol", "4".parse().unwrap());
        assert!(peer_warning(&headers).unwrap_err().contains("protocol 4"));
        for protocol in 0..16 {
            headers.insert(
                "x-ax-replication-protocol",
                protocol.to_string().parse().unwrap(),
            );
            assert_eq!(peer_warning(&headers).is_ok(), protocol == 3);
        }
        headers.insert("x-ax-replication-protocol", "3".parse().unwrap());
        headers.insert("x-ax-version", "invalid<script>".parse().unwrap());
        let warning = peer_warning(&headers).unwrap().unwrap();
        assert!(warning.contains("unknown"));
        assert!(!warning.contains("<script>"));
    }
    #[test]
    fn compatibility_warning_persists_and_clears_on_matching_success() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join(".ax")).unwrap();
        let project = ProjectRegistration {
            id: "project".into(),
            path: tmp.path().into(),
            token_env: "AX_TEST_PEER_TOKEN".into(),
        };
        let config = ServerConfig {
            schema_version: 1,
            cluster_id: "cluster".into(),
            node_id: "replica".into(),
            role: NodeRole::Replica,
            primary_url: Some("https://example.test".into()),
            projects: vec![project.clone()],
        };
        let outcome = Ok(("full_data_synced".into(), "revision-one".into()));
        record_attempt(
            &config,
            &project,
            &outcome,
            Some("Peer Ax 8.0.0; local Ax 8.1.0"),
        )
        .unwrap();
        assert!(status(&config, &project)["compatibility_warning"]
            .as_str()
            .unwrap()
            .contains("8.0.0"));
        record_attempt(
            &config,
            &project,
            &Err("incompatible protocol 4".into()),
            None,
        )
        .unwrap();
        assert_eq!(
            status(&config, &project)["last_success_revision"],
            "revision-one"
        );
        assert_eq!(
            status(&config, &project)["last_error"],
            "incompatible protocol 4"
        );
        record_attempt(&config, &project, &outcome, None).unwrap();
        assert!(status(&config, &project)["compatibility_warning"].is_null());
    }
}
