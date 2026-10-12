//! Local public export metadata, separate from credentials and project grants.
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use std::path::Path;

pub const LEASE_STALE_SECONDS: i64 = 45;

pub fn fingerprint(value: &[u8]) -> String {
    blake3::hash(value).to_hex().to_string()
}
pub fn workspace_id(root: &Path) -> Result<String> {
    let root = root.canonicalize()?;
    Ok(fingerprint(root.as_os_str().as_encoded_bytes()))
}
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Only public, pre-registered client IDs. These are not credentials.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientRegistrations {
    pub ax_proxy: Option<String>,
    pub cursor: Option<String>,
    pub vscode: Option<String>,
}
impl ClientRegistrations {
    pub fn validate(&self) -> Result<()> {
        for id in [&self.ax_proxy, &self.cursor, &self.vscode]
            .into_iter()
            .flatten()
        {
            if id.is_empty() || id.len() > 512 || id.chars().any(char::is_control) {
                bail!("Public client IDs must contain 1–512 printable characters");
            }
        }
        Ok(())
    }
}
#[derive(Clone)]
pub struct ProjectBinding {
    pub remote_project_id: String,
    pub canonical_project_root: String,
    pub project_label: String,
    pub endpoint_url: String,
}
#[derive(Clone)]
pub struct PublisherSnapshot {
    pub id: String,
    pub public_origin: String,
    pub oauth_issuer: String,
    pub config_fingerprint: String,
    pub label: String,
    pub preserve_label: bool,
    pub clients: Option<ClientRegistrations>,
    pub projects: Vec<ProjectBinding>,
}
#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Publication {
    pub id: String,
    pub publisher_id: String,
    pub public_origin: String,
    pub oauth_issuer: String,
    pub resource: String,
    pub publisher_label: String,
    pub config_fingerprint: String,
    pub remote_project_id: String,
    pub project_label: String,
    pub endpoint_url: String,
    #[serde(skip)]
    pub canonical_project_root: String,
    #[serde(skip)]
    pub client_registrations: String,
    pub runtime_status: String,
    pub last_heartbeat_at: Option<i64>,
}
impl Publication {
    pub fn clients(&self) -> Result<ClientRegistrations> {
        Ok(serde_json::from_str(&self.client_registrations)?)
    }
}

