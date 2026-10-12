//! Validated server configuration -> public local export catalogue.
use super::config::RemoteConfig;
use ax_global_db::remote_mcp_publications::{
    self as catalog, ClientRegistrations, ProjectBinding, PublisherSnapshot,
};
use std::path::Path;

pub fn snapshot(
    config: &RemoteConfig,
    label: Option<&str>,
    clients: Option<ClientRegistrations>,
    active_root: Option<&Path>,
) -> Result<PublisherSnapshot, String> {
    let mut config = config.clone();
    config.validate()?;
    if let Some(clients) = &clients {
        clients.validate().map_err(|e| e.to_string())?;
    }
    let preserve_label = label.is_none();
    let label = label.unwrap_or(&config.public_url).trim().to_owned();
    if label.is_empty() || label.len() > 160 || label.chars().any(char::is_control) {
        return Err("Publisher label must contain 1–160 printable characters".into());
    }
    let active = active_root
        .map(Path::canonicalize)
        .transpose()
        .map_err(|_| "Active project is unavailable")?;
    let projects = config
        .projects
        .iter()
        .filter(|p| active.as_ref().is_none_or(|root| *root == p.root))
        .map(|p| {
            let mut endpoint = super::config::https_url(&config.public_url)?;
            endpoint
                .path_segments_mut()
                .map_err(|_| "Invalid publisher URL")?
                .extend(["projects", &p.id, "mcp"]);
            Ok(ProjectBinding {
                remote_project_id: p.id.clone(),
                canonical_project_root: p
                    .root
                    .to_str()
                    .ok_or("Project root is not UTF-8")?
                    .to_owned(),
                project_label: p
                    .root
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&p.id)
                    .to_owned(),
                endpoint_url: endpoint.into(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    if projects.is_empty() {
        return Err("The active project is not registered in this server configuration".into());
    }
    let id =
        catalog::fingerprint(format!("{}\0{}", config.public_url, config.oauth.issuer).as_bytes());
    let config_fingerprint =
        catalog::fingerprint(&serde_json::to_vec(&config).map_err(|e| e.to_string())?);
    Ok(PublisherSnapshot {
        id,
        public_origin: config.public_url,
        oauth_issuer: config.oauth.issuer,
        config_fingerprint,
        label,
        preserve_label,
        clients,
        projects,
    })
}

pub async fn register(
    config: &RemoteConfig,
    label: Option<&str>,
    clients: Option<ClientRegistrations>,
) -> Result<PublisherSnapshot, String> {
    let snapshot = snapshot(config, label, clients, None)?;
    let path = ax_global_db::global_db_path().map_err(|e| e.to_string())?;
    let pool = ax_global_db::open_and_init(&path)
        .await
        .map_err(|e| e.to_string())?;
    catalog::register(&pool, &snapshot, true)
        .await
        .map_err(|e| e.to_string())?;
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_snapshot_keeps_only_active_project_and_never_persists_grants() {
        let temp = tempfile::tempdir().unwrap();
        let a = temp.path().join("a/demo");
        let b = temp.path().join("b/demo");
        for root in [&a, &b] {
            std::fs::create_dir_all(root.join(".ax")).unwrap();
            std::fs::write(root.join(".ax/ax.db"), "").unwrap();
        }
        let config: RemoteConfig = serde_json::from_value(serde_json::json!({"public_url":"https://ax.example","oauth":{"issuer":"https://issuer.example/","jwks_uri":"https://issuer.example/jwks"},"projects":[{"id":"remote-a","root":a,"subjects":["private-subject"]},{"id":"remote-b","root":b,"subjects":["private-subject"]}]})).unwrap();
        let result = snapshot(&config, None, None, Some(&a)).unwrap();
        assert_eq!(result.projects.len(), 1);
        assert_eq!(result.projects[0].remote_project_id, "remote-a");
        assert_eq!(
            result.projects[0].endpoint_url,
            "https://ax.example/projects/remote-a/mcp"
        );
        assert!(snapshot(&config, None, None, Some(temp.path())).is_err());
        assert_eq!(config.projects[0].subjects, ["private-subject"]);
    }
}
