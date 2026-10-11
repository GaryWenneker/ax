use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemberRole {
    Owner,
    Admin,
    Member,
    Readonly,
}

pub const VALID_SCOPES: &[&str] = &[
    "memory:read",
    "memory:write",
    "preflight:read",
    "context:read",
    "share:read",
    "share:write",
    "admin:clients",
    "admin:team",
    "admin:nodes",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClientPrincipal {
    pub member_id: String,
    pub client_id: String,
    pub role: MemberRole,
    pub scopes: Vec<String>,
}

impl ClientPrincipal {
    pub fn allows(&self, scope: &str) -> bool {
        self.scopes.iter().any(|candidate| candidate == scope)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct IssuedClientCredential {
    pub credential_id: String,
    pub token: String,
    pub token_prefix: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IssuedNodeCredential {
    pub credential_id: String,
    pub token: String,
    pub token_prefix: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NodePrincipal {
    pub node_id: String,
}

impl MemberRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Member => "member",
            Self::Readonly => "readonly",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "owner" => Ok(Self::Owner),
            "admin" => Ok(Self::Admin),
            "member" => Ok(Self::Member),
            "readonly" => Ok(Self::Readonly),
            other => bail!("unknown team member role: {other}"),
        }
    }

    pub fn default_scopes(self) -> &'static [&'static str] {
        match self {
            Self::Owner => VALID_SCOPES,
            Self::Admin => &[
                "memory:read",
                "memory:write",
                "preflight:read",
                "context:read",
                "share:read",
                "share:write",
                "admin:clients",
                "admin:team",
                "admin:nodes",
            ],
            Self::Member => &[
                "memory:read",
                "memory:write",
                "preflight:read",
                "context:read",
                "share:read",
                "share:write",
            ],
            Self::Readonly => &[
                "memory:read",
                "preflight:read",
                "context:read",
                "share:read",
            ],
        }
    }

    pub fn permits(self, scope: &str) -> bool {
        self.default_scopes().contains(&scope)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TeamMember {
    pub id: String,
    pub display_name: String,
    pub email: Option<String>,
    pub role: MemberRole,
    pub status: String,
    pub created_at: String,
    pub last_active_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Client {
    pub id: String,
    pub member_id: String,
    pub display_name: String,
    pub client_type: String,
    pub scopes: Vec<String>,
    pub status: String,
    pub created_at: String,
    pub last_seen_at: Option<String>,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReplicationNode {
    pub id: String,
    pub display_name: String,
    pub role: String,
    pub endpoint: Option<String>,
    pub trust_state: String,
    pub credential_id: Option<String>,
    pub discovered_at: String,
    pub approved_at: Option<String>,
    pub revoked_at: Option<String>,
    pub last_seen_at: Option<String>,
    pub last_sync_at: Option<String>,
}

pub async fn bootstrap_owner(
    pool: &SqlitePool,
    id: &str,
    display_name: &str,
    email: Option<&str>,
) -> Result<bool> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM team_members")
        .fetch_one(pool)
        .await?;
    if count.0 != 0 {
        return Ok(false);
    }
    sqlx::query(
        "INSERT INTO team_members (id, display_name, email, role, status)
         VALUES (?, ?, ?, 'owner', 'active')",
    )
    .bind(id)
    .bind(display_name)
    .bind(email)
    .execute(pool)
    .await?;
    Ok(true)
}

pub async fn create_member(
    pool: &SqlitePool,
    id: &str,
    display_name: &str,
    email: Option<&str>,
    role: MemberRole,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO team_members (id, display_name, email, role, status)
         VALUES (?, ?, ?, ?, 'active')",
    )
    .bind(id)
    .bind(display_name)
    .bind(email)
    .bind(role.as_str())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_members(pool: &SqlitePool) -> Result<Vec<TeamMember>> {
    let rows: Vec<(String, String, Option<String>, String, String, String, Option<String>)> =
        sqlx::query_as(
            "SELECT id, display_name, email, role, status, created_at, last_active_at
             FROM team_members ORDER BY created_at, id",
        )
        .fetch_all(pool)
        .await?;
    rows.into_iter()
        .map(|(id, display_name, email, role, status, created_at, last_active_at)| {
            Ok(TeamMember {
                id,
                display_name,
                email,
                role: MemberRole::parse(&role)?,
                status,
                created_at,
                last_active_at,
            })
        })
        .collect()
}

pub async fn create_client(
    pool: &SqlitePool,
    id: &str,
    member_id: &str,
    display_name: &str,
    client_type: &str,
    scopes: &[String],
) -> Result<()> {
    let (role, status): (String, String) = sqlx::query_as(
        "SELECT role, status FROM team_members WHERE id = ?",
    )
    .bind(member_id)
    .fetch_one(pool)
    .await?;
    if status != "active" {
        bail!("team member is not active");
    }
    let role = MemberRole::parse(&role)?;
    validate_client_scopes(role, scopes)?;
    let scopes = serde_json::to_string(scopes)?;
    sqlx::query(
        "INSERT INTO clients (id, member_id, display_name, client_type, scopes, status)
         VALUES (?, ?, ?, ?, ?, 'active')",
    )
    .bind(id)
    .bind(member_id)
    .bind(display_name)
    .bind(client_type)
    .bind(scopes)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_clients(pool: &SqlitePool) -> Result<Vec<Client>> {
    let rows: Vec<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT id, member_id, display_name, client_type, scopes, status,
                created_at, last_seen_at, revoked_at
         FROM clients ORDER BY created_at, id",
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(
            |(
                id,
                member_id,
                display_name,
                client_type,
                scopes,
                status,
                created_at,
                last_seen_at,
                revoked_at,
            )| {
                Ok(Client {
                    id,
                    member_id,
                    display_name,
                    client_type,
                    scopes: serde_json::from_str(&scopes)?,
                    status,
                    created_at,
                    last_seen_at,
                    revoked_at,
                })
            },
        )
        .collect()
}


pub fn validate_client_scopes(role: MemberRole, scopes: &[String]) -> Result<()> {
    for scope in scopes {
        if !VALID_SCOPES.contains(&scope.as_str()) {
            bail!("unknown client scope: {scope}");
        }
        if !role.permits(scope) {
            bail!("scope {scope} exceeds the member role");
        }
    }
    Ok(())
}

fn token_verifier(token: &str) -> String {
    blake3::hash(token.as_bytes()).to_hex().to_string()
}

pub async fn issue_client_credential(
    pool: &SqlitePool,
    client_id: &str,
    expires_at: Option<&str>,
) -> Result<IssuedClientCredential> {
    let status: Option<(String,)> =
        sqlx::query_as("SELECT status FROM clients WHERE id = ?")
            .bind(client_id)
            .fetch_optional(pool)
            .await?;
    match status {
        Some((status,)) if status == "active" => {}
        Some(_) => bail!("client is revoked"),
        None => bail!("client does not exist"),
    }

    let credential_id = Uuid::new_v4().to_string();
    let secret = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let token = format!("axc_{secret}");
    let token_prefix = token.chars().take(12).collect::<String>();
    let verifier = token_verifier(&token);
    add_client_credential(
        pool,
        &credential_id,
        client_id,
        &token_prefix,
        &verifier,
        expires_at,
    )
    .await?;

    Ok(IssuedClientCredential {
        credential_id,
        token,
        token_prefix,
    })
}

pub async fn authenticate_client_token(
    pool: &SqlitePool,
    token: &str,
) -> Result<Option<ClientPrincipal>> {
    if !token.starts_with("axc_") || token.len() < 40 {
        return Ok(None);
    }
    let prefix = token.chars().take(12).collect::<String>();
    let verifier = token_verifier(token);
    let row: Option<(String, String, String, String, String, String, Option<String>)> =
        sqlx::query_as(
            "SELECT c.member_id, c.id, tm.role, tm.status, c.status, c.scopes, cc.expires_at
             FROM client_credentials cc
             JOIN clients c ON c.id = cc.client_id
             JOIN team_members tm ON tm.id = c.member_id
             WHERE cc.token_prefix = ?
               AND cc.verifier = ?
               AND cc.revoked_at IS NULL",
        )
        .bind(&prefix)
        .bind(&verifier)
        .fetch_optional(pool)
        .await?;

    let Some((member_id, client_id, role, member_status, client_status, scopes, expires_at)) = row
    else {
        return Ok(None);
    };
    if member_status != "active" || client_status != "active" {
        return Ok(None);
    }
    if let Some(expires_at) = expires_at {
        let valid: (i64,) = sqlx::query_as(
            "SELECT CASE WHEN datetime(?) > CURRENT_TIMESTAMP THEN 1 ELSE 0 END",
        )
        .bind(expires_at)
        .fetch_one(pool)
        .await?;
        if valid.0 != 1 {
            return Ok(None);
        }
    }

    let role = MemberRole::parse(&role)?;
    let requested: Vec<String> = serde_json::from_str(&scopes)?;
    let effective = requested
        .into_iter()
        .filter(|scope| role.permits(scope))
        .collect::<Vec<_>>();

    sqlx::query(
        "UPDATE client_credentials SET last_used_at = CURRENT_TIMESTAMP WHERE token_prefix = ?",
    )
    .bind(&prefix)
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE clients SET last_seen_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(&client_id)
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE team_members SET last_active_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(&member_id)
    .execute(pool)
    .await?;

    Ok(Some(ClientPrincipal {
        member_id,
        client_id,
        role,
        scopes: effective,
    }))
}

pub async fn revoke_client_credential(pool: &SqlitePool, credential_id: &str) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE client_credentials
         SET revoked_at = CURRENT_TIMESTAMP
         WHERE id = ? AND revoked_at IS NULL",
    )
    .bind(credential_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn add_client_credential(
    pool: &SqlitePool,
    id: &str,
    client_id: &str,
    token_prefix: &str,
    verifier: &str,
    expires_at: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO client_credentials
            (id, client_id, token_prefix, verifier, expires_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(client_id)
    .bind(token_prefix)
    .bind(verifier)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn revoke_client(pool: &SqlitePool, client_id: &str) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let result = sqlx::query(
        "UPDATE clients SET status = 'revoked', revoked_at = CURRENT_TIMESTAMP
         WHERE id = ? AND status = 'active'",
    )
    .bind(client_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE client_credentials SET revoked_at = CURRENT_TIMESTAMP
         WHERE client_id = ? AND revoked_at IS NULL",
    )
    .bind(client_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(result.rows_affected() == 1)
}

pub async fn discover_replica(
    pool: &SqlitePool,
    id: &str,
    display_name: &str,
    endpoint: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO replication_nodes
            (id, display_name, role, endpoint, trust_state, last_seen_at)
         VALUES (?, ?, 'replica', ?, 'pending', CURRENT_TIMESTAMP)
         ON CONFLICT(id) DO UPDATE SET
            display_name = excluded.display_name,
            endpoint = excluded.endpoint,
            last_seen_at = CURRENT_TIMESTAMP",
    )
    .bind(id)
    .bind(display_name)
    .bind(endpoint)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn register_trusted_replica(
    pool: &SqlitePool,
    node_id: &str,
    display_name: &str,
    endpoint: Option<&str>,
    project_id: &str,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO replication_nodes
            (id, display_name, role, endpoint, trust_state, approved_at, last_seen_at)
         VALUES (?, ?, 'replica', ?, 'trusted', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
         ON CONFLICT(id) DO UPDATE SET
            display_name = excluded.display_name,
            endpoint = COALESCE(excluded.endpoint, replication_nodes.endpoint),
            trust_state = 'trusted',
            approved_at = COALESCE(replication_nodes.approved_at, CURRENT_TIMESTAMP),
            revoked_at = NULL,
            last_seen_at = CURRENT_TIMESTAMP",
    )
    .bind(node_id)
    .bind(display_name)
    .bind(endpoint)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO node_project_access (node_id, project_id, revoked_at)
         VALUES (?, ?, NULL)
         ON CONFLICT(node_id, project_id) DO UPDATE SET
            revoked_at = NULL,
            granted_at = CURRENT_TIMESTAMP",
    )
    .bind(node_id)
    .bind(project_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

pub async fn approve_node(
    pool: &SqlitePool,
    node_id: &str,
    credential_id: &str,
) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE replication_nodes
         SET trust_state = 'trusted', credential_id = ?, approved_at = CURRENT_TIMESTAMP,
             revoked_at = NULL
         WHERE id = ? AND trust_state = 'pending'",
    )
    .bind(credential_id)
    .bind(node_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}


pub async fn issue_node_credential(
    pool: &SqlitePool,
    node_id: &str,
    expires_at: Option<&str>,
) -> Result<IssuedNodeCredential> {
    let trust: Option<(String,)> =
        sqlx::query_as("SELECT trust_state FROM replication_nodes WHERE id = ?")
            .bind(node_id)
            .fetch_optional(pool)
            .await?;
    match trust {
        Some((state,)) if state == "pending" || state == "trusted" => {}
        Some(_) => bail!("node is revoked"),
        None => bail!("node does not exist"),
    }

    let credential_id = Uuid::new_v4().to_string();
    let secret = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let token = format!("axn_{secret}");
    let token_prefix = token.chars().take(12).collect::<String>();
    let verifier = token_verifier(&token);

    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO node_credentials
            (id, node_id, token_prefix, verifier, expires_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&credential_id)
    .bind(node_id)
    .bind(&token_prefix)
    .bind(&verifier)
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;
    let result = sqlx::query(
        "UPDATE replication_nodes
         SET trust_state = 'trusted', credential_id = ?, approved_at = CURRENT_TIMESTAMP,
             revoked_at = NULL
         WHERE id = ? AND trust_state IN ('pending', 'trusted')",
    )
    .bind(&credential_id)
    .bind(node_id)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() != 1 {
        bail!("node cannot be approved");
    }
    tx.commit().await?;

    Ok(IssuedNodeCredential {
        credential_id,
        token,
        token_prefix,
    })
}

pub async fn grant_node_project(
    pool: &SqlitePool,
    node_id: &str,
    project_id: &str,
) -> Result<()> {
    let trust: Option<(String,)> =
        sqlx::query_as("SELECT trust_state FROM replication_nodes WHERE id = ?")
            .bind(node_id)
            .fetch_optional(pool)
            .await?;
    if trust.is_none() {
        bail!("node does not exist");
    }
    sqlx::query(
        "INSERT INTO node_project_access (node_id, project_id, revoked_at)
         VALUES (?, ?, NULL)
         ON CONFLICT(node_id, project_id) DO UPDATE SET
            revoked_at = NULL,
            granted_at = CURRENT_TIMESTAMP",
    )
    .bind(node_id)
    .bind(project_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn revoke_node_project(
    pool: &SqlitePool,
    node_id: &str,
    project_id: &str,
) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE node_project_access
         SET revoked_at = CURRENT_TIMESTAMP
         WHERE node_id = ? AND project_id = ? AND revoked_at IS NULL",
    )
    .bind(node_id)
    .bind(project_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub async fn authenticate_node_token(
    pool: &SqlitePool,
    token: &str,
) -> Result<Option<NodePrincipal>> {
    if !token.starts_with("axn_") || token.len() < 40 {
        return Ok(None);
    }
    let prefix = token.chars().take(12).collect::<String>();
    let verifier = token_verifier(token);
    let row: Option<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT nc.node_id, rn.trust_state, nc.expires_at
         FROM node_credentials nc
         JOIN replication_nodes rn ON rn.id = nc.node_id
         WHERE nc.token_prefix = ?
           AND nc.verifier = ?
           AND nc.revoked_at IS NULL",
    )
    .bind(&prefix)
    .bind(&verifier)
    .fetch_optional(pool)
    .await?;

    let Some((node_id, trust_state, expires_at)) = row else {
        return Ok(None);
    };
    if trust_state != "trusted" {
        return Ok(None);
    }
    if let Some(expires_at) = expires_at {
        let valid: (i64,) = sqlx::query_as(
            "SELECT CASE WHEN datetime(?) > CURRENT_TIMESTAMP THEN 1 ELSE 0 END",
        )
        .bind(expires_at)
        .fetch_one(pool)
        .await?;
        if valid.0 != 1 {
            return Ok(None);
        }
    }

    sqlx::query(
        "UPDATE node_credentials SET last_used_at = CURRENT_TIMESTAMP WHERE token_prefix = ?",
    )
    .bind(&prefix)
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE replication_nodes SET last_seen_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(&node_id)
    .execute(pool)
    .await?;

    Ok(Some(NodePrincipal { node_id }))
}

pub async fn authenticate_node_token_for_project(
    pool: &SqlitePool,
    token: &str,
    project_id: &str,
) -> Result<Option<NodePrincipal>> {
    let Some(principal) = authenticate_node_token(pool, token).await? else {
        return Ok(None);
    };
    let access: Option<(i64,)> = sqlx::query_as(
        "SELECT 1 FROM node_project_access
         WHERE node_id = ? AND project_id = ? AND revoked_at IS NULL
         LIMIT 1",
    )
    .bind(&principal.node_id)
    .bind(project_id)
    .fetch_optional(pool)
    .await?;
    if access.is_none() {
        return Ok(None);
    }
    Ok(Some(principal))
}

pub async fn revoke_node(pool: &SqlitePool, node_id: &str) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let result = sqlx::query(
        "UPDATE replication_nodes
         SET trust_state = 'revoked', revoked_at = CURRENT_TIMESTAMP
         WHERE id = ? AND trust_state != 'revoked'",
    )
    .bind(node_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE node_credentials
         SET revoked_at = CURRENT_TIMESTAMP
         WHERE node_id = ? AND revoked_at IS NULL",
    )
    .bind(node_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(result.rows_affected() == 1)
}

pub async fn list_nodes(pool: &SqlitePool) -> Result<Vec<ReplicationNode>> {
    let rows: Vec<(
        String,
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT id, display_name, role, endpoint, trust_state, credential_id,
                discovered_at, approved_at, revoked_at, last_seen_at, last_sync_at
         FROM replication_nodes ORDER BY discovered_at, id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(
                id,
                display_name,
                role,
                endpoint,
                trust_state,
                credential_id,
                discovered_at,
                approved_at,
                revoked_at,
                last_seen_at,
                last_sync_at,
            )| ReplicationNode {
                id,
                display_name,
                role,
                endpoint,
                trust_state,
                credential_id,
                discovered_at,
                approved_at,
                revoked_at,
                last_seen_at,
                last_sync_at,
            },
        )
        .collect())
}

pub async fn audit(
    pool: &SqlitePool,
    actor_member_id: Option<&str>,
    actor_client_id: Option<&str>,
    event_type: &str,
    subject_type: &str,
    subject_id: &str,
    details: &serde_json::Value,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO security_audit_log
            (actor_member_id, actor_client_id, event_type, subject_type, subject_id, details)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(actor_member_id)
    .bind(actor_client_id)
    .bind(event_type)
    .bind(subject_type)
    .bind(subject_id)
    .bind(details.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open_and_init;

    #[tokio::test]
    async fn access_lifecycle_persists_and_revokes() {
        let dir = tempfile::tempdir().unwrap();
        let pool = open_and_init(&dir.path().join("global.db")).await.unwrap();

        assert!(bootstrap_owner(&pool, "u1", "Owner", Some("owner@example.test"))
            .await
            .unwrap());
        assert!(!bootstrap_owner(&pool, "u2", "Other", None).await.unwrap());

        create_client(
            &pool,
            "c1",
            "u1",
            "Cursor",
            "cursor",
            &["memory:read".into(), "memory:write".into()],
        )
        .await
        .unwrap();
        let issued = issue_client_credential(&pool, "c1", None).await.unwrap();
        assert!(issued.token.starts_with("axc_"));
        let principal = authenticate_client_token(&pool, &issued.token)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(principal.member_id, "u1");
        assert!(principal.allows("memory:read"));
        assert!(principal.allows("memory:write"));
        assert!(!principal.allows("admin:team"));

        assert_eq!(list_members(&pool).await.unwrap().len(), 1);
        assert_eq!(list_clients(&pool).await.unwrap()[0].scopes.len(), 2);
        assert!(revoke_client(&pool, "c1").await.unwrap());
        assert!(authenticate_client_token(&pool, &issued.token)
            .await
            .unwrap()
            .is_none());

        discover_replica(&pool, "n1", "Replica 1", Some("http://10.0.0.2:7070"))
            .await
            .unwrap();
        let node_credential = issue_node_credential(&pool, "n1", None).await.unwrap();
        grant_node_project(&pool, "n1", "project-a").await.unwrap();
        assert!(node_credential.token.starts_with("axn_"));
        assert_eq!(
            authenticate_node_token_for_project(&pool, &node_credential.token, "project-a")
                .await
                .unwrap()
                .unwrap()
                .node_id,
            "n1"
        );
        assert!(authenticate_node_token_for_project(&pool, &node_credential.token, "project-b")
            .await
            .unwrap()
            .is_none());
        assert!(authenticate_client_token(&pool, &node_credential.token)
            .await
            .unwrap()
            .is_none());
        assert!(revoke_node(&pool, "n1").await.unwrap());
        assert!(authenticate_node_token(&pool, &node_credential.token)
            .await
            .unwrap()
            .is_none());
        assert_eq!(list_nodes(&pool).await.unwrap()[0].trust_state, "revoked");
    }
}