/// Reconcile a CLI snapshot; browser imports register only the active binding.
/// An early write serializes SQLite transactions before checking conflicts.
pub async fn register(
    pool: &SqlitePool,
    snapshot: &PublisherSnapshot,
    reconcile: bool,
) -> Result<()> {
    if let Some(clients) = &snapshot.clients {
        clients.validate()?;
    }
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE remote_mcp_publishers SET updated_at = updated_at WHERE id = ?")
        .bind(&snapshot.id)
        .execute(&mut *tx)
        .await?;
    let live: Option<(String,)> = sqlx::query_as("SELECT config_fingerprint FROM remote_mcp_runtime_leases WHERE publisher_id = ? AND stopped_at IS NULL AND last_heartbeat_at > ?")
        .bind(&snapshot.id).bind(now() - LEASE_STALE_SECONDS).fetch_optional(&mut *tx).await?;
    if live.is_some_and(|(fp,)| fp != snapshot.config_fingerprint) {
        bail!("This publisher is running a different configuration. Stop it before replacing its registration.");
    }
    for project in &snapshot.projects {
        let existing: Option<(String,)> = sqlx::query_as("SELECT canonical_project_root FROM remote_mcp_publications WHERE publisher_id = ? AND remote_project_id = ?")
            .bind(&snapshot.id).bind(&project.remote_project_id).fetch_optional(&mut *tx).await?;
        if existing.is_some_and(|(root,)| root != project.canonical_project_root) {
            bail!("Project ID is already bound to another local root. Remove that local registration explicitly first.");
        }
    }
    let clients = snapshot
        .clients
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    let timestamp = now();
    sqlx::query("INSERT INTO remote_mcp_publishers (id, public_origin, oauth_issuer, resource, label, config_fingerprint, client_registrations, configured_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, COALESCE(?, '{}'), ?, ?) ON CONFLICT(id) DO UPDATE SET label=CASE WHEN ? THEN remote_mcp_publishers.label ELSE excluded.label END, config_fingerprint=excluded.config_fingerprint, client_registrations=COALESCE(?, remote_mcp_publishers.client_registrations), updated_at=excluded.updated_at")
        .bind(&snapshot.id).bind(&snapshot.public_origin).bind(&snapshot.oauth_issuer).bind(&snapshot.public_origin)
        .bind(&snapshot.label).bind(&snapshot.config_fingerprint).bind(&clients).bind(timestamp).bind(timestamp).bind(snapshot.preserve_label).bind(&clients)
        .execute(&mut *tx).await?;
    if reconcile {
        sqlx::query("DELETE FROM remote_mcp_publications WHERE publisher_id = ?")
            .bind(&snapshot.id)
            .execute(&mut *tx)
            .await?;
    }
    for project in &snapshot.projects {
        let id = fingerprint(format!("{}\0{}", snapshot.id, project.remote_project_id).as_bytes());
        sqlx::query("INSERT INTO remote_mcp_publications (id, publisher_id, remote_project_id, canonical_project_root, project_label, endpoint_url) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET project_label=excluded.project_label, endpoint_url=excluded.endpoint_url")
            .bind(id).bind(&snapshot.id).bind(&project.remote_project_id).bind(&project.canonical_project_root)
            .bind(&project.project_label).bind(&project.endpoint_url).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn list_for_root(pool: &SqlitePool, root: &Path) -> Result<Vec<Publication>> {
    let canonical = root.canonicalize()?;
    let root = canonical
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Project root is not UTF-8"))?;
    Ok(sqlx::query_as::<_, Publication>("SELECT p.id, p.publisher_id, p.remote_project_id, p.canonical_project_root, p.project_label, p.endpoint_url, s.public_origin, s.oauth_issuer, s.resource, s.label AS publisher_label, s.config_fingerprint, s.client_registrations, l.last_heartbeat_at, CASE WHEN l.instance_id IS NULL OR l.stopped_at IS NOT NULL THEN 'stopped' WHEN l.last_heartbeat_at > ? THEN 'running' ELSE 'unknown' END AS runtime_status FROM remote_mcp_publications p JOIN remote_mcp_publishers s ON s.id = p.publisher_id LEFT JOIN remote_mcp_runtime_leases l ON l.publisher_id = s.id WHERE p.canonical_project_root = ? ORDER BY s.label, p.remote_project_id")
        .bind(now() - LEASE_STALE_SECONDS).bind(root).fetch_all(pool).await?)
}
pub async fn remove(pool: &SqlitePool, id: &str, root: &Path) -> Result<bool> {
    let canonical = root.canonicalize()?;
    let result = sqlx::query(
        "DELETE FROM remote_mcp_publications WHERE id = ? AND canonical_project_root = ?",
    )
    .bind(id)
    .bind(
        canonical
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid root"))?,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn start_lease(pool: &SqlitePool, snapshot: &PublisherSnapshot) -> Result<String> {
    let instance = uuid::Uuid::new_v4().to_string();
    let t = now();
    let result = sqlx::query("INSERT INTO remote_mcp_runtime_leases (publisher_id, instance_id, config_fingerprint, started_at, last_heartbeat_at, stopped_at) SELECT ?, ?, ?, ?, ?, NULL WHERE EXISTS (SELECT 1 FROM remote_mcp_publishers WHERE id = ? AND config_fingerprint = ?) ON CONFLICT(publisher_id) DO UPDATE SET instance_id=excluded.instance_id, config_fingerprint=excluded.config_fingerprint, started_at=excluded.started_at, last_heartbeat_at=excluded.last_heartbeat_at, stopped_at=NULL WHERE remote_mcp_runtime_leases.stopped_at IS NOT NULL OR remote_mcp_runtime_leases.last_heartbeat_at <= ?")
        .bind(&snapshot.id).bind(&instance).bind(&snapshot.config_fingerprint).bind(t).bind(t).bind(&snapshot.id).bind(&snapshot.config_fingerprint).bind(t - LEASE_STALE_SECONDS).execute(pool).await?;
    if result.rows_affected() != 1 {
        bail!("Publisher is already running or its configuration changed during startup");
    }
    Ok(instance)
}
pub async fn heartbeat(pool: &SqlitePool, publisher: &str, instance: &str) -> Result<()> {
    sqlx::query("UPDATE remote_mcp_runtime_leases SET last_heartbeat_at = ? WHERE publisher_id = ? AND instance_id = ? AND stopped_at IS NULL")
        .bind(now()).bind(publisher).bind(instance).execute(pool).await?;
    Ok(())
}
pub async fn stop_lease(pool: &SqlitePool, publisher: &str, instance: &str) -> Result<()> {
    sqlx::query("UPDATE remote_mcp_runtime_leases SET stopped_at = ? WHERE publisher_id = ? AND instance_id = ?")
        .bind(now()).bind(publisher).bind(instance).execute(pool).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot(root: &Path, host: &str) -> PublisherSnapshot {
        PublisherSnapshot {
            id: fingerprint(host.as_bytes()),
            public_origin: format!("https://{host}"),
            oauth_issuer: "https://issuer.example/".into(),
            config_fingerprint: "config-one".into(),
            label: host.into(),
            preserve_label: false,
            clients: Some(ClientRegistrations {
                ax_proxy: Some("native-public".into()),
                ..Default::default()
            }),
            projects: vec![ProjectBinding {
                remote_project_id: "demo".into(),
                canonical_project_root: root.canonicalize().unwrap().to_str().unwrap().into(),
                project_label: "demo".into(),
                endpoint_url: format!("https://{host}/projects/demo/mcp"),
            }],
        }
    }
    async fn fixture() -> (
        tempfile::TempDir,
        SqlitePool,
        std::path::PathBuf,
        std::path::PathBuf,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let a = temp.path().join("a/demo");
        let b = temp.path().join("b/demo");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let pool = crate::open_and_init(&temp.path().join("global.db"))
            .await
            .unwrap();
        (temp, pool, a, b)
    }
    #[tokio::test]
    async fn roots_and_publishers_are_distinct_and_public_response_hides_local_data() {
        let (_temp, pool, a, b) = fixture().await;
        register(&pool, &snapshot(&a, "one.example"), true)
            .await
            .unwrap();
        register(&pool, &snapshot(&a, "two.example"), true)
            .await
            .unwrap();
        assert!(list_for_root(&pool, &b).await.unwrap().is_empty());
        let list = list_for_root(&pool, &a).await.unwrap();
        assert_eq!(list.len(), 2);
        assert_ne!(list[0].id, list[1].id);
        let json = serde_json::to_string(&list).unwrap();
        assert!(!json.contains(a.to_str().unwrap()));
        assert!(!json.contains("native-public"));
        assert!(!remove(&pool, &list[0].id, &b).await.unwrap());
        assert_eq!(list_for_root(&pool, &a).await.unwrap().len(), 2);
    }
    #[tokio::test]
    async fn root_rebinding_is_rejected_atomically_until_explicit_removal() {
        let (_temp, pool, a, b) = fixture().await;
        let one = snapshot(&a, "one.example");
        register(&pool, &one, true).await.unwrap();
        let changed = snapshot(&b, "one.example");
        assert!(register(&pool, &changed, true).await.is_err());
        assert_eq!(list_for_root(&pool, &a).await.unwrap().len(), 1);
        let id = list_for_root(&pool, &a).await.unwrap()[0].id.clone();
        remove(&pool, &id, &a).await.unwrap();
        register(&pool, &changed, true).await.unwrap();
        assert!(list_for_root(&pool, &a).await.unwrap().is_empty());
        assert_eq!(list_for_root(&pool, &b).await.unwrap().len(), 1);
    }
    #[tokio::test]
    async fn reconciliation_removes_missing_bindings_but_import_only_adds_active_binding() {
        let (_temp, pool, a, b) = fixture().await;
        let mut one = snapshot(&a, "one.example");
        let mut second = snapshot(&b, "one.example").projects.remove(0);
        second.remote_project_id = "second".into();
        one.projects.push(second);
        register(&pool, &one, true).await.unwrap();
        one.projects.pop();
        register(&pool, &one, false).await.unwrap();
        assert_eq!(list_for_root(&pool, &b).await.unwrap().len(), 1);
        register(&pool, &one, true).await.unwrap();
        assert!(list_for_root(&pool, &b).await.unwrap().is_empty());
    }
    #[tokio::test]
    async fn live_snapshot_conflicts_and_old_lease_cleanup_cannot_stop_new_instance() {
        let (_temp, pool, a, _b) = fixture().await;
        let mut one = snapshot(&a, "one.example");
        register(&pool, &one, true).await.unwrap();
        let first = start_lease(&pool, &one).await.unwrap();
        assert_eq!(
            list_for_root(&pool, &a).await.unwrap()[0].runtime_status,
            "running"
        );
        assert!(start_lease(&pool, &one).await.is_err());
        one.config_fingerprint = "new-config".into();
        assert!(register(&pool, &one, true).await.is_err());
        sqlx::query("UPDATE remote_mcp_runtime_leases SET last_heartbeat_at = ?")
            .bind(now() - 46)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            list_for_root(&pool, &a).await.unwrap()[0].runtime_status,
            "unknown"
        );
        register(&pool, &one, true).await.unwrap();
        let second = start_lease(&pool, &one).await.unwrap();
        stop_lease(&pool, &one.id, &first).await.unwrap();
        heartbeat(&pool, &one.id, &first).await.unwrap();
        assert_eq!(
            list_for_root(&pool, &a).await.unwrap()[0].runtime_status,
            "running"
        );
        stop_lease(&pool, &one.id, &second).await.unwrap();
        assert_eq!(
            list_for_root(&pool, &a).await.unwrap()[0].runtime_status,
            "stopped"
        );
        assert_eq!(
            list_for_root(&pool, &a).await.unwrap()[0]
                .clients()
                .unwrap()
                .ax_proxy
                .as_deref(),
            Some("native-public")
        );
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn symlink_alias_matches_the_same_canonical_project() {
        let (temp, pool, a, _b) = fixture().await;
        let alias = temp.path().join("alias");
        std::os::unix::fs::symlink(&a, &alias).unwrap();
        register(&pool, &snapshot(&a, "one.example"), true)
            .await
            .unwrap();
        assert_eq!(list_for_root(&pool, &alias).await.unwrap().len(), 1);
        assert_eq!(workspace_id(&a).unwrap(), workspace_id(&alias).unwrap());
    }
    #[tokio::test]
    async fn server_autoregistration_preserves_explicit_label_and_client_ids() {
        let (_temp, pool, a, _b) = fixture().await;
        let mut one = snapshot(&a, "one.example");
        one.label = "Team server".into();
        register(&pool, &one, true).await.unwrap();
        one.label = "default label".into();
        one.preserve_label = true;
        one.clients = None;
        register(&pool, &one, true).await.unwrap();
        let p = list_for_root(&pool, &a).await.unwrap().remove(0);
        assert_eq!(p.publisher_label, "Team server");
        assert_eq!(
            p.clients().unwrap().ax_proxy.as_deref(),
            Some("native-public")
        );
    }
}
