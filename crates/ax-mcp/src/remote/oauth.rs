//! Built-in public-client OAuth with PKCE and explicit local-owner approval.
use super::{
    auth::{Principal, CONTEXT_SCOPE, WRITE_SCOPE},
    config::{ClientAddressHeader, RemoteConfig},
};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Form, Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
};
use url::Url;
fn hash(secret: &str) -> String {
    hex::encode(Sha256::digest(secret.as_bytes()))
}
fn secret() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}
fn now() -> i64 {
    ax_global_db::remote_mcp_publications::now()
}
/// Logs a database failure and returns text that is safe to show any caller.
fn db(error: sqlx::Error) -> String {
    tracing::error!("remote OAuth database error: {error}");
    "OAuth database unavailable".into()
}
/// An OAuth endpoint failure: an RFC 6749 error code, its HTTP status and a
/// description that never carries internal details.
#[derive(Debug)]
struct Fail {
    status: StatusCode,
    code: &'static str,
    description: String,
}
impl Fail {
    fn client(code: &'static str, description: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code,
            description: description.into(),
        }
    }
    fn grant(description: &str) -> Self {
        Self::client("invalid_grant", description)
    }
    fn into_response(self) -> Response {
        response(
            self.status,
            json!({"error":self.code,"error_description":self.description}),
        )
    }
}
impl From<&str> for Fail {
    fn from(description: &str) -> Self {
        Self::client("invalid_request", description)
    }
}
impl From<String> for Fail {
    fn from(description: String) -> Self {
        Self::client("invalid_request", description)
    }
}
/// A storage failure in an OAuth endpoint: logged here, reported as `server_error`.
fn server(error: impl std::fmt::Display) -> Fail {
    tracing::error!("remote OAuth endpoint failed: {error}");
    Fail {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        code: "server_error",
        description: "The authorization server could not complete the request".into(),
    }
}
#[derive(Clone)]
pub struct OAuthStore {
    pub pool: SqlitePool,
    pub origin: String,
}
impl OAuthStore {
    pub async fn open(origin: &str) -> Result<Self, String> {
        let pool = ax_global_db::open_and_init(
            &ax_global_db::global_db_path().map_err(|e| e.to_string())?,
        )
        .await
        .map_err(|e| e.to_string())?;
        Self::with_pool(pool, origin).await
    }
    pub async fn with_pool(pool: SqlitePool, origin: &str) -> Result<Self, String> {
        for query in [
            "CREATE TABLE IF NOT EXISTS remote_oauth_clients(origin TEXT NOT NULL,id TEXT NOT NULL,name TEXT NOT NULL,redirects TEXT NOT NULL,native INTEGER NOT NULL DEFAULT 0,created INTEGER NOT NULL,PRIMARY KEY(origin,id))",
            "CREATE TABLE IF NOT EXISTS remote_oauth_requests(origin TEXT NOT NULL,id TEXT PRIMARY KEY,nonce_hash TEXT NOT NULL UNIQUE,client_id TEXT NOT NULL,redirect TEXT NOT NULL,state TEXT NOT NULL,challenge TEXT NOT NULL,scope TEXT NOT NULL,project TEXT NOT NULL,resource TEXT NOT NULL,expires INTEGER NOT NULL,status TEXT NOT NULL DEFAULT 'pending',code_hash TEXT UNIQUE,revoked INTEGER NOT NULL DEFAULT 0)",
            "CREATE TABLE IF NOT EXISTS remote_oauth_tokens(origin TEXT NOT NULL,access_hash TEXT PRIMARY KEY,refresh_hash TEXT NOT NULL UNIQUE,request_id TEXT NOT NULL,client_id TEXT NOT NULL,expires INTEGER NOT NULL,refresh_expires INTEGER NOT NULL)",
            "CREATE TABLE IF NOT EXISTS remote_oauth_used_refresh(origin TEXT NOT NULL,hash TEXT NOT NULL,client_id TEXT NOT NULL,request_id TEXT NOT NULL,expires INTEGER NOT NULL,PRIMARY KEY(origin,hash))",
        ] {sqlx::query(query).execute(&pool).await.map_err(db)?;}
        // The caller a sign-in came from, so one source cannot push out everyone's.
        let has_source: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_table_info('remote_oauth_requests') WHERE name='source'",
        )
        .fetch_one(&pool)
        .await
        .map_err(db)?;
        if has_source == 0 {
            sqlx::query(
                "ALTER TABLE remote_oauth_requests ADD COLUMN source TEXT NOT NULL DEFAULT ''",
            )
            .execute(&pool)
            .await
            .map_err(db)?;
        }
        Ok(Self {
            pool,
            origin: origin.into(),
        })
    }
    pub async fn native_client(&self) -> Result<String, String> {
        let id = format!("ax-native-{}", &hash(&self.origin)[..24]);
        sqlx::query("INSERT OR IGNORE INTO remote_oauth_clients VALUES (?,?,?,? ,1,?)")
            .bind(&self.origin)
            .bind(&id)
            .bind("Ax native client")
            .bind("[]")
            .bind(now())
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(id)
    }
    pub async fn requests(&self, project: &str) -> Result<Vec<Value>, String> {
        let rows = sqlx::query("SELECT r.id,c.name,r.scope,r.status,r.expires,r.revoked,r.redirect, EXISTS(SELECT 1 FROM remote_oauth_tokens t WHERE t.origin=r.origin AND t.request_id=r.id AND (t.expires>? OR t.refresh_expires>?)) AS live FROM remote_oauth_requests r JOIN remote_oauth_clients c ON c.origin=r.origin AND c.id=r.client_id WHERE r.origin=? AND r.project=? ORDER BY r.expires DESC LIMIT 100")
            .bind(now()).bind(now()).bind(&self.origin).bind(project).fetch_all(&self.pool).await.map_err(db)?;
        Ok(rows.iter().map(|r| {
            let status: String = r.get("status");
            let expired = (matches!(status.as_str(), "pending" | "approved") && r.get::<i64,_>("expires") <= now()) || (status == "delivered" && r.get::<i64,_>("live") == 0);
            json!({"id":r.get::<String,_>("id"),"clientName":r.get::<String,_>("name"),"scope":r.get::<String,_>("scope"),"status":if expired {"expired"} else {&status},"expires":r.get::<i64,_>("expires"),"revoked":r.get::<i64,_>("revoked")!=0,"redirectUri":r.get::<String,_>("redirect")})
        }).collect())
    }
    pub async fn decide(&self, project: &str, id: &str, decision: &str) -> Result<(), String> {
        if decision == "remove" {
            return self.remove_inactive(project, id).await;
        }
        let result=match decision {
            "approve"|"deny"=>sqlx::query("UPDATE remote_oauth_requests SET status=? WHERE origin=? AND project=? AND id=? AND status='pending' AND expires>? AND revoked=0").bind(if decision=="approve" {"approved"} else {"denied"}).bind(&self.origin).bind(project).bind(id).bind(now()).execute(&self.pool).await,
            "revoke"=>sqlx::query("UPDATE remote_oauth_requests SET revoked=1 WHERE origin=? AND project=? AND id=?").bind(&self.origin).bind(project).bind(id).execute(&self.pool).await,
            _=>return Err("Unknown authorization decision".into()),
        }.map_err(db)?;
        if result.rows_affected() != 1 {
            return Err("Authorization request is expired or no longer pending".into());
        }
        Ok(())
    }
    async fn remove_inactive(&self, project: &str, id: &str) -> Result<(), String> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let removable: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM remote_oauth_requests r WHERE r.origin=? AND r.project=? AND r.id=? AND (r.revoked=1 OR r.status='denied' OR (r.status IN ('pending','approved') AND r.expires<=?) OR (r.status='delivered' AND NOT EXISTS(SELECT 1 FROM remote_oauth_tokens t WHERE t.origin=r.origin AND t.request_id=r.id AND (t.expires>? OR t.refresh_expires>?))))")
            .bind(&self.origin).bind(project).bind(id).bind(now()).bind(now()).bind(now()).fetch_one(&mut *tx).await.map_err(db)?;
        if removable != 1 {
            return Err("Only revoked, denied or expired connections can be removed".into());
        }
        for table in ["remote_oauth_tokens", "remote_oauth_used_refresh"] {
            sqlx::query(&format!(
                "DELETE FROM {table} WHERE origin=? AND request_id=?"
            ))
            .bind(&self.origin)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        sqlx::query("DELETE FROM remote_oauth_requests WHERE origin=? AND project=? AND id=?")
            .bind(&self.origin)
            .bind(project)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(())
    }

    pub async fn revoke_project(&self, project: &str) -> Result<(), String> {
        sqlx::query("UPDATE remote_oauth_requests SET revoked=1 WHERE origin=? AND project=?")
            .bind(&self.origin)
            .bind(project)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }
    pub async fn verify(&self, token: &str) -> Result<Principal, String> {
        let row=sqlx::query("SELECT r.id,r.project,r.scope FROM remote_oauth_tokens t JOIN remote_oauth_requests r ON r.origin=t.origin AND r.id=t.request_id WHERE t.origin=? AND t.access_hash=? AND t.expires>? AND r.status='delivered' AND r.revoked=0").bind(&self.origin).bind(hash(token)).bind(now()).fetch_optional(&self.pool).await.map_err(db)?.ok_or("Invalid or expired access token")?;
        let scopes: HashSet<String> = row
            .get::<String, _>("scope")
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        Ok(Principal {
            id: format!("oauth:{}", row.get::<String, _>("id")),
            subject: None,
            key_projects: vec![row.get("project")],
            key_write: scopes.contains(WRITE_SCOPE),
            // The built-in authorization server issues context and write scopes only.
            key_admin: false,
            scopes,
        })
    }
}
/// Pending authorization requests kept for the whole host.
const MAX_PENDING: i64 = 128;
/// Pending authorization requests one dynamically registered client may hold.
const MAX_PENDING_PER_CLIENT: i64 = 16;
/// Pending authorization requests one source address may hold, for every
/// client including the native one.
const MAX_PENDING_PER_SOURCE: i64 = 8;
/// Registered clients kept for the whole host.
const MAX_CLIENTS: i64 = 512;
/// A registered client that never started a sign-in is removed after this many seconds.
const UNUSED_CLIENT_TTL: i64 = 3600;
/// Unauthenticated calls to `/oauth/authorize` and `/oauth/register` (each) per
/// minute from one source address.
const OAUTH_CALLS_PER_MINUTE: u32 = 30;
/// Unauthenticated calls to `/oauth/authorize` and `/oauth/register` (each) per
/// minute from all source addresses together. It bounds a caller that has many
/// addresses: at this pace a flood needs more than two minutes to push the
/// oldest of [`MAX_PENDING`] waiting sign-ins out.
const HOST_OAUTH_CALLS_PER_MINUTE: u32 = 60;
/// Source addresses whose call counts are kept at once. Further addresses
/// share one window.
const MAX_RATE_SOURCES: usize = 4096;
/// The source of a request when no trusted proxy names the client address.
const UNKNOWN_SOURCE: &str = "direct";

