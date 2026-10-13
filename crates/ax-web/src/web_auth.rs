//! Optional human authentication for Command Center. Client/MCP/replica principals
//! remain separate from short-lived browser sessions.
use ax_global_db::{access, global_db_path, open_and_init};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{ConnectInfo, Path, Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::Deserialize;
use sqlx::SqlitePool;
use std::{collections::HashMap, net::SocketAddr, sync::OnceLock, time::{Duration, Instant}};
use tokio::sync::Mutex;
use uuid::Uuid;
use webauthn_rs::prelude::*;
use crate::workspace_state::WebHub;

type ErrorResponse = (StatusCode, Json<serde_json::Value>);
fn err(status: StatusCode, reason: &'static str) -> ErrorResponse {
    (status, Json(serde_json::json!({"error":reason})))
}
fn hash(s: &str) -> String { blake3::hash(s.as_bytes()).to_hex().to_string() }
fn fresh_secret() -> String { format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()) }
fn cookie_value(headers: &HeaderMap) -> Option<&str> {
    headers.get(header::COOKIE)?.to_str().ok()?.split(';')
        .find_map(|part| part.trim().strip_prefix("ax_web_session="))
}
fn local_request(headers: &HeaderMap, peer: SocketAddr, port: u16) -> bool {
    if !peer.ip().is_loopback() { return false; }
    if headers.contains_key("forwarded") || headers.contains_key("x-forwarded-for")
        || headers.contains_key("x-forwarded-host") { return false; }
    let host = headers.get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or("");
    let allowed = [format!("localhost:{port}"), format!("127.0.0.1:{port}")];
    if !allowed.iter().any(|h| h == host) { return false; }
    let origin = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok());
    if origin.is_some_and(|o| !allowed.iter().any(|h| o == format!("http://{h}"))) { return false; }
    headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) != Some("cross-site")
}
async fn db() -> Result<SqlitePool, ErrorResponse> {
    let path = global_db_path().map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR,"Database unavailable"))?;
    open_and_init(&path).await.map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR,"Database unavailable"))
}
async fn enabled(pool: &SqlitePool) -> Result<bool, ErrorResponse> {
    sqlx::query_scalar::<_, i64>("SELECT enabled FROM web_auth_settings WHERE singleton=1")
        .fetch_optional(pool).await
        .map(|v| v.unwrap_or(0) != 0)
        .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Authentication configuration unavailable"))
}
pub(crate) async fn web_login_enabled(pool: &SqlitePool) -> bool {
    // A database error must never silently disable protection.
    enabled(pool).await.unwrap_or(true)
}
pub(crate) async fn session_member(pool: &SqlitePool, headers: &HeaderMap)
    -> Option<(String, access::MemberRole)> {
    let id=member_for_session(pool,headers).await?;
    let members=access::list_members(pool).await.ok()?;
    let role=members.into_iter().find(|m|m.id==id && m.status=="active")?.role;
    Some((id,role))
}
async fn member_for_session(pool: &SqlitePool, headers: &HeaderMap) -> Option<String> {
    let token = cookie_value(headers)?;
    if token.len() != 64 || !token.bytes().all(|c| c.is_ascii_hexdigit()) { return None; }
    let member_id: Option<(String,)> = sqlx::query_as(
        "SELECT s.member_id FROM web_sessions s JOIN team_members m ON m.id=s.member_id
         WHERE s.token_hash=? AND s.revoked_at IS NULL AND m.status='active'
         AND s.expires_at > unixepoch() AND s.last_seen_at > unixepoch()-1800"
    ).bind(hash(token)).fetch_optional(pool).await.ok()?;
    let id = member_id?.0;
    let _ = sqlx::query("UPDATE web_sessions SET last_seen_at=unixepoch() WHERE token_hash=?")
        .bind(hash(token)).execute(pool).await;
    Some(id)
}
fn trusted_local_mode(hub: &WebHub) -> bool {
    // A TLS/public canonical origin can be proxied into a loopback listener.
    // That cannot act as proof of local Owner identity before sign-in.
    hub.local_only && std::env::var("AX_WEB_AUTH_ORIGIN").ok()
        .is_none_or(|configured| configured.starts_with("http://localhost:"))
}
async fn browser_member(pool: &SqlitePool, headers: &HeaderMap, peer: SocketAddr, hub: &WebHub)
    -> Result<String, ErrorResponse> {
    if let Some(id) = member_for_session(pool, headers).await { return Ok(id); }
    if !enabled(pool).await? && trusted_local_mode(hub) && local_request(headers, peer, hub.port) {
        return access::list_members(pool).await
            .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Members unavailable"))?
            .into_iter().find(|m| m.role == access::MemberRole::Owner && m.status == "active")
            .map(|m|m.id)
            .ok_or_else(||err(StatusCode::FORBIDDEN,"Owner missing"));
    }
    Err(err(StatusCode::UNAUTHORIZED,"Sign in required"))
}
async fn require_owner(pool: &SqlitePool, member_id: &str) -> Result<(), ErrorResponse> {
    let owner: Option<(String,)> = sqlx::query_as(
        "SELECT role FROM team_members WHERE id=? AND status='active'"
    ).bind(member_id).fetch_optional(pool).await
        .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Members unavailable"))?;
    if owner.is_some_and(|(role,)|role=="owner") { Ok(()) }
    else { Err(err(StatusCode::FORBIDDEN,"Owner role required")) }
}
fn session_cookie(token: &str, secure: bool) -> String {
    format!("ax_web_session={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=28800{}",
        if secure {"; Secure"} else {""})
}
fn clear_cookie(secure: bool) -> String {
    format!("ax_web_session=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0{}",
        if secure {"; Secure"} else {""})
}
fn cookie_response(body: serde_json::Value, cookie: String) -> Response {
    let mut response = Json(body).into_response();
    response.headers_mut().insert(header::SET_COOKIE, HeaderValue::from_str(&cookie).expect("valid cookie"));
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
async fn establish(pool: &SqlitePool, member_id: &str, headers: &HeaderMap, secure: bool) -> Result<Response,ErrorResponse> {
    if let Some(existing)=cookie_value(headers) {
        let _ = sqlx::query("UPDATE web_sessions SET revoked_at=unixepoch() WHERE token_hash=?")
            .bind(hash(existing)).execute(pool).await;
    }
    let token = fresh_secret();
    sqlx::query("INSERT INTO web_sessions (token_hash,member_id,expires_at) VALUES (?,?,unixepoch()+28800)")
        .bind(hash(&token)).bind(member_id).execute(pool).await
        .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Cannot establish session"))?;
    Ok(cookie_response(serde_json::json!({"authenticated":true}),session_cookie(&token,secure)))
}
fn origin() -> Result<Url,ErrorResponse> {
    let value=std::env::var("AX_WEB_AUTH_ORIGIN").unwrap_or_else(|_|"http://localhost:7070".to_string());
    let url=Url::parse(&value).map_err(|_|err(StatusCode::BAD_REQUEST,"Invalid AX_WEB_AUTH_ORIGIN"))?;
    let host=url.host_str().unwrap_or("");
    if host.is_empty() || !(url.scheme()=="https" || (url.scheme()=="http" && host=="localhost")) {
        return Err(err(StatusCode::BAD_REQUEST,"Passkeys require HTTPS or localhost"));
    }
    Ok(url)
}
fn webauthn() -> Result<Webauthn,ErrorResponse> {
    let url=origin()?;
    let rp_id=url.host_str().ok_or_else(||err(StatusCode::BAD_REQUEST,"RP ID missing"))?;
    WebauthnBuilder::new(rp_id,&url)
        .and_then(|builder|builder.build())
        .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"WebAuthn configuration invalid"))
}
fn secure_origin() -> bool { origin().is_ok_and(|u| u.scheme()=="https") }
fn passkey_ceremonies() -> &'static Mutex<HashMap<String,(Instant,Ceremony)>> {
    static STORE: OnceLock<Mutex<HashMap<String,(Instant,Ceremony)>>> = OnceLock::new();
    STORE.get_or_init(||Mutex::new(HashMap::new()))
}
fn login_attempts() -> &'static Mutex<HashMap<String, (u32, Instant)>> {
    static STORE: OnceLock<Mutex<HashMap<String, (u32, Instant)>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}
