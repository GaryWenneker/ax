//! Local lifecycle operations for standalone Ax configuration.
//!
//! Listening, snapshot serving and replication will be added separately;
//! these commands do not pretend that a configured node is running.

use crate::server_config::{NodeRole, ProjectRegistration, ServerConfig};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub fn init(
    config_path: &Path,
    role: NodeRole,
    cluster_id: String,
    project_id: String,
    project_path: PathBuf,
    primary_url: Option<String>,
) -> Result<(), String> {
    let project_path = project_path
        .canonicalize()
        .map_err(|e| format!("cannot resolve project path: {e}"))?;
    // Authentication token contents never enter this file.
    let token_env = format!(
        "AX_SERVER_{}_TOKEN",
        project_id.to_ascii_uppercase().replace('-', "_")
    );
    let config = ServerConfig {
        schema_version: 1,
        cluster_id,
        node_id: uuid::Uuid::new_v4().to_string(),
        role,
        primary_url,
        projects: vec![ProjectRegistration {
            id: project_id,
            path: project_path,
            token_env: token_env.clone(),
        }],
    };
    config.validate()?;
    let parent = config_path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    if !parent.is_dir() {
        return Err(format!("configuration directory does not exist: {}", parent.display()));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(config_path)
        .map_err(|e| format!("cannot create configuration without overwriting: {e}"))?;
    let written = (|| -> Result<(), String> {
        serde_json::to_writer_pretty(&mut file, &config)
            .map_err(|e| format!("cannot serialize configuration: {e}"))?;
        file.write_all(b"\n")
            .map_err(|e| format!("cannot finish configuration: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("cannot persist configuration: {e}"))
    })();
    if let Err(err) = written {
        drop(file);
        let _ = fs::remove_file(config_path);
        return Err(err);
    }
    println!(
        "Created {} ({} node {}). Set {} to a random token of at least 32 characters before starting the server.",
        config_path.display(),
        match role { NodeRole::Primary => "primary", NodeRole::Replica => "replica" },
        config.node_id,
        token_env
    );
    Ok(())
}

pub fn load(path: &Path) -> Result<ServerConfig, String> {
    let source = fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let config: ServerConfig =
        serde_json::from_str(&source).map_err(|e| format!("invalid configuration JSON: {e}"))?;
    config.validate()?;
    Ok(config)
}

pub fn status(config_path: &Path) -> Result<(), String> {
    let config = load(config_path)?;
    let projects: Vec<_> = config
        .projects
        .iter()
        .map(|project| {
            serde_json::json!({
                "id": project.id,
                "path": project.path,
                "token_env": project.token_env,
                "replication": crate::server_replica::status(&config, project),
            })
        })
        .collect();
    let summary = serde_json::json!({
        "schema_version": config.schema_version,
        "cluster_id": config.cluster_id,
        "node_id": config.node_id,
        "role": config.role,
        "primary_url": config.primary_url,
        "projects": projects,
        "server_state": "not_started",
    });
    println!("{}", serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_creates_valid_config_without_overwriting() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("work");
        fs::create_dir_all(project.join(".ax")).unwrap();
        let target = tmp.path().join("server.json");
        let create = || init(
            &target,
            NodeRole::Primary,
            "my-cluster".into(),
            "my-project".into(),
            project.clone(),
            None,
        );
        create().unwrap();
        let original = fs::read_to_string(&target).unwrap();
        assert!(create().is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), original);
        assert_eq!(load(&target).unwrap().projects.len(), 1);
        assert!(!original.contains("super-secret-token"));
    }

    #[test]
    fn init_requires_initialized_project() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("server.json");
        assert!(init(
            &target,
            NodeRole::Primary,
            "cluster".into(),
            "project".into(),
            tmp.path().to_path_buf(),
            None,
        )
        .is_err());
        assert!(!target.exists());
    }
}