/// The client address a request came from, as named by the trusted proxy in
/// front of Ax. The listener binds to loopback (or a socket bridge, or a
/// platform ingress), so the peer address is always that proxy. Without a
/// configured [`ClientAddressHeader`] every request has the same source: a
/// caller can set `CF-Connecting-IP` or `X-Forwarded-For` itself when no proxy
/// overwrites them. Only used to share out limits, never to trust a caller.
fn source(headers: &HeaderMap, trusted: Option<ClientAddressHeader>) -> String {
    let ip = |value: &str| value.trim().parse::<std::net::IpAddr>().ok();
    let named = match trusted {
        None => None,
        Some(ClientAddressHeader::CfConnectingIp) => headers
            .get("cf-connecting-ip")
            .and_then(|v| v.to_str().ok())
            .and_then(ip),
        Some(ClientAddressHeader::XForwardedFor) => headers
            .get_all("x-forwarded-for")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(','))
            .last()
            .and_then(ip),
    };
    named.map_or_else(|| UNKNOWN_SOURCE.to_string(), |ip| ip.to_string())
}

/// A fixed one-minute window of calls.
#[derive(Clone, Copy)]
struct Window {
    start: std::time::Instant,
    calls: u32,
}
impl Window {
    fn new() -> Self {
        Self {
            start: std::time::Instant::now(),
            calls: 0,
        }
    }
    /// Starts a new minute when this one ran out.
    fn current(&mut self) -> &mut Self {
        if self.start.elapsed() >= std::time::Duration::from_secs(60) {
            *self = Self::new();
        }
        self
    }
    fn retry_after(&self) -> u64 {
        60u64.saturating_sub(self.start.elapsed().as_secs()).max(1)
    }
}

#[derive(Default)]
struct Windows {
    by_source: std::collections::HashMap<String, Window>,
    /// Shared by the sources that found the table full.
    overflow: Option<Window>,
    host: Option<Window>,
}

/// Counts calls to an unauthenticated endpoint in one-minute windows: per
/// source address, so one caller cannot use up everyone's calls, and for the
/// whole host, so a caller with many addresses cannot either.
struct RateLimit {
    per_minute: u32,
    host_per_minute: u32,
    windows: std::sync::Mutex<Windows>,
}
impl RateLimit {
    fn new(per_minute: u32, host_per_minute: u32) -> Self {
        Self {
            per_minute,
            host_per_minute,
            windows: std::sync::Mutex::default(),
        }
    }
    /// Counts one call from `source`; `Err(seconds)` tells how long until a
    /// window reopens. A source that finds the address table full is counted
    /// in one window shared by all such sources, never let through uncounted.
    fn check(&self, source: &str) -> Result<(), u64> {
        let mut windows = self.windows.lock().unwrap_or_else(|e| e.into_inner());
        let Windows {
            by_source,
            overflow,
            host,
        } = &mut *windows;
        if !by_source.contains_key(source) && by_source.len() >= MAX_RATE_SOURCES {
            by_source.retain(|_, w| w.start.elapsed() < std::time::Duration::from_secs(60));
        }
        let mine = if by_source.contains_key(source) || by_source.len() < MAX_RATE_SOURCES {
            by_source
                .entry(source.to_owned())
                .or_insert_with(Window::new)
        } else {
            overflow.get_or_insert_with(Window::new)
        }
        .current();
        if mine.calls >= self.per_minute {
            return Err(mine.retry_after());
        }
        let host = host.get_or_insert_with(Window::new).current();
        if host.calls >= self.host_per_minute {
            return Err(host.retry_after());
        }
        mine.calls += 1;
        host.calls += 1;
        Ok(())
    }
}
fn too_many(retry_after: u64) -> Response {
    let mut r = response(
        StatusCode::TOO_MANY_REQUESTS,
        json!({"error":"temporarily_unavailable","error_description":"Too many OAuth requests; retry later"}),
    );
    if let Ok(value) = HeaderValue::from_str(&retry_after.to_string()) {
        r.headers_mut().insert("retry-after", value);
    }
    r
}