fn dummy_password_hash() -> &'static str {
    static DUMMY: OnceLock<String> = OnceLock::new();
    DUMMY.get_or_init(|| {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default().hash_password(b"dummy-unusable-password", &salt)
            .expect("argon2 dummy hash").to_string()
    })
}
enum Ceremony {
    Register { member_id: String, state: PasskeyRegistration },
    Authenticate { member_id: String, state: PasskeyAuthentication },
}
async fn store_challenge(ceremony: Ceremony) -> String {
    let id=fresh_secret();
    let mut map=passkey_ceremonies().lock().await;
    map.retain(|_,(created,_)|created.elapsed()<Duration::from_secs(180));
    if map.len() >= 1000 { map.clear(); }
    map.insert(id.clone(),(Instant::now(),ceremony));
    id
}
async fn consume_challenge(id: &str) -> Option<Ceremony> {
    let (when,state)=passkey_ceremonies().lock().await.remove(id)?;
    (when.elapsed()<Duration::from_secs(180)).then_some(state)
}
#[derive(Deserialize)]
struct PasswordBody { identity: String, password: String }
#[derive(Deserialize)]
struct SetupBody { password: String }
#[derive(Deserialize)]
struct ConfigBody { enabled: bool }
#[derive(Deserialize)]
struct CeremonyBody { ceremony: String, credential: serde_json::Value }
#[derive(Deserialize)]
struct StartBody { identity: String }
#[derive(Deserialize)]
struct PasskeyLabel { label: String }

