//! Shared guard for local owner-only browser APIs, without bearer/share authority.
use ax_global_db::access;
use axum::http::HeaderMap;
use sqlx::SqlitePool;

pub(crate) fn browser_headers(
    headers: &HeaderMap,
    port: u16,
    readonly: bool,
    mutation: bool,
    csrf: &str,
) -> Result<(), &'static str> {
    if readonly {
        return Err("Local writable Command Center required");
    }
    if headers.contains_key("authorization")
        || headers.contains_key("mcp-session-id")
        || headers
            .get("cookie")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| {
                v.split(';')
                    .any(|part| part.trim().starts_with("ax_share="))
            })
    {
        return Err("Remote credentials and share sessions cannot manage project publications");
    }
    let local = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !local.iter().any(|h| h == host) {
        return Err("Local Command Center host required");
    }
    if headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) == Some("cross-site") {
        return Err("Cross-site request rejected");
    }
    if headers.get("origin").is_some_and(|v| {
        v.to_str()
            .ok()
            .is_none_or(|v| !local.iter().any(|h| v == format!("http://{h}")))
    }) {
        return Err("Local Command Center origin required");
    }
    if mutation && headers.get("x-ax-csrf").and_then(|v| v.to_str().ok()) != Some(csrf) {
        return Err("Reload Settings before changing the registration");
    }
    Ok(())
}
pub(crate) async fn require_active_owner(pool: &SqlitePool) -> Result<(), &'static str> {
    let members = access::list_members(pool)
        .await
        .map_err(|_| "Cannot verify local owner")?;
    if !members
        .iter()
        .any(|m| m.role == access::MemberRole::Owner && m.status == "active")
    {
        return Err("No active local owner");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owner_surface_rejects_bearer_share_and_foreign_browser_authority() {
        let mut h = HeaderMap::new();
        h.insert("host", "localhost:7070".parse().unwrap());
        assert!(browser_headers(&h, 7070, false, false, "csrf").is_ok());
        assert!(browser_headers(&h, 7070, false, true, "csrf").is_err());
        h.insert("x-ax-csrf", "csrf".parse().unwrap());
        assert!(browser_headers(&h, 7070, false, true, "csrf").is_ok());
        for (name, value) in [
            ("authorization", "Bearer axc_owner"),
            ("cookie", "ax_share=test"),
            ("mcp-session-id", "remote"),
            ("origin", "http://localhost:8080"),
            ("sec-fetch-site", "cross-site"),
            ("host", "attacker.example"),
        ] {
            let mut foreign = h.clone();
            foreign.insert(name, value.parse().unwrap());
            assert!(
                browser_headers(&foreign, 7070, false, false, "csrf").is_err(),
                "{name}"
            );
        }
        assert!(browser_headers(&h, 7070, true, false, "csrf").is_err());
    }
    #[tokio::test]
    async fn active_owner_is_required_in_database() {
        let temp = tempfile::tempdir().unwrap();
        let pool = ax_global_db::open_and_init(&temp.path().join("global.db"))
            .await
            .unwrap();
        assert!(require_active_owner(&pool).await.is_err());
        access::bootstrap_owner(&pool, "owner", "Owner", None)
            .await
            .unwrap();
        assert!(require_active_owner(&pool).await.is_ok());
        sqlx::query("UPDATE team_members SET status='disabled'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(require_active_owner(&pool).await.is_err());
    }
}