#[derive(Clone)]
struct Provider {
    store: OAuthStore,
    config: Arc<RemoteConfig>,
    authorize_limit: Arc<RateLimit>,
    register_limit: Arc<RateLimit>,
}
impl Provider {
    fn new(
        store: OAuthStore,
        config: Arc<RemoteConfig>,
        per_minute: u32,
        host_per_minute: u32,
    ) -> Self {
        Self {
            store,
            config,
            authorize_limit: Arc::new(RateLimit::new(per_minute, host_per_minute)),
            register_limit: Arc::new(RateLimit::new(per_minute, host_per_minute)),
        }
    }
    /// The client address of a request, as far as the configuration trusts it.
    fn source(&self, headers: &HeaderMap) -> String {
        source(headers, self.config.client_address_header)
    }
}
fn response(status: StatusCode, value: Value) -> Response {
    secure((status, Json(value)).into_response())
}
fn secure(mut r: Response) -> Response {
    for (name, value) in [
        ("cache-control", "no-store"),
        ("pragma", "no-cache"),
        ("referrer-policy", "no-referrer"),
        ("x-content-type-options", "nosniff"),
    ] {
        r.headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    r
}
pub fn router(store: OAuthStore, config: Arc<RemoteConfig>) -> Router {
    Router::new()
        .route(
            "/.well-known/oauth-authorization-server/oauth",
            get(metadata),
        )
        .route("/oauth/.well-known/openid-configuration", get(metadata))
        .route(
            "/oauth/jwks",
            get(|| async { response(StatusCode::OK, json!({"keys":[]})) }),
        )
        .route("/oauth/register", post(register))
        .route("/oauth/authorize", get(authorize))
        .route("/oauth/requests/{nonce}", get(poll))
        .route("/oauth/token", post(token))
        .route("/oauth/revoke", post(revoke))
        .with_state(Provider::new(
            store,
            config,
            OAUTH_CALLS_PER_MINUTE,
            HOST_OAUTH_CALLS_PER_MINUTE,
        ))
}
async fn metadata(State(p): State<Provider>) -> Response {
    let origin = &p.store.origin;
    response(
        StatusCode::OK,
        json!({"issuer":format!("{origin}/oauth"),"authorization_endpoint":format!("{origin}/oauth/authorize"),"token_endpoint":format!("{origin}/oauth/token"),"registration_endpoint":format!("{origin}/oauth/register"),"revocation_endpoint":format!("{origin}/oauth/revoke"),"response_types_supported":["code"],"grant_types_supported":["authorization_code","refresh_token"],"token_endpoint_auth_methods_supported":["none"],"code_challenge_methods_supported":["S256"],"scopes_supported":[CONTEXT_SCOPE,WRITE_SCOPE],"authorization_response_iss_parameter_supported":true}),
    )
}
fn redirect(value: &str, native: bool) -> bool {
    if value.len() > 2048 {
        return false;
    }
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return false;
    }
    let loopback = url.host_str().is_some_and(|h| {
        h.trim_matches(['[', ']'])
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
    });
    if native {
        url.scheme() == "http"
            && url.host_str() == Some("127.0.0.1")
            && url.port().is_some()
            && url.path() == "/callback"
            && url.query().is_none()
    } else {
        url.host_str().is_some()
            && (url.scheme() == "https" || (url.scheme() == "http" && loopback))
    }
}
async fn register(
    State(p): State<Provider>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if let Err(retry) = p.register_limit.check(&p.source(&headers)) {
        return too_many(retry);
    }
    let result=async {
        if body.get("token_endpoint_auth_method").and_then(Value::as_str).is_some_and(|s|s!="none") {return Err("Only public clients are supported".into());}
        for (key,allowed) in [("grant_types",vec!["authorization_code","refresh_token"]),("response_types",vec!["code"])] {
            if let Some(value)=body.get(key) {let items=value.as_array().ok_or("Invalid client metadata")?; if items.iter().any(|v|v.as_str().is_none_or(|s|!allowed.contains(&s))) {return Err("Unsupported client grant or response type".into());}}
        }
        let values=body["redirect_uris"].as_array().ok_or("redirect_uris is required")?;
        let redirects:Vec<&str>=values.iter().filter_map(Value::as_str).collect();
        if redirects.is_empty() || redirects.len()>8 || redirects.len()!=values.len() || redirects.iter().any(|s|!redirect(s,false)) {return Err("Register exact HTTPS or IP-loopback redirect URIs".into());}
        let name=body.get("client_name").and_then(Value::as_str).unwrap_or("Remote MCP client");
        if name.is_empty() || name.len()>160 || name.chars().any(char::is_control) {return Err("Invalid client name".into());}
        let mut tx=p.store.pool.begin().await.map_err(server)?;
        // Acquire the SQLite write lock before checking the public registration quota.
        let unused="origin=? AND native=0 AND id NOT IN (SELECT client_id FROM remote_oauth_requests WHERE origin=?)";
        sqlx::query(&format!("DELETE FROM remote_oauth_clients WHERE {unused} AND created<?")).bind(&p.store.origin).bind(&p.store.origin).bind(now()-UNUSED_CLIENT_TTL).execute(&mut *tx).await.map_err(server)?;
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM remote_oauth_clients WHERE origin=?").bind(&p.store.origin).fetch_one(&mut *tx).await.map_err(server)?;
        if count>=MAX_CLIENTS {
            // Make room by removing the oldest client that never started a sign-in.
            let evicted=sqlx::query(&format!("DELETE FROM remote_oauth_clients WHERE rowid IN (SELECT rowid FROM remote_oauth_clients WHERE {unused} ORDER BY created, rowid LIMIT ?)")).bind(&p.store.origin).bind(&p.store.origin).bind(count-MAX_CLIENTS+1).execute(&mut *tx).await.map_err(server)?;
            if count-(evicted.rows_affected() as i64)>=MAX_CLIENTS {return Err("Client registration capacity reached".into());}
        }
        let id=uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO remote_oauth_clients VALUES (?,?,?,?,0,?)").bind(&p.store.origin).bind(&id).bind(name).bind(serde_json::to_string(&redirects).map_err(server)?).bind(now()).execute(&mut *tx).await.map_err(server)?;
        tx.commit().await.map_err(server)?;
        Ok::<_,Fail>(json!({"client_id":id,"client_id_issued_at":now(),"client_name":name,"redirect_uris":redirects,"token_endpoint_auth_method":"none","grant_types":["authorization_code","refresh_token"],"response_types":["code"]}))
    }.await;
    match result {
        Ok(value) => response(StatusCode::CREATED, value),
        Err(e) => e.into_response(),
    }
}
#[derive(Clone, Deserialize)]
struct Authorization {
    client_id: String,
    redirect_uri: String,
    response_type: String,
    state: String,
    code_challenge: String,
    code_challenge_method: String,
    scope: String,
    resource: String,
}
async fn authorize(
    State(p): State<Provider>,
    headers: HeaderMap,
    Query(q): Query<Authorization>,
) -> Response {
    let source = p.source(&headers);
    if let Err(retry) = p.authorize_limit.check(&source) {
        return too_many(retry);
    }
    let result=async {
        let row=sqlx::query("SELECT redirects,native FROM remote_oauth_clients WHERE origin=? AND id=?").bind(&p.store.origin).bind(&q.client_id).fetch_optional(&p.store.pool).await.map_err(server)?.ok_or("Unknown OAuth client")?;
        let native=row.get::<i64,_>("native")!=0; let registered:Vec<String>=serde_json::from_str(&row.get::<String,_>("redirects")).map_err(|_|"Invalid registered client")?;
        if !redirect(&q.redirect_uri,native) || (!native && !registered.contains(&q.redirect_uri)) {return Err("Redirect URI is not registered".into());}
        let project=p.config.projects.iter().find(|project|q.resource==format!("{}/projects/{}/mcp",p.store.origin,project.id) || (p.config.projects.len()==1 && q.resource==p.store.origin)).ok_or("Unknown project resource")?;
        let scope:HashSet<_>=q.scope.split_whitespace().collect();
        if q.response_type!="code" || q.state.is_empty() || q.state.len()>2048 || q.code_challenge_method!="S256" || q.code_challenge.len()!=43 || URL_SAFE_NO_PAD.decode(&q.code_challenge).map_or(true,|v|v.len()!=32) || !scope.contains(CONTEXT_SCOPE) || scope.iter().any(|s|!matches!(*s,CONTEXT_SCOPE|WRITE_SCOPE)) {return Err("Authorization requires state, S256 PKCE and supported scopes".into());}
        let mut tx=p.store.pool.begin().await.map_err(server)?;
        sqlx::query("DELETE FROM remote_oauth_requests WHERE origin=? AND expires<? AND status IN ('pending','denied')").bind(&p.store.origin).bind(now()).execute(&mut *tx).await.map_err(server)?;
        // A full table never refuses a new sign-in: the oldest pending request makes room.
        let oldest="DELETE FROM remote_oauth_requests WHERE id IN (SELECT id FROM remote_oauth_requests WHERE origin=? AND status='pending'";
        // One source makes room among its own requests first, whatever the client.
        if source!=UNKNOWN_SOURCE {
            let theirs:i64=sqlx::query_scalar("SELECT COUNT(*) FROM remote_oauth_requests WHERE origin=? AND status='pending' AND source=?").bind(&p.store.origin).bind(&source).fetch_one(&mut *tx).await.map_err(server)?;
            if theirs>=MAX_PENDING_PER_SOURCE {sqlx::query(&format!("{oldest} AND source=? ORDER BY expires, rowid LIMIT ?)")).bind(&p.store.origin).bind(&source).bind(theirs-MAX_PENDING_PER_SOURCE+1).execute(&mut *tx).await.map_err(server)?;}
        }
        if !native {
            let mine:i64=sqlx::query_scalar("SELECT COUNT(*) FROM remote_oauth_requests WHERE origin=? AND status='pending' AND client_id=?").bind(&p.store.origin).bind(&q.client_id).fetch_one(&mut *tx).await.map_err(server)?;
            if mine>=MAX_PENDING_PER_CLIENT {sqlx::query(&format!("{oldest} AND client_id=? ORDER BY expires, rowid LIMIT ?)")).bind(&p.store.origin).bind(&q.client_id).bind(mine-MAX_PENDING_PER_CLIENT+1).execute(&mut *tx).await.map_err(server)?;}
        }
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM remote_oauth_requests WHERE origin=? AND status='pending'").bind(&p.store.origin).fetch_one(&mut *tx).await.map_err(server)?;
        if count>=MAX_PENDING {sqlx::query(&format!("{oldest} ORDER BY expires, rowid LIMIT ?)")).bind(&p.store.origin).bind(count-MAX_PENDING+1).execute(&mut *tx).await.map_err(server)?;}
        let id=uuid::Uuid::new_v4().to_string(); let nonce=secret(); let scope=if scope.contains(WRITE_SCOPE) {"ax:context ax:write"} else {CONTEXT_SCOPE};
        sqlx::query("INSERT INTO remote_oauth_requests(origin,id,nonce_hash,client_id,redirect,state,challenge,scope,project,resource,expires,source) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)").bind(&p.store.origin).bind(&id).bind(hash(&nonce)).bind(&q.client_id).bind(&q.redirect_uri).bind(&q.state).bind(&q.code_challenge).bind(scope).bind(&project.id).bind(&q.resource).bind(now()+600).bind(&source).execute(&mut *tx).await.map_err(server)?;
        tx.commit().await.map_err(server)?; Ok::<_,Fail>((id,nonce))
    }.await;
    match result {
        Err(e) => e.into_response(),
        Ok((id, nonce)) => {
            // Only generated UUID/hex values are embedded; no caller-controlled HTML or URL.
            let html = format!(
                r##"<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>Ax authorization</title><style>body{{font:16px system-ui;background:#1b1b1b;color:#f0f0f0;max-width:40rem;margin:12vh auto;padding:2rem}}code{{overflow-wrap:anywhere}}p{{line-height:1.6}}</style><svg viewBox="0 0 162 100" height="56" role="img" aria-label="Ax"><path fill="#f0f0f0" d="M0 100 60 0l60 100H92L60 46.667 28 100ZM68 0h28l60 100h-28Z"/><path fill="#A2F50F" d="M134 0h28l-29 48.333L119 25Z"/></svg><h1>Connect to Ax</h1><p>Ask the owner of the hosting computer to approve this request in Settings → Remote MCP. Match this request ID:</p><p><code>{id}</code></p><p id="status">Waiting for host approval…</p><script>const poll=async()=>{{try{{const r=await fetch('/oauth/requests/{nonce}',{{cache:'no-store'}});const data=await r.json();if(data.redirect){{location.replace(data.redirect);return}}if(data.status==='denied'||data.status==='expired'){{document.getElementById('status').textContent='Request '+data.status+'. Start a new connection to retry.';return}}}}catch{{}}setTimeout(poll,1500)}};poll();</script></html>"##
            );
            let mut r = secure(Html(html).into_response());
            r.headers_mut().insert("content-security-policy",HeaderValue::from_static("default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'"));
            r
        }
    }
}
async fn poll(State(p): State<Provider>, Path(nonce): Path<String>) -> Response {
    let result=async {
        if nonce.len()!=64 || !nonce.bytes().all(|b|b.is_ascii_hexdigit()) {return Err("Invalid request".into());}
        let row=sqlx::query("SELECT id,status,expires,revoked,redirect,state FROM remote_oauth_requests WHERE origin=? AND nonce_hash=?").bind(&p.store.origin).bind(hash(&nonce)).fetch_optional(&p.store.pool).await.map_err(server)?.ok_or("Unknown request")?;
        if row.get::<i64,_>("expires")<=now() || row.get::<i64,_>("revoked")!=0 {return Ok(json!({"status":"expired"}));}
        let status:String=row.get("status"); if status!="approved" {return Ok(json!({"status":status}));}
        let code=secret(); let changed=sqlx::query("UPDATE remote_oauth_requests SET status='delivered',code_hash=? WHERE origin=? AND id=? AND status='approved' AND revoked=0 AND expires>?").bind(hash(&code)).bind(&p.store.origin).bind(row.get::<String,_>("id")).bind(now()).execute(&p.store.pool).await.map_err(server)?;
        if changed.rows_affected()!=1 {return Ok(json!({"status":"expired"}));}
        let mut url=Url::parse(&row.get::<String,_>("redirect")).map_err(|_|"Invalid redirect")?; url.query_pairs_mut().extend_pairs([("code",code.as_str()),("state",row.get::<String,_>("state").as_str()),("iss",format!("{}/oauth",p.store.origin).as_str())]);
        Ok::<_,Fail>(json!({"status":"approved","redirect":url.as_str()}))
    }.await;
    match result {
        Ok(v) => response(StatusCode::OK, v),
        Err(e) => e.into_response(),
    }
}
async fn token(State(p): State<Provider>, Form(q): Form<BTreeMap<String, String>>) -> Response {
    let result=async {
        let value=|key:&str|q.get(key).map(String::as_str).unwrap_or(""); let client=value("client_id");
        let mut tx=p.store.pool.begin().await.map_err(server)?;
        sqlx::query("DELETE FROM remote_oauth_tokens WHERE origin=? AND refresh_expires<=?").bind(&p.store.origin).bind(now()).execute(&mut *tx).await.map_err(server)?;
        sqlx::query("DELETE FROM remote_oauth_used_refresh WHERE origin=? AND expires<=?").bind(&p.store.origin).bind(now()).execute(&mut *tx).await.map_err(server)?;
        let row=match value("grant_type") {
            "authorization_code"=>{
                let row=sqlx::query("SELECT * FROM remote_oauth_requests WHERE origin=? AND code_hash=? AND client_id=? AND status='delivered' AND revoked=0 AND expires>?").bind(&p.store.origin).bind(hash(value("code"))).bind(client).bind(now()).fetch_optional(&mut *tx).await.map_err(server)?.ok_or_else(||Fail::grant("Invalid authorization grant"))?;
                let verifier=value("code_verifier");
                if !(43..=128).contains(&verifier.len()) || !verifier.bytes().all(|b|b.is_ascii_alphanumeric() || b"-._~".contains(&b)) || URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))!=row.get::<String,_>("challenge") || value("redirect_uri")!=row.get::<String,_>("redirect") {return Err(Fail::grant("Invalid authorization grant"));} row
            },
            "refresh_token"=>{
                let row=sqlx::query("SELECT r.* FROM remote_oauth_tokens t JOIN remote_oauth_requests r ON r.origin=t.origin AND r.id=t.request_id WHERE t.origin=? AND t.refresh_hash=? AND t.client_id=? AND t.refresh_expires>? AND r.revoked=0").bind(&p.store.origin).bind(hash(value("refresh_token"))).bind(client).bind(now()).fetch_optional(&mut *tx).await.map_err(server)?;
                if let Some(row)=row {row} else {
                    let id:Option<String>=sqlx::query_scalar("SELECT request_id FROM remote_oauth_used_refresh WHERE origin=? AND hash=? AND client_id=? AND expires>?").bind(&p.store.origin).bind(hash(value("refresh_token"))).bind(client).bind(now()).fetch_optional(&mut *tx).await.map_err(server)?;
                    if let Some(id)=id {sqlx::query("UPDATE remote_oauth_requests SET revoked=1 WHERE origin=? AND id=?").bind(&p.store.origin).bind(id).execute(&mut *tx).await.map_err(server)?;tx.commit().await.map_err(server)?;}
                    return Err(Fail::grant("Invalid refresh grant"));
                }
            },
            _=>return Err(Fail::client("unsupported_grant_type","Unsupported grant type")),
        };
        if value("resource")!=row.get::<String,_>("resource") {return Err(Fail::client("invalid_target","Invalid resource"));}
        let id:String=row.get("id");
        if value("grant_type")=="authorization_code" {sqlx::query("UPDATE remote_oauth_requests SET code_hash=NULL WHERE origin=? AND id=?").bind(&p.store.origin).bind(&id).execute(&mut *tx).await.map_err(server)?;}
        else {
            sqlx::query("INSERT INTO remote_oauth_used_refresh VALUES (?,?,?,?,?)").bind(&p.store.origin).bind(hash(value("refresh_token"))).bind(client).bind(&id).bind(now()+30*86400).execute(&mut *tx).await.map_err(server)?;
            sqlx::query("DELETE FROM remote_oauth_tokens WHERE origin=? AND refresh_hash=?").bind(&p.store.origin).bind(hash(value("refresh_token"))).execute(&mut *tx).await.map_err(server)?;
        }
        let access=format!("axo_{}",secret());let refresh=secret();
        sqlx::query("INSERT INTO remote_oauth_tokens VALUES (?,?,?,?,?,?,?)").bind(&p.store.origin).bind(hash(&access)).bind(hash(&refresh)).bind(&id).bind(client).bind(now()+3600).bind(now()+30*86400).execute(&mut *tx).await.map_err(server)?;
        tx.commit().await.map_err(server)?;
        Ok::<_,Fail>(json!({"access_token":access,"token_type":"Bearer","expires_in":3600,"refresh_token":refresh,"scope":row.get::<String,_>("scope")}))
    }.await;
    match result {
        Ok(v) => response(StatusCode::OK, v),
        Err(e) => e.into_response(),
    }
}
async fn revoke(State(p): State<Provider>, Form(q): Form<BTreeMap<String, String>>) -> Response {
    let token = q.get("token").map(String::as_str).unwrap_or("");
    let client = q.get("client_id").map(String::as_str).unwrap_or("");
    match sqlx::query("UPDATE remote_oauth_requests SET revoked=1 WHERE origin=? AND id IN (SELECT request_id FROM remote_oauth_tokens WHERE origin=? AND client_id=? AND (access_hash=? OR refresh_hash=?))").bind(&p.store.origin).bind(&p.store.origin).bind(client).bind(hash(token)).bind(hash(token)).execute(&p.store.pool).await {Ok(_)=>response(StatusCode::OK,json!({})),Err(_)=>response(StatusCode::INTERNAL_SERVER_ERROR,json!({"error":"server_error"}))}
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn provider() -> Provider {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let store = OAuthStore::with_pool(pool, "https://ax.example.com")
            .await
            .unwrap();
        Provider::new(
            store,
            Arc::new(RemoteConfig {
                public_url: "https://ax.example.com".into(),
                oauth: super::super::config::OAuthConfig {
                    built_in: true,
                    issuer: "https://ax.example.com/oauth".into(),
                    jwks_uri: "https://ax.example.com/oauth/jwks".into(),
                    clock_skew_seconds: super::super::config::DEFAULT_CLOCK_SKEW_SECONDS,
                },
                projects: vec![super::super::config::Project {
                    id: "project".into(),
                    root: ".".into(),
                    subjects: vec!["owner".into()],
                    write_subjects: vec![],
                    admin_subjects: vec![],
                }],
                api_keys: vec![],
                allowed_origins: vec![],
                client_address_header: Some(ClientAddressHeader::XForwardedFor),
            }),
            10_000,
            10_000,
        )
    }
    async fn json_body(response: Response) -> Value {
        serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 128 * 1024)
                .await
                .unwrap(),
        )
        .unwrap()
    }
    async fn grant(p: &Provider, client: &str, verifier: &str) -> (String, String) {
        let response = authorize(
            State(p.clone()),
            HeaderMap::new(),
            Query(Authorization {
                client_id: client.into(),
                redirect_uri: "http://127.0.0.1:8765/callback".into(),
                response_type: "code".into(),
                state: "state".into(),
                code_challenge: URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
                code_challenge_method: "S256".into(),
                scope: CONTEXT_SCOPE.into(),
                resource: p.store.origin.clone(),
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let html = String::from_utf8(
            axum::body::to_bytes(response.into_body(), 128 * 1024)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        let nonce = &html.split("/oauth/requests/").nth(1).unwrap()[..64];
        let requests = p.store.requests("project").await.unwrap();
        let id = requests.iter().find(|r| r["status"] == "pending").unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(p
            .store
            .decide("other-project", &id, "approve")
            .await
            .is_err());
        p.store.decide("project", &id, "approve").await.unwrap();
        let response = json_body(poll(State(p.clone()), Path(nonce.into())).await).await;
        let url = Url::parse(response["redirect"].as_str().unwrap()).unwrap();
        let pairs: BTreeMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(pairs["state"], "state");
        assert_eq!(pairs["iss"], "https://ax.example.com/oauth");
        (pairs["code"].clone(), id)
    }
    fn code_form(
        p: &Provider,
        client: &str,
        verifier: &str,
        code: &str,
    ) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("grant_type".into(), "authorization_code".into()),
            ("client_id".into(), client.into()),
            ("code".into(), code.into()),
            ("code_verifier".into(), verifier.into()),
            (
                "redirect_uri".into(),
                "http://127.0.0.1:8765/callback".into(),
            ),
            ("resource".into(), p.store.origin.clone()),
        ])
    }
    #[tokio::test]
    async fn approval_pkce_project_scope_refresh_replay_and_revocation_work() {
        let p = provider().await;
        let client = p.store.native_client().await.unwrap();
        assert_eq!(client, p.store.native_client().await.unwrap());
        let verifier = "v".repeat(64);
        let (code, id) = grant(&p, &client, &verifier).await;
        assert_eq!(
            token(
                State(p.clone()),
                Form(code_form(&p, &client, &"x".repeat(64), &code))
            )
            .await
            .status(),
            StatusCode::BAD_REQUEST
        );
        let issued = json_body(
            token(
                State(p.clone()),
                Form(code_form(&p, &client, &verifier, &code)),
            )
            .await,
        )
        .await;
        let access = issued["access_token"].as_str().unwrap();
        let refresh = issued["refresh_token"].as_str().unwrap();
        let principal = p.store.verify(access).await.unwrap();
        assert_eq!(principal.key_projects, vec!["project"]);
        assert!(!principal.key_write);
        let stored: String = sqlx::query_scalar("SELECT access_hash FROM remote_oauth_tokens")
            .fetch_one(&p.store.pool)
            .await
            .unwrap();
        assert_eq!(stored, hash(access));
        assert_ne!(stored, access);
        assert_eq!(
            token(
                State(p.clone()),
                Form(code_form(&p, &client, &verifier, &code))
            )
            .await
            .status(),
            StatusCode::BAD_REQUEST
        );
        let form = BTreeMap::from([
            ("grant_type".into(), "refresh_token".into()),
            ("client_id".into(), client.clone()),
            ("refresh_token".into(), refresh.into()),
            ("resource".into(), p.store.origin.clone()),
        ]);
        let rotated = json_body(token(State(p.clone()), Form(form.clone())).await).await;
        let next = rotated["access_token"].as_str().unwrap();
        assert!(p.store.verify(next).await.is_ok());
        assert!(p.store.verify(access).await.is_err());
        assert_eq!(
            token(State(p.clone()), Form(form)).await.status(),
            StatusCode::BAD_REQUEST
        );
        assert!(p.store.verify(next).await.is_err());
        let (_, new_id) = grant(&p, &client, &verifier).await;
        p.store.decide("project", &new_id, "revoke").await.unwrap();
        assert_ne!(id, new_id);
    }
    #[tokio::test]
    async fn inactive_connections_are_scoped_removed_and_cannot_refresh() {
        let p = provider().await;
        let client = p.store.native_client().await.unwrap();
        let verifier = "v".repeat(48);
        let (code, id) = grant(&p, &client, &verifier).await;
        let issued = json_body(
            token(
                State(p.clone()),
                Form(code_form(&p, &client, &verifier, &code)),
            )
            .await,
        )
        .await;
        let access = issued["access_token"].as_str().unwrap();
        assert!(p.store.decide("project", &id, "remove").await.is_err());
        assert!(p.store.verify(access).await.is_ok());
        p.store.decide("project", &id, "revoke").await.unwrap();
        assert!(p
            .store
            .decide("another-project", &id, "remove")
            .await
            .is_err());
        p.store.decide("project", &id, "remove").await.unwrap();
        assert!(p.store.verify(access).await.is_err());
        assert!(p.store.requests("project").await.unwrap().is_empty());
        let refresh = BTreeMap::from([
            ("grant_type".into(), "refresh_token".into()),
            ("client_id".into(), client.clone()),
            (
                "refresh_token".into(),
                issued["refresh_token"].as_str().unwrap().into(),
            ),
            ("resource".into(), p.store.origin.clone()),
        ]);
        assert_eq!(
            token(State(p.clone()), Form(refresh)).await.status(),
            StatusCode::BAD_REQUEST
        );
        let tokens: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM remote_oauth_tokens WHERE request_id=?")
                .bind(&id)
                .fetch_one(&p.store.pool)
                .await
                .unwrap();
        assert_eq!(tokens, 0);
        let verifier = "w".repeat(48);
        let (code, expired) = grant(&p, &client, &verifier).await;
        assert_eq!(
            token(
                State(p.clone()),
                Form(code_form(&p, &client, &verifier, &code))
            )
            .await
            .status(),
            StatusCode::OK
        );
        sqlx::query(
            "UPDATE remote_oauth_tokens SET expires=0,refresh_expires=0 WHERE request_id=?",
        )
        .bind(&expired)
        .execute(&p.store.pool)
        .await
        .unwrap();
        assert_eq!(
            p.store.requests("project").await.unwrap()[0]["status"],
            "expired"
        );
        p.store.decide("project", &expired, "remove").await.unwrap();
        assert!(p.store.requests("project").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn authorize_rejects_forged_state_and_invalid_pkce() {
        let p = provider().await;
        let client = p.store.native_client().await.unwrap();
        let base = Authorization {
            client_id: client,
            redirect_uri: "http://127.0.0.1:8765/callback".into(),
            response_type: "code".into(),
            state: "legit-state".into(),
            code_challenge: URL_SAFE_NO_PAD.encode([0; 32]),
            code_challenge_method: "S256".into(),
            scope: CONTEXT_SCOPE.into(),
            resource: p.store.origin.clone(),
        };
        for (label, q) in [
            ("empty state", {
                let mut q = base.clone();
                q.state = String::new();
                q
            }),
            ("oversized state", {
                let mut q = base.clone();
                q.state = "x".repeat(2049);
                q
            }),
            ("invalid challenge length", {
                let mut q = base.clone();
                q.code_challenge = "short".into();
                q
            }),
        ] {
            assert_eq!(
                authorize(State(p.clone()), HeaderMap::new(), Query(q))
                    .await
                    .status(),
                StatusCode::BAD_REQUEST,
                "{label}"
            );
        }
    }

    #[tokio::test]
    async fn public_registration_rejects_unsafe_redirects_and_unknown_resources() {
        let p = provider().await;
        for uri in [
            "http://example.com/callback",
            "https://user:secret@example.com/callback",
            "https://example.com/callback#fragment",
        ] {
            assert_eq!(
                register(
                    State(p.clone()),
                    HeaderMap::new(),
                    Json(json!({"redirect_uris":[uri]}))
                )
                .await
                .status(),
                StatusCode::BAD_REQUEST
            );
        }
        let response=register(State(p.clone()), HeaderMap::new(),Json(json!({"redirect_uris":["http://[::1]:8123/callback","https://client.example.com/callback"],"client_name":"Client","token_endpoint_auth_method":"none"}))).await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let client = json_body(response).await;
        assert!(client.get("client_secret").is_none());
        let q = Authorization {
            client_id: client["client_id"].as_str().unwrap().into(),
            redirect_uri: "https://client.example.com/other".into(),
            response_type: "code".into(),
            state: "s".into(),
            code_challenge: URL_SAFE_NO_PAD.encode([0; 32]),
            code_challenge_method: "S256".into(),
            scope: CONTEXT_SCOPE.into(),
            resource: "https://unrelated.example.com".into(),
        };
        assert_eq!(
            authorize(State(p.clone()), HeaderMap::new(), Query(q))
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
        let metadata = json_body(metadata(State(p.clone())).await).await;
        assert_eq!(
            metadata["code_challenge_methods_supported"],
            json!(["S256"])
        );
    }

    fn sign_in(client: &str, redirect: &str, state: &str) -> Authorization {
        Authorization {
            client_id: client.into(),
            redirect_uri: redirect.into(),
            response_type: "code".into(),
            state: state.into(),
            code_challenge: URL_SAFE_NO_PAD.encode([7; 32]),
            code_challenge_method: "S256".into(),
            scope: CONTEXT_SCOPE.into(),
            resource: "https://ax.example.com".into(),
        }
    }
    async fn pending(p: &Provider, client: Option<&str>) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM remote_oauth_requests WHERE status='pending' AND (? IS NULL OR client_id=?)")
            .bind(client)
            .bind(client)
            .fetch_one(&p.store.pool)
            .await
            .unwrap()
    }
    const NATIVE_REDIRECT: &str = "http://127.0.0.1:8765/callback";

    #[tokio::test]
    async fn a_full_pending_table_evicts_the_oldest_request_instead_of_refusing() {
        let p = provider().await;
        let native = p.store.native_client().await.unwrap();
        for i in 0..MAX_PENDING {
            let q = sign_in(&native, NATIVE_REDIRECT, &format!("flood-{i}"));
            assert_eq!(
                authorize(State(p.clone()), HeaderMap::new(), Query(q))
                    .await
                    .status(),
                StatusCode::OK
            );
        }
        let q = sign_in(&native, NATIVE_REDIRECT, "real-user");
        assert_eq!(
            authorize(State(p.clone()), HeaderMap::new(), Query(q))
                .await
                .status(),
            StatusCode::OK,
            "a real sign-in was refused because the pending table was full"
        );
        assert_eq!(pending(&p, None).await, MAX_PENDING);
        let oldest: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM remote_oauth_requests WHERE state='flood-0'")
                .fetch_one(&p.store.pool)
                .await
                .unwrap();
        assert_eq!(
            oldest, 0,
            "the oldest pending request was not the one evicted"
        );
    }

    #[tokio::test]
    async fn one_registered_client_cannot_hold_more_than_its_share_of_pending_requests() {
        let p = provider().await;
        let client = json_body(
            register(
                State(p.clone()),
                HeaderMap::new(),
                Json(json!({"redirect_uris":["https://client.example.com/cb"]})),
            )
            .await,
        )
        .await["client_id"]
            .as_str()
            .unwrap()
            .to_owned();
        for i in 0..MAX_PENDING_PER_CLIENT + 5 {
            let q = sign_in(&client, "https://client.example.com/cb", &format!("s{i}"));
            assert_eq!(
                authorize(State(p.clone()), HeaderMap::new(), Query(q))
                    .await
                    .status(),
                StatusCode::OK
            );
        }
        assert_eq!(pending(&p, Some(&client)).await, MAX_PENDING_PER_CLIENT);
    }

    #[tokio::test]
    async fn a_full_client_table_makes_room_by_removing_an_unused_client() {
        let p = provider().await;
        p.store.native_client().await.unwrap();
        for i in 1..MAX_CLIENTS {
            sqlx::query("INSERT INTO remote_oauth_clients VALUES (?,?,?,?,0,?)")
                .bind(&p.store.origin)
                .bind(format!("flood-{i}"))
                .bind("Flood")
                .bind("[]")
                .bind(now() - 60 + i)
                .execute(&p.store.pool)
                .await
                .unwrap();
        }
        let response = register(
            State(p.clone()),
            HeaderMap::new(),
            Json(json!({"redirect_uris":["https://client.example.com/cb"]})),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::CREATED,
            "registration was refused while unused clients filled the table"
        );
        let first: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM remote_oauth_clients WHERE id='flood-1'")
                .fetch_one(&p.store.pool)
                .await
                .unwrap();
        assert_eq!(first, 0, "the oldest unused client was not the one removed");
        let native: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM remote_oauth_clients WHERE native=1")
                .fetch_one(&p.store.pool)
                .await
                .unwrap();
        assert_eq!(native, 1, "the native client was removed");
    }

    #[tokio::test]
    async fn unauthenticated_oauth_endpoints_are_rate_limited() {
        let mut p = provider().await;
        p.authorize_limit = Arc::new(RateLimit::new(3, 10_000));
        p.register_limit = Arc::new(RateLimit::new(2, 10_000));
        let native = p.store.native_client().await.unwrap();
        for i in 0..3 {
            let q = sign_in(&native, NATIVE_REDIRECT, &format!("s{i}"));
            assert_eq!(
                authorize(State(p.clone()), HeaderMap::new(), Query(q))
                    .await
                    .status(),
                StatusCode::OK
            );
        }
        let limited = authorize(
            State(p.clone()),
            HeaderMap::new(),
            Query(sign_in(&native, NATIVE_REDIRECT, "s4")),
        )
        .await;
        assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(limited.headers().contains_key("retry-after"));
        let body = || Json(json!({"redirect_uris":["https://client.example.com/cb"]}));
        for _ in 0..2 {
            assert_eq!(
                register(State(p.clone()), HeaderMap::new(), body())
                    .await
                    .status(),
                StatusCode::CREATED
            );
        }
        assert_eq!(
            register(State(p.clone()), HeaderMap::new(), body())
                .await
                .status(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }

    /// Headers a proxy in front of Ax sends for a caller at `ip`; the first
    /// entry is what the caller itself claimed.
    fn from(ip: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_str(&format!("203.0.113.9, {ip}")).unwrap(),
        );
        headers
    }

    #[test]
    fn the_source_is_the_address_the_configured_proxy_header_names() {
        let forwarded = Some(ClientAddressHeader::XForwardedFor);
        let cloudflare_only = Some(ClientAddressHeader::CfConnectingIp);
        assert_eq!(source(&from("198.51.100.1"), forwarded), "198.51.100.1");
        let mut cloudflare = from("198.51.100.1");
        cloudflare.insert("cf-connecting-ip", HeaderValue::from_static("2001:db8::7"));
        assert_eq!(source(&cloudflare, cloudflare_only), "2001:db8::7");
        // Each setting reads only its own header.
        assert_eq!(source(&cloudflare, forwarded), "198.51.100.1");
        assert_eq!(
            source(&from("198.51.100.1"), cloudflare_only),
            UNKNOWN_SOURCE
        );
        let mut garbage = HeaderMap::new();
        garbage.insert("x-forwarded-for", HeaderValue::from_static("not-an-ip"));
        assert_eq!(source(&garbage, forwarded), UNKNOWN_SOURCE);
        assert_eq!(source(&HeaderMap::new(), forwarded), UNKNOWN_SOURCE);
    }

    #[test]
    fn without_a_configured_proxy_header_every_caller_shares_one_source() {
        let mut spoofed = from("198.51.100.1");
        spoofed.insert("cf-connecting-ip", HeaderValue::from_static("2001:db8::7"));
        assert_eq!(source(&spoofed, None), UNKNOWN_SOURCE);
    }

    #[tokio::test]
    async fn spoofed_addresses_still_meet_the_host_wide_limit() {
        let mut p = provider().await;
        p.authorize_limit = Arc::new(RateLimit::new(3, 10));
        let native = p.store.native_client().await.unwrap();
        let q = sign_in(&native, NATIVE_REDIRECT, "real-user");
        assert_eq!(
            authorize(State(p.clone()), from("198.51.100.2"), Query(q))
                .await
                .status(),
            StatusCode::OK
        );
        let mut refused = 0;
        for i in 0..(MAX_PENDING + 20) {
            // Every call claims a new address.
            let address = format!("2001:db8::{i:x}");
            let q = sign_in(&native, NATIVE_REDIRECT, &format!("flood-{i}"));
            let response = authorize(State(p.clone()), from(&address), Query(q)).await;
            if response.status() == StatusCode::TOO_MANY_REQUESTS {
                assert!(response.headers().contains_key("retry-after"));
                refused += 1;
            }
        }
        assert_eq!(
            refused,
            MAX_PENDING + 20 - 9,
            "the host-wide limit let calls through"
        );
        let kept: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM remote_oauth_requests WHERE state='real-user' AND status='pending'",
        )
        .fetch_one(&p.store.pool)
        .await
        .unwrap();
        assert_eq!(
            kept, 1,
            "a flood of spoofed addresses evicted a waiting sign-in"
        );
    }

    #[test]
    fn a_full_address_table_counts_new_addresses_in_one_shared_window() {
        let limit = RateLimit::new(3, u32::MAX);
        for i in 0..MAX_RATE_SOURCES {
            assert!(limit.check(&format!("source-{i}")).is_ok());
        }
        for _ in 0..3 {
            assert!(limit.check("late-a").is_ok());
        }
        assert!(
            limit.check("late-a").is_err(),
            "a late address went uncounted"
        );
        assert!(
            limit.check("late-b").is_err(),
            "late addresses do not share one window"
        );
        // An address already tracked keeps its own window.
        assert!(limit.check("source-0").is_ok());
    }

    #[tokio::test]
    async fn a_flood_from_one_source_does_not_refuse_another_source() {
        let mut p = provider().await;
        p.authorize_limit = Arc::new(RateLimit::new(3, 10_000));
        p.register_limit = Arc::new(RateLimit::new(2, 10_000));
        let native = p.store.native_client().await.unwrap();
        let (flood, user) = ("198.51.100.1", "198.51.100.2");
        for i in 0..4 {
            let q = sign_in(&native, NATIVE_REDIRECT, &format!("flood-{i}"));
            authorize(State(p.clone()), from(flood), Query(q)).await;
        }
        let q = sign_in(&native, NATIVE_REDIRECT, "flood-more");
        assert_eq!(
            authorize(State(p.clone()), from(flood), Query(q))
                .await
                .status(),
            StatusCode::TOO_MANY_REQUESTS
        );
        let q = sign_in(&native, NATIVE_REDIRECT, "real-user");
        assert_eq!(
            authorize(State(p.clone()), from(user), Query(q))
                .await
                .status(),
            StatusCode::OK,
            "another source's sign-in was refused by one source's flood"
        );
        let body = || Json(json!({"redirect_uris":["https://client.example.com/cb"]}));
        for _ in 0..3 {
            register(State(p.clone()), from(flood), body()).await;
        }
        assert_eq!(
            register(State(p.clone()), from(flood), body())
                .await
                .status(),
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            register(State(p.clone()), from(user), body())
                .await
                .status(),
            StatusCode::CREATED,
            "another source's registration was refused by one source's flood"
        );
    }

    #[tokio::test]
    async fn one_source_cannot_push_out_another_sources_pending_sign_in() {
        let p = provider().await;
        let native = p.store.native_client().await.unwrap();
        let q = sign_in(&native, NATIVE_REDIRECT, "real-user");
        assert_eq!(
            authorize(State(p.clone()), from("198.51.100.2"), Query(q))
                .await
                .status(),
            StatusCode::OK
        );
        for i in 0..MAX_PENDING {
            let q = sign_in(&native, NATIVE_REDIRECT, &format!("flood-{i}"));
            assert_eq!(
                authorize(State(p.clone()), from("198.51.100.1"), Query(q))
                    .await
                    .status(),
                StatusCode::OK,
                "a sign-in was refused instead of replacing the source's oldest"
            );
        }
        let kept: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM remote_oauth_requests WHERE state='real-user' AND status='pending'",
        )
        .fetch_one(&p.store.pool)
        .await
        .unwrap();
        assert_eq!(
            kept, 1,
            "one source's flood evicted another source's sign-in"
        );
        let flood: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM remote_oauth_requests WHERE source='198.51.100.1' AND status='pending'",
        )
        .fetch_one(&p.store.pool)
        .await
        .unwrap();
        assert_eq!(flood, MAX_PENDING_PER_SOURCE);
    }

    #[tokio::test]
    async fn an_older_request_table_gets_the_source_column() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE remote_oauth_requests(origin TEXT NOT NULL,id TEXT PRIMARY KEY,nonce_hash TEXT NOT NULL UNIQUE,client_id TEXT NOT NULL,redirect TEXT NOT NULL,state TEXT NOT NULL,challenge TEXT NOT NULL,scope TEXT NOT NULL,project TEXT NOT NULL,resource TEXT NOT NULL,expires INTEGER NOT NULL,status TEXT NOT NULL DEFAULT 'pending',code_hash TEXT UNIQUE,revoked INTEGER NOT NULL DEFAULT 0)")
            .execute(&pool)
            .await
            .unwrap();
        OAuthStore::with_pool(pool.clone(), "https://ax.example.com")
            .await
            .unwrap();
        // Opening again must not try to add the column twice.
        OAuthStore::with_pool(pool.clone(), "https://ax.example.com")
            .await
            .unwrap();
        let columns: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_table_info('remote_oauth_requests') WHERE name='source'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(columns, 1);
    }

    #[tokio::test]
    async fn token_errors_use_rfc_codes_and_hide_database_details() {
        let p = provider().await;
        let client = p.store.native_client().await.unwrap();
        let form = |pairs: &[(&str, &str)]| {
            Form(
                pairs
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect::<BTreeMap<_, _>>(),
            )
        };
        let unsupported = token(
            State(p.clone()),
            form(&[("grant_type", "password"), ("client_id", &client)]),
        )
        .await;
        assert_eq!(unsupported.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            json_body(unsupported).await["error"],
            "unsupported_grant_type"
        );
        let unknown_code = token(
            State(p.clone()),
            form(&[
                ("grant_type", "authorization_code"),
                ("client_id", &client),
                ("code", "nope"),
            ]),
        )
        .await;
        assert_eq!(unknown_code.status(), StatusCode::BAD_REQUEST);
        assert_eq!(json_body(unknown_code).await["error"], "invalid_grant");

        // A database failure is a server error, not a rejected grant, and the
        // driver's message never reaches the anonymous caller.
        p.store.pool.close().await;
        let failed = token(
            State(p.clone()),
            form(&[
                ("grant_type", "refresh_token"),
                ("client_id", &client),
                ("refresh_token", "still-valid"),
            ]),
        )
        .await;
        assert_eq!(failed.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = json_body(failed).await;
        assert_eq!(body["error"], "server_error");
        assert!(!body.to_string().contains("pool"), "{body}");
        let registered = register(
            State(p.clone()),
            HeaderMap::new(),
            Json(json!({"redirect_uris":["https://client.example.com/cb"]})),
        )
        .await;
        assert_eq!(registered.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = json_body(registered).await;
        assert_eq!(body["error"], "server_error");
        assert!(!body.to_string().contains("pool"), "{body}");
    }
}