async fn status() -> Result<Json<serde_json::Value>,ErrorResponse> {
    let pool=db().await?;
    Ok(Json(serde_json::json!({"enabled":enabled(&pool).await?,"passkeys":true})))
}
async fn session(headers: HeaderMap) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let pool=db().await?;
    if !enabled(&pool).await? {
        return Ok(Json(serde_json::json!({"enabled":false,"authenticated":false})));
    }
    let id=member_for_session(&pool,&headers).await.ok_or_else(||err(StatusCode::UNAUTHORIZED,"Sign in required"))?;
    let user=access::list_members(&pool).await
        .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Members unavailable"))?
        .into_iter().find(|m|m.id==id).ok_or_else(||err(StatusCode::UNAUTHORIZED,"Invalid session"))?;
    Ok(Json(serde_json::json!({"authenticated":true,"member":user})))
}
async fn setup(State(hub):State<WebHub>,ConnectInfo(peer):ConnectInfo<SocketAddr>,
    headers:HeaderMap,Json(body):Json<SetupBody>) -> Result<Json<serde_json::Value>,ErrorResponse> {
    let pool=db().await?;
    if enabled(&pool).await? {return Err(err(StatusCode::FORBIDDEN,"Already enabled"));}
    if hub.readonly || !trusted_local_mode(&hub) || !local_request(&headers,peer,hub.port) {return Err(err(StatusCode::FORBIDDEN,"Local Owner required"));}
    if body.password.len()<12 || body.password.len()>1024 {return Err(err(StatusCode::BAD_REQUEST,"Password must be 12-1024 characters"));}
    let owner=access::list_members(&pool).await
        .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Members unavailable"))?
        .into_iter().find(|m|m.role==access::MemberRole::Owner && m.status=="active")
        .ok_or_else(||err(StatusCode::FORBIDDEN,"Owner required"))?;
    let salt=SaltString::generate(&mut OsRng);
    let verifier=Argon2::default().hash_password(body.password.as_bytes(),&salt)
        .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Password setup failed"))?.to_string();
    sqlx::query("INSERT INTO web_password_credentials(member_id,password_hash) VALUES (?,?)
                 ON CONFLICT(member_id) DO UPDATE SET password_hash=excluded.password_hash")
        .bind(&owner.id).bind(verifier).execute(&pool).await
        .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Password setup failed"))?;
    Ok(Json(serde_json::json!({"configured":true,"member_id":owner.id})))
}
async fn configure(State(hub):State<WebHub>,ConnectInfo(peer):ConnectInfo<SocketAddr>,
    headers:HeaderMap,Json(body):Json<ConfigBody>) -> Result<Json<serde_json::Value>,ErrorResponse> {
    let pool=db().await?;
    if hub.readonly {return Err(err(StatusCode::FORBIDDEN,"Read-only")); }
    let current=enabled(&pool).await?;
    let actor=browser_member(&pool,&headers,peer,&hub).await?;
    require_owner(&pool,&actor).await?;
    if !hub.local_only || !local_request(&headers,peer,hub.port) {
        return Err(err(StatusCode::FORBIDDEN,"Change authentication only at local Command Center"));
    }
    if body.enabled {
        origin()?;
        let count: (i64,) =sqlx::query_as(
            "SELECT COUNT(*) FROM web_password_credentials w JOIN team_members m ON m.id=w.member_id
             WHERE m.role='owner' AND m.status='active'"
        ).fetch_one(&pool).await
            .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Owner credentials unavailable"))?;
        let keys: (i64,) =sqlx::query_as(
            "SELECT COUNT(*) FROM web_passkeys p JOIN team_members m ON m.id=p.member_id
             WHERE m.role='owner' AND m.status='active' AND p.revoked_at IS NULL"
        ).fetch_one(&pool).await
            .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Owner passkeys unavailable"))?;
        if count.0 + keys.0==0 {return Err(err(StatusCode::CONFLICT,"Enroll an Owner sign-in method first"));}
    }
    if current && !body.enabled {
        // A fresh full authentication is required to disable the guard.
        let recent: Option<(i64,)> = sqlx::query_as(
            "SELECT 1 FROM web_sessions WHERE token_hash=? AND authenticated_at>=unixepoch()-300
             AND revoked_at IS NULL"
        ).bind(hash(cookie_value(&headers).unwrap_or(""))).fetch_optional(&pool).await
            .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Session unavailable"))?;
        if recent.is_none() {return Err(err(StatusCode::FORBIDDEN,"Reauthenticate before disabling login"));}
    }
    sqlx::query("INSERT INTO web_auth_settings(singleton,enabled) VALUES (1,?)
                 ON CONFLICT(singleton) DO UPDATE SET enabled=excluded.enabled")
        .bind(body.enabled).execute(&pool).await
        .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Cannot update authentication"))?;
    sqlx::query("UPDATE web_sessions SET revoked_at=unixepoch() WHERE revoked_at IS NULL")
        .execute(&pool).await
        .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Cannot revoke sessions"))?;
    let _=access::audit(&pool,Some(&actor),None,"web_auth.config_updated","installation","1",&serde_json::json!({"enabled":body.enabled})).await;
    Ok(Json(serde_json::json!({"enabled":body.enabled})))
}
async fn password_login(ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers:HeaderMap,Json(body):Json<PasswordBody>) -> Result<Response,ErrorResponse> {
    let pool=db().await?;
    if !enabled(&pool).await? {return Err(err(StatusCode::CONFLICT,"Login disabled"));}
    let identity=body.identity.trim();
    if identity.is_empty() || body.password.len()>1024 {return Err(err(StatusCode::UNAUTHORIZED,"Invalid credentials"));}
    let limit_key = format!("{}:{}",peer.ip(),hash(identity));
    {
        let attempts=login_attempts().lock().await;
        if attempts.get(&limit_key).is_some_and(|(count,first)|
            *count >= 5 && first.elapsed() < Duration::from_secs(300)) {
            return Err(err(StatusCode::TOO_MANY_REQUESTS,"Too many login attempts"));
        }
    }
    let row: Option<(String,String)> = sqlx::query_as(
        "SELECT m.id, c.password_hash FROM team_members m
         JOIN web_password_credentials c ON c.member_id=m.id
         WHERE m.status='active' AND (m.id=? OR m.email=? OR m.display_name=?) LIMIT 1"
    ).bind(identity).bind(identity).bind(identity).fetch_optional(&pool).await
        .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Sign-in unavailable"))?;
    // Always run Argon2, even for unknown identities, to avoid timing
    // differences that disclose account existence.
    let digest = row.as_ref().map(|(_, hash)| hash.as_str()).unwrap_or_else(||dummy_password_hash());
    let valid = PasswordHash::new(digest).ok()
        .is_some_and(|parsed| Argon2::default().verify_password(body.password.as_bytes(),&parsed).is_ok());
    if !valid {
        let mut attempts=login_attempts().lock().await;
        let entry=attempts.entry(limit_key).or_insert((0,Instant::now()));
        if entry.1.elapsed() >= Duration::from_secs(300) { *entry=(0,Instant::now()); }
        entry.0+=1;
        return Err(err(StatusCode::UNAUTHORIZED,"Invalid credentials"));
    }
    login_attempts().lock().await.remove(&limit_key);
    establish(&pool,&row.expect("valid account has a password").0,&headers,secure_origin()).await
}
async fn user_password(State(hub):State<WebHub>,ConnectInfo(peer):ConnectInfo<SocketAddr>,
    headers:HeaderMap,Path(target):Path<String>,Json(body):Json<SetupBody>)
    -> Result<Json<serde_json::Value>,ErrorResponse> {
    let pool=db().await?;
    if hub.readonly {return Err(err(StatusCode::FORBIDDEN,"Read-only"));}
    let actor=browser_member(&pool,&headers,peer,&hub).await?;
    require_owner(&pool,&actor).await?;
    let req_origin=headers.get(header::ORIGIN).and_then(|value|value.to_str().ok());
    let trusted=local_request(&headers,peer,hub.port)
        || origin().is_ok_and(|url|req_origin==Some(url.as_str().trim_end_matches('/')));
    if !trusted {return Err(err(StatusCode::FORBIDDEN,"Untrusted request origin"));}
    if body.password.len()<12 || body.password.len()>1024 {
        return Err(err(StatusCode::BAD_REQUEST,"Password must be 12-1024 characters"));
    }
    let exists:Option<(String,)>=sqlx::query_as(
        "SELECT id FROM team_members WHERE id=? AND status='active'"
    ).bind(&target).fetch_optional(&pool).await
        .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"User lookup failed"))?;
    if exists.is_none() {return Err(err(StatusCode::NOT_FOUND,"Active user not found"));}
    let salt=SaltString::generate(&mut OsRng);
    let verifier=Argon2::default().hash_password(body.password.as_bytes(),&salt)
        .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Password setup failed"))?.to_string();
    let mut tx=pool.begin().await
        .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Password update failed"))?;
    sqlx::query("INSERT INTO web_password_credentials(member_id,password_hash) VALUES (?,?)
         ON CONFLICT(member_id) DO UPDATE SET password_hash=excluded.password_hash")
        .bind(&target).bind(verifier).execute(&mut *tx).await
        .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Password update failed"))?;
    sqlx::query("UPDATE web_sessions SET revoked_at=unixepoch() WHERE member_id=? AND revoked_at IS NULL")
        .bind(&target).execute(&mut *tx).await
        .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Sessions unavailable"))?;
    tx.commit().await.map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Password update failed"))?;
    let _=access::audit(&pool,Some(&actor),None,"web_auth.password_reset","member",&target,&serde_json::json!({})).await;
    Ok(Json(serde_json::json!({"updated":true})))
}
async fn logout(headers:HeaderMap) -> Result<Response,ErrorResponse> {
    let pool=db().await?;
    if let Some(token)=cookie_value(&headers) {
        sqlx::query("UPDATE web_sessions SET revoked_at=unixepoch() WHERE token_hash=?")
            .bind(hash(token)).execute(&pool).await
            .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Logout failed"))?;
    }
    Ok(cookie_response(serde_json::json!({"authenticated":false}),clear_cookie(secure_origin())))
}
async fn passkeys(State(hub):State<WebHub>,ConnectInfo(peer):ConnectInfo<SocketAddr>,
    headers:HeaderMap) -> Result<Json<serde_json::Value>,ErrorResponse> {
    let pool=db().await?;
    let actor=browser_member(&pool,&headers,peer,&hub).await?;
    let rows: Vec<(String,String,String,Option<i64>)> = sqlx::query_as(
        "SELECT id,label,created_at,revoked_at FROM web_passkeys WHERE member_id=? ORDER BY created_at"
    ).bind(actor).fetch_all(&pool).await
        .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Passkeys unavailable"))?;
    Ok(Json(serde_json::json!({"passkeys":rows.into_iter().map(|(id,label,created_at,revoked_at)|
        serde_json::json!({"id":id,"label":label,"created_at":created_at,"revoked":revoked_at.is_some()})).collect::<Vec<_>>()})))
}
async fn register_start(State(hub):State<WebHub>,ConnectInfo(peer):ConnectInfo<SocketAddr>,
    headers:HeaderMap) -> Result<Json<serde_json::Value>,ErrorResponse> {
    let pool=db().await?;
    let actor=browser_member(&pool,&headers,peer,&hub).await?;
    let user=access::list_members(&pool).await.map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Members unavailable"))?
        .into_iter().find(|m|m.id==actor).ok_or_else(||err(StatusCode::FORBIDDEN,"Unknown user"))?;
    let user_id=Uuid::parse_str(&actor).unwrap_or_else(|_| {
        let digest=blake3::hash(actor.as_bytes());
        Uuid::from_bytes(digest.as_bytes()[..16].try_into().expect("16 bytes"))
    });
    let (options,state)=webauthn()?.start_passkey_registration(user_id,
        user.email.as_deref().unwrap_or(&user.display_name),&user.display_name,None)
        .map_err(|_|err(StatusCode::BAD_REQUEST,"Cannot start passkey registration"))?;
    let ceremony=store_challenge(Ceremony::Register{member_id:actor,state}).await;
    Ok(Json(serde_json::json!({"ceremony":ceremony,"options":options})))
}
async fn register_finish(State(hub):State<WebHub>,ConnectInfo(peer):ConnectInfo<SocketAddr>,
    headers:HeaderMap,Json(body):Json<CeremonyBody>) -> Result<Json<serde_json::Value>,ErrorResponse> {
    let pool=db().await?;
    let actor=browser_member(&pool,&headers,peer,&hub).await?;
    let state=match consume_challenge(&body.ceremony).await {
        Some(Ceremony::Register{member_id,state}) if member_id==actor=>state,
        _=>return Err(err(StatusCode::BAD_REQUEST,"Registration expired or invalid")),
    };
    let credential: RegisterPublicKeyCredential=serde_json::from_value(body.credential)
        .map_err(|_|err(StatusCode::BAD_REQUEST,"Malformed registration"))?;
    let passkey=webauthn()?.finish_passkey_registration(&credential,&state)
        .map_err(|_|err(StatusCode::BAD_REQUEST,"Passkey registration rejected"))?;
    let id=fresh_secret();
    sqlx::query("INSERT INTO web_passkeys(id,member_id,credential_id,credential_json,label)
                 VALUES (?,?,?,?,?)")
        .bind(&id).bind(&actor).bind(URL_SAFE_NO_PAD.encode(passkey.cred_id()))
        .bind(serde_json::to_string(&passkey).map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Passkey storage failed"))?)
        .bind("Passkey").execute(&pool).await
        .map_err(|_|err(StatusCode::CONFLICT,"Passkey already registered"))?;
    let _=access::audit(&pool,Some(&actor),None,"web_auth.passkey_added","passkey",&id,&serde_json::json!({})).await;
    Ok(Json(serde_json::json!({"id":id,"created":true})))
}
async fn auth_start(Json(body):Json<StartBody>) -> Result<Json<serde_json::Value>,ErrorResponse> {
    let pool=db().await?;
    if !enabled(&pool).await? {return Err(err(StatusCode::CONFLICT,"Login disabled"));}
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM team_members WHERE status='active' AND (id=? OR email=? OR display_name=?) LIMIT 1"
    ).bind(&body.identity).bind(&body.identity).bind(&body.identity).fetch_optional(&pool).await
        .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Sign-in unavailable"))?;
    let (member_id,)=row.ok_or_else(||err(StatusCode::UNAUTHORIZED,"No available passkey"))?;
    let raw:Vec<(String,)> = sqlx::query_as(
        "SELECT credential_json FROM web_passkeys WHERE member_id=? AND revoked_at IS NULL"
    ).bind(&member_id).fetch_all(&pool).await
        .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Passkeys unavailable"))?;
    let credentials=raw.iter().filter_map(|(s,)|serde_json::from_str::<Passkey>(s).ok()).collect::<Vec<_>>();
    if credentials.is_empty() {return Err(err(StatusCode::UNAUTHORIZED,"No available passkey"));}
    let (options,state)=webauthn()?.start_passkey_authentication(&credentials)
        .map_err(|_|err(StatusCode::BAD_REQUEST,"Cannot start passkey sign-in"))?;
    let ceremony=store_challenge(Ceremony::Authenticate{member_id,state}).await;
    Ok(Json(serde_json::json!({"ceremony":ceremony,"options":options})))
}
async fn auth_finish(headers:HeaderMap,Json(body):Json<CeremonyBody>) -> Result<Response,ErrorResponse> {
    let pool=db().await?;
    if !enabled(&pool).await? {return Err(err(StatusCode::CONFLICT,"Login disabled"));}
    let (member_id,state)=match consume_challenge(&body.ceremony).await {
        Some(Ceremony::Authenticate{member_id,state})=>(member_id,state),
        _=>return Err(err(StatusCode::BAD_REQUEST,"Authentication expired or invalid")),
    };
    let credential:PublicKeyCredential=serde_json::from_value(body.credential)
        .map_err(|_|err(StatusCode::BAD_REQUEST,"Malformed assertion"))?;
    let verified=webauthn()?.finish_passkey_authentication(&credential,&state)
        .map_err(|_|err(StatusCode::UNAUTHORIZED,"Passkey assertion rejected"))?;
    if !verified.user_verified() {return Err(err(StatusCode::UNAUTHORIZED,"User verification required"));}
    let id=URL_SAFE_NO_PAD.encode(verified.cred_id());
    let stored:Option<(String,String)>=sqlx::query_as(
        "SELECT id,credential_json FROM web_passkeys WHERE credential_id=? AND member_id=? AND revoked_at IS NULL"
    ).bind(id).bind(&member_id).fetch_optional(&pool).await
        .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Credential lookup failed"))?;
    let (record_id,json)=stored.ok_or_else(||err(StatusCode::UNAUTHORIZED,"Passkey revoked"))?;
    let mut passkey:Passkey=serde_json::from_str(&json)
        .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Stored key malformed"))?;
    if passkey.update_credential(&verified).is_none() {
        return Err(err(StatusCode::UNAUTHORIZED,"Passkey identity mismatch"));
    }
    sqlx::query("UPDATE web_passkeys SET credential_json=?,last_used_at=unixepoch() WHERE id=?")
        .bind(serde_json::to_string(&passkey).map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Cannot update passkey"))?)
        .bind(record_id).execute(&pool).await
        .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Cannot update passkey"))?;
    establish(&pool,&member_id,&headers,secure_origin()).await
}
async fn remove_key(State(hub):State<WebHub>,ConnectInfo(peer):ConnectInfo<SocketAddr>,
    headers:HeaderMap,Path(id):Path<String>) -> Result<Json<serde_json::Value>,ErrorResponse> {
    let pool=db().await?;
    let actor=browser_member(&pool,&headers,peer,&hub).await?;
    // Never strand the final Owner without a remaining sign-in method.
    let owner: Option<(String,)> = sqlx::query_as("SELECT role FROM team_members WHERE id=?")
        .bind(&actor).fetch_optional(&pool).await.ok().flatten();
    if owner.is_some_and(|(role,)|role=="owner") {
        let password: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM web_password_credentials WHERE member_id=?")
            .bind(&actor).fetch_one(&pool).await.map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Credential lookup failed"))?;
        let keys: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM web_passkeys WHERE member_id=? AND revoked_at IS NULL")
            .bind(&actor).fetch_one(&pool).await.map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Credential lookup failed"))?;
        if password.0==0 && keys.0<=1 { return Err(err(StatusCode::CONFLICT,"Cannot remove final Owner login method")); }
    }
    let result=sqlx::query("UPDATE web_passkeys SET revoked_at=unixepoch()
        WHERE id=? AND member_id=? AND revoked_at IS NULL")
        .bind(&id).bind(&actor).execute(&pool).await
        .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Cannot revoke passkey"))?;
    if result.rows_affected()!=1 {return Err(err(StatusCode::NOT_FOUND,"Passkey not found"));}
    let _=access::audit(&pool,Some(&actor),None,"web_auth.passkey_revoked","passkey",&id,&serde_json::json!({})).await;
    Ok(Json(serde_json::json!({"revoked":true})))
}
async fn rename_key(State(hub):State<WebHub>,ConnectInfo(peer):ConnectInfo<SocketAddr>,
    headers:HeaderMap,Path(id):Path<String>,Json(body):Json<PasskeyLabel>) -> Result<Json<serde_json::Value>,ErrorResponse> {
    let pool=db().await?;
    let actor=browser_member(&pool,&headers,peer,&hub).await?;
    if body.label.trim().is_empty() || body.label.len()>80 {return Err(err(StatusCode::BAD_REQUEST,"Invalid label"));}
    let result=sqlx::query("UPDATE web_passkeys SET label=? WHERE id=? AND member_id=? AND revoked_at IS NULL")
        .bind(body.label.trim()).bind(id).bind(actor).execute(&pool).await
        .map_err(|_|err(StatusCode::INTERNAL_SERVER_ERROR,"Cannot rename passkey"))?;
    if result.rows_affected()!=1 {return Err(err(StatusCode::NOT_FOUND,"Passkey not found"));}
    Ok(Json(serde_json::json!({"updated":true})))
}
/// Public paths and machine principals retain their existing authentication.
fn excluded(path:&str)->bool {
    path=="/api/web-auth/status" || path=="/api/web-auth/session"
        || path=="/api/web-auth/login" || path=="/api/web-auth/logout"
        || path=="/api/web-auth/passkeys/authentication/start"
        || path=="/api/web-auth/passkeys/authentication/finish"
        || path=="/api/web-auth/setup" || path=="/api/web-auth/config"
        || path=="/api/web-auth/passkeys" || path.starts_with("/api/web-auth/passkeys/")
        || path.starts_with("/api/remote-mcp") || path.starts_with("/api/auth/")
        || path.starts_with("/mcp") || path.starts_with("/api/share")
        || path.starts_with("/dav") || path.starts_with("/api/dav")
}
pub async fn gate(State(hub):State<WebHub>,mut req:Request,next:Next)->Response {
    let pool=match db().await {
        Ok(pool)=>pool, Err((s,j))=>return (s,j).into_response(),
    };
    let active=match enabled(&pool).await {
        Ok(value)=>value,Err((s,j))=>return (s,j).into_response(),
    };
    let path=req.uri().path();
    // All JSON data routes require the gate. The static SPA shell itself can
    // load to display a login without exposing any indexed project data.
    if excluded(path) || (!path.starts_with("/api/") && path != "/api") {
        // Static SPA assets and its unauthenticated login shell contain no user data;
        // every data API remains behind the server-side gate.
        return next.run(req).await;
    }
    // Machine bearer clients keep scope checks in their dedicated resource routers.
    let bearer=req.headers().get(header::AUTHORIZATION).and_then(|v|v.to_str().ok())
        .is_some_and(|value|value.starts_with("Bearer axc_"));
    if bearer && path.starts_with("/api/access/") {return next.run(req).await;}
    if bearer && path.starts_with("/api/") && !active {return next.run(req).await;}
    let peer=req.extensions().get::<ConnectInfo<SocketAddr>>().map(|v|v.0);
    if !active {
        if trusted_local_mode(&hub) && peer.is_some_and(|addr|local_request(req.headers(),addr,hub.port)) {return next.run(req).await;}
        return err(StatusCode::FORBIDDEN,"Remote Command Center requires authentication").into_response();
    }
    let Some(member_id)=member_for_session(&pool,req.headers()).await else {
        return err(StatusCode::UNAUTHORIZED,"Sign in required").into_response();
    };
    if req.method()!=Method::GET && req.method()!=Method::HEAD {
        let expected=origin().ok();
        let req_origin=req.headers().get(header::ORIGIN).and_then(|v|v.to_str().ok());
        // Reject cross-site mutations. Require an exact Origin from modern browsers.
        // Local command center accepts either loopback spelling for non-WebAuthn endpoints.
        let local_origin = peer.is_some_and(|addr|local_request(req.headers(),addr,hub.port));
        if !local_origin && expected.as_ref().is_none_or(|url|
            req_origin!=Some(url.as_str().trim_end_matches('/'))) {
            return err(StatusCode::FORBIDDEN,"Untrusted request origin").into_response();
        }
    }
    // Coarse Command Center permissions are enforced here before resource-level ACLs.
    // All write operations require a writable human role, and management APIs
    // require Admin or Owner. Client token scopes remain independent.
    let Some((_, role))=session_member(&pool,req.headers()).await else {
        return err(StatusCode::UNAUTHORIZED,"Sign in required").into_response();
    };
    let admin_path = path.starts_with("/api/access/")
        || path.starts_with("/api/ops/")
        || path.starts_with("/api/ship/")
        || path.starts_with("/api/workspace/")
        || path.starts_with("/api/plugins/")
        || path.starts_with("/api/policy/share")
        || path.starts_with("/api/vault/")
        || path.starts_with("/api/actions/");
    if admin_path && !role.permits("admin:team") {
        return err(StatusCode::FORBIDDEN,"Administrative role required").into_response();
    }
    if req.method()!=Method::GET && req.method()!=Method::HEAD
        && !role.permits("memory:write") {
        return err(StatusCode::FORBIDDEN,"Write permission required").into_response();
    }
    req.extensions_mut().insert(member_id);
    next.run(req).await
}
pub fn router(hub:WebHub)->Router {
    Router::new()
        .route("/status",get(status))
        .route("/session",get(session))
        .route("/setup",post(setup))
        .route("/config",post(configure))
        .route("/login",post(password_login))
        .route("/logout",post(logout))
        .route("/users/{id}/password",post(user_password))
        .route("/passkeys",get(passkeys))
        .route("/passkeys/registration/start",post(register_start))
        .route("/passkeys/registration/finish",post(register_finish))
        .route("/passkeys/authentication/start",post(auth_start))
        .route("/passkeys/authentication/finish",post(auth_finish))
        .route("/passkeys/{id}",axum::routing::delete(remove_key).patch(rename_key))
        .with_state(hub)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn invalid_origins_and_cookie_parsing() {
        let mut headers=HeaderMap::new();
        headers.insert(header::HOST,"localhost:7070".parse().unwrap());
        let peer="127.0.0.1:23456".parse().unwrap();
        assert!(local_request(&headers,peer,7070));
        headers.insert("x-forwarded-for","203.0.113.7".parse().unwrap());
        assert!(!local_request(&headers,peer,7070));
        assert!(!local_request(&HeaderMap::new(),peer,7070));
        assert!(!local_request(&headers,"203.0.113.7:23456".parse().unwrap(),7070));
        assert!(session_cookie("abc",true).contains("HttpOnly"));
        assert!(session_cookie("abc",true).contains("Secure"));
    }
}
