//! Built-in public-client OAuth with PKCE and explicit local-owner approval.
use super::{auth::{Principal,CONTEXT_SCOPE,WRITE_SCOPE},config::RemoteConfig};
use axum::{extract::{State,Query,Path},response::{IntoResponse,Response,Html},http::{StatusCode,HeaderValue},routing::{get,post},Json,Form,Router};
use base64::{Engine,engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Deserialize;
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use sqlx::{SqlitePool,Row};
use std::{collections::{HashSet,BTreeMap},sync::Arc};
use url::Url;
fn hash(secret:&str)->String {hex::encode(Sha256::digest(secret.as_bytes()))}
fn secret()->String {format!("{}{}",uuid::Uuid::new_v4().simple(),uuid::Uuid::new_v4().simple())}
fn now()->i64 {ax_global_db::remote_mcp_publications::now()}
fn db(error:sqlx::Error)->String {error.to_string()}
#[derive(Clone)]
pub struct OAuthStore {pub pool:SqlitePool,pub origin:String}
impl OAuthStore {
    pub async fn open(origin:&str)->Result<Self,String> {
        let pool=ax_global_db::open_and_init(&ax_global_db::global_db_path().map_err(|e|e.to_string())?).await.map_err(|e|e.to_string())?;
        Self::with_pool(pool,origin).await
    }
    pub async fn with_pool(pool:SqlitePool,origin:&str)->Result<Self,String> {
        for query in [
            "CREATE TABLE IF NOT EXISTS remote_oauth_clients(origin TEXT NOT NULL,id TEXT NOT NULL,name TEXT NOT NULL,redirects TEXT NOT NULL,native INTEGER NOT NULL DEFAULT 0,created INTEGER NOT NULL,PRIMARY KEY(origin,id))",
            "CREATE TABLE IF NOT EXISTS remote_oauth_requests(origin TEXT NOT NULL,id TEXT PRIMARY KEY,nonce_hash TEXT NOT NULL UNIQUE,client_id TEXT NOT NULL,redirect TEXT NOT NULL,state TEXT NOT NULL,challenge TEXT NOT NULL,scope TEXT NOT NULL,project TEXT NOT NULL,resource TEXT NOT NULL,expires INTEGER NOT NULL,status TEXT NOT NULL DEFAULT 'pending',code_hash TEXT UNIQUE,revoked INTEGER NOT NULL DEFAULT 0)",
            "CREATE TABLE IF NOT EXISTS remote_oauth_tokens(origin TEXT NOT NULL,access_hash TEXT PRIMARY KEY,refresh_hash TEXT NOT NULL UNIQUE,request_id TEXT NOT NULL,client_id TEXT NOT NULL,expires INTEGER NOT NULL,refresh_expires INTEGER NOT NULL)",
            "CREATE TABLE IF NOT EXISTS remote_oauth_used_refresh(origin TEXT NOT NULL,hash TEXT NOT NULL,client_id TEXT NOT NULL,request_id TEXT NOT NULL,expires INTEGER NOT NULL,PRIMARY KEY(origin,hash))",
        ] {sqlx::query(query).execute(&pool).await.map_err(db)?;}
        Ok(Self{pool,origin:origin.into()})
    }
    pub async fn native_client(&self)->Result<String,String> {
        let id=format!("ax-native-{}",&hash(&self.origin)[..24]);
        sqlx::query("INSERT OR IGNORE INTO remote_oauth_clients VALUES (?,?,?,? ,1,?)").bind(&self.origin).bind(&id).bind("Ax native client").bind("[]").bind(now()).execute(&self.pool).await.map_err(db)?; Ok(id)
    }
    pub async fn requests(&self,project:&str)->Result<Vec<Value>,String> {
        let rows=sqlx::query("SELECT r.id,c.name,r.scope,r.status,r.expires,r.revoked,r.redirect FROM remote_oauth_requests r JOIN remote_oauth_clients c ON c.origin=r.origin AND c.id=r.client_id WHERE r.origin=? AND r.project=? AND ((r.status='pending' AND r.expires>?) OR r.status IN ('approved','delivered')) ORDER BY r.expires DESC LIMIT 100").bind(&self.origin).bind(project).bind(now()).fetch_all(&self.pool).await.map_err(db)?;
        Ok(rows.iter().map(|r|json!({"id":r.get::<String,_>("id"),"clientName":r.get::<String,_>("name"),"scope":r.get::<String,_>("scope"),"status":r.get::<String,_>("status"),"expires":r.get::<i64,_>("expires"),"revoked":r.get::<i64,_>("revoked")!=0,"redirectUri":r.get::<String,_>("redirect")})).collect())
    }
    pub async fn decide(&self,project:&str,id:&str,decision:&str)->Result<(),String> {
        let result=match decision {
            "approve"|"deny"=>sqlx::query("UPDATE remote_oauth_requests SET status=? WHERE origin=? AND project=? AND id=? AND status='pending' AND expires>? AND revoked=0").bind(if decision=="approve" {"approved"} else {"denied"}).bind(&self.origin).bind(project).bind(id).bind(now()).execute(&self.pool).await,
            "revoke"=>sqlx::query("UPDATE remote_oauth_requests SET revoked=1 WHERE origin=? AND project=? AND id=?").bind(&self.origin).bind(project).bind(id).execute(&self.pool).await,
            _=>return Err("Unknown authorization decision".into()),
        }.map_err(db)?;
        if result.rows_affected()!=1 {return Err("Authorization request is expired or no longer pending".into());} Ok(())
    }
    pub async fn revoke_project(&self,project:&str)->Result<(),String> {sqlx::query("UPDATE remote_oauth_requests SET revoked=1 WHERE origin=? AND project=?").bind(&self.origin).bind(project).execute(&self.pool).await.map_err(db)?; Ok(())}
    pub async fn verify(&self,token:&str)->Result<Principal,String> {
        let row=sqlx::query("SELECT r.id,r.project,r.scope FROM remote_oauth_tokens t JOIN remote_oauth_requests r ON r.origin=t.origin AND r.id=t.request_id WHERE t.origin=? AND t.access_hash=? AND t.expires>? AND r.status='delivered' AND r.revoked=0").bind(&self.origin).bind(hash(token)).bind(now()).fetch_optional(&self.pool).await.map_err(db)?.ok_or("Invalid or expired access token")?;
        let scopes:HashSet<String>=row.get::<String,_>("scope").split_whitespace().map(str::to_owned).collect();
        Ok(Principal{id:format!("oauth:{}",row.get::<String,_>("id")),subject:None,key_projects:vec![row.get("project")],key_write:scopes.contains(WRITE_SCOPE),scopes})
    }
}
#[derive(Clone)]
struct Provider {store:OAuthStore,config:Arc<RemoteConfig>}
fn response(status:StatusCode,value:Value)->Response {secure((status,Json(value)).into_response())}
fn secure(mut r:Response)->Response {for (name,value) in [("cache-control","no-store"),("pragma","no-cache"),("referrer-policy","no-referrer"),("x-content-type-options","nosniff")] {r.headers_mut().insert(name,HeaderValue::from_static(value));} r}
fn error(message:&str)->Response {response(StatusCode::BAD_REQUEST,json!({"error":"invalid_request","error_description":message}))}
pub fn router(store:OAuthStore,config:Arc<RemoteConfig>)->Router {
    Router::new().route("/.well-known/oauth-authorization-server/oauth",get(metadata)).route("/oauth/.well-known/openid-configuration",get(metadata)).route("/oauth/jwks",get(||async{response(StatusCode::OK,json!({"keys":[]}))})).route("/oauth/register",post(register)).route("/oauth/authorize",get(authorize)).route("/oauth/requests/{nonce}",get(poll)).route("/oauth/token",post(token)).route("/oauth/revoke",post(revoke)).with_state(Provider{store,config})
}
async fn metadata(State(p):State<Provider>)->Response {let origin=&p.store.origin;response(StatusCode::OK,json!({"issuer":format!("{origin}/oauth"),"authorization_endpoint":format!("{origin}/oauth/authorize"),"token_endpoint":format!("{origin}/oauth/token"),"registration_endpoint":format!("{origin}/oauth/register"),"revocation_endpoint":format!("{origin}/oauth/revoke"),"response_types_supported":["code"],"grant_types_supported":["authorization_code","refresh_token"],"token_endpoint_auth_methods_supported":["none"],"code_challenge_methods_supported":["S256"],"scopes_supported":[CONTEXT_SCOPE,WRITE_SCOPE],"authorization_response_iss_parameter_supported":true}))}
fn redirect(value:&str,native:bool)->bool {
    if value.len()>2048 {return false;} let Ok(url)=Url::parse(value) else{return false;};
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {return false;}
    let loopback=url.host_str().is_some_and(|h|h.trim_matches(['[',']']).parse::<std::net::IpAddr>().is_ok_and(|ip|ip.is_loopback()));
    if native {url.scheme()=="http" && url.host_str()==Some("127.0.0.1") && url.port().is_some() && url.path()=="/callback" && url.query().is_none()}
    else {url.host_str().is_some() && (url.scheme()=="https" || (url.scheme()=="http" && loopback))}
}
async fn register(State(p):State<Provider>,Json(body):Json<Value>)->Response {
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
        let mut tx=p.store.pool.begin().await.map_err(db)?;
        // Acquire the SQLite write lock before checking the public registration quota.
        sqlx::query("DELETE FROM remote_oauth_clients WHERE origin=? AND native=0 AND created<? AND id NOT IN (SELECT client_id FROM remote_oauth_requests WHERE origin=?)").bind(&p.store.origin).bind(now()-86400).bind(&p.store.origin).execute(&mut *tx).await.map_err(db)?;
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM remote_oauth_clients WHERE origin=?").bind(&p.store.origin).fetch_one(&mut *tx).await.map_err(db)?;
        if count>=512 {return Err("Client registration capacity reached".into());}
        let id=uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO remote_oauth_clients VALUES (?,?,?,?,0,?)").bind(&p.store.origin).bind(&id).bind(name).bind(serde_json::to_string(&redirects).map_err(|e|e.to_string())?).bind(now()).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok::<_,String>(json!({"client_id":id,"client_id_issued_at":now(),"client_name":name,"redirect_uris":redirects,"token_endpoint_auth_method":"none","grant_types":["authorization_code","refresh_token"],"response_types":["code"]}))
    }.await; match result {Ok(value)=>response(StatusCode::CREATED,value),Err(e)=>error(&e)}
}
#[derive(Deserialize)]
struct Authorization {client_id:String,redirect_uri:String,response_type:String,state:String,code_challenge:String,code_challenge_method:String,scope:String,resource:String}
async fn authorize(State(p):State<Provider>,Query(q):Query<Authorization>)->Response {
    let result=async {
        let row=sqlx::query("SELECT redirects,native FROM remote_oauth_clients WHERE origin=? AND id=?").bind(&p.store.origin).bind(&q.client_id).fetch_optional(&p.store.pool).await.map_err(db)?.ok_or("Unknown OAuth client")?;
        let native=row.get::<i64,_>("native")!=0; let registered:Vec<String>=serde_json::from_str(&row.get::<String,_>("redirects")).map_err(|_|"Invalid registered client")?;
        if !redirect(&q.redirect_uri,native) || (!native && !registered.contains(&q.redirect_uri)) {return Err("Redirect URI is not registered".into());}
        let project=p.config.projects.iter().find(|project|q.resource==format!("{}/projects/{}/mcp",p.store.origin,project.id) || (p.config.projects.len()==1 && q.resource==p.store.origin)).ok_or("Unknown project resource")?;
        let scope:HashSet<_>=q.scope.split_whitespace().collect();
        if q.response_type!="code" || q.state.is_empty() || q.state.len()>2048 || q.code_challenge_method!="S256" || q.code_challenge.len()!=43 || URL_SAFE_NO_PAD.decode(&q.code_challenge).map_or(true,|v|v.len()!=32) || !scope.contains(CONTEXT_SCOPE) || scope.iter().any(|s|!matches!(*s,CONTEXT_SCOPE|WRITE_SCOPE)) {return Err("Authorization requires state, S256 PKCE and supported scopes".into());}
        let mut tx=p.store.pool.begin().await.map_err(db)?;
        sqlx::query("DELETE FROM remote_oauth_requests WHERE origin=? AND expires<? AND status IN ('pending','denied')").bind(&p.store.origin).bind(now()).execute(&mut *tx).await.map_err(db)?;
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM remote_oauth_requests WHERE origin=? AND status='pending' AND expires>?").bind(&p.store.origin).bind(now()).fetch_one(&mut *tx).await.map_err(db)?;
        if count>=128 {return Err("Pending authorization capacity reached".into());}
        let id=uuid::Uuid::new_v4().to_string(); let nonce=secret(); let scope=if scope.contains(WRITE_SCOPE) {"ax:context ax:write"} else {CONTEXT_SCOPE};
        sqlx::query("INSERT INTO remote_oauth_requests(origin,id,nonce_hash,client_id,redirect,state,challenge,scope,project,resource,expires) VALUES (?,?,?,?,?,?,?,?,?,?,?)").bind(&p.store.origin).bind(&id).bind(hash(&nonce)).bind(&q.client_id).bind(&q.redirect_uri).bind(&q.state).bind(&q.code_challenge).bind(scope).bind(&project.id).bind(&q.resource).bind(now()+600).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?; Ok::<_,String>((id,nonce))
    }.await;
    match result {Err(e)=>error(&e),Ok((id,nonce))=>{
        // Only generated UUID/hex values are embedded; no caller-controlled HTML or URL.
        let html=format!(r#"<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>Ax authorization</title><style>body{{font:16px system-ui;background:#1b1b1b;color:#f0f0f0;max-width:40rem;margin:12vh auto;padding:2rem}}code{{overflow-wrap:anywhere}}p{{line-height:1.6}}</style><h1>Connect to Ax</h1><p>Ask the owner of the hosting computer to approve this request in Settings → Remote MCP. Match this request ID:</p><p><code>{id}</code></p><p id="status">Waiting for host approval…</p><script>const poll=async()=>{{try{{const r=await fetch('/oauth/requests/{nonce}',{{cache:'no-store'}});const data=await r.json();if(data.redirect){{location.replace(data.redirect);return}}if(data.status==='denied'||data.status==='expired'){{document.getElementById('status').textContent='Request '+data.status+'. Start a new connection to retry.';return}}}}catch{{}}setTimeout(poll,1500)}};poll();</script></html>"#);
        let mut r=secure(Html(html).into_response());r.headers_mut().insert("content-security-policy",HeaderValue::from_static("default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'"));r
    }}
}
async fn poll(State(p):State<Provider>,Path(nonce):Path<String>)->Response {
    let result=async {
        if nonce.len()!=64 || !nonce.bytes().all(|b|b.is_ascii_hexdigit()) {return Err("Invalid request".into());}
        let row=sqlx::query("SELECT id,status,expires,revoked,redirect,state FROM remote_oauth_requests WHERE origin=? AND nonce_hash=?").bind(&p.store.origin).bind(hash(&nonce)).fetch_optional(&p.store.pool).await.map_err(db)?.ok_or("Unknown request")?;
        if row.get::<i64,_>("expires")<=now() || row.get::<i64,_>("revoked")!=0 {return Ok(json!({"status":"expired"}));}
        let status:String=row.get("status"); if status!="approved" {return Ok(json!({"status":status}));}
        let code=secret(); let changed=sqlx::query("UPDATE remote_oauth_requests SET status='delivered',code_hash=? WHERE origin=? AND id=? AND status='approved' AND revoked=0 AND expires>?").bind(hash(&code)).bind(&p.store.origin).bind(row.get::<String,_>("id")).bind(now()).execute(&p.store.pool).await.map_err(db)?;
        if changed.rows_affected()!=1 {return Ok(json!({"status":"expired"}));}
        let mut url=Url::parse(&row.get::<String,_>("redirect")).map_err(|_|"Invalid redirect")?; url.query_pairs_mut().extend_pairs([("code",code.as_str()),("state",row.get::<String,_>("state").as_str()),("iss",format!("{}/oauth",p.store.origin).as_str())]);
        Ok::<_,String>(json!({"status":"approved","redirect":url.as_str()}))
    }.await; match result {Ok(v)=>response(StatusCode::OK,v),Err(e)=>error(&e)}
}
async fn token(State(p):State<Provider>,Form(q):Form<BTreeMap<String,String>>)->Response {
    let result=async {
        let value=|key:&str|q.get(key).map(String::as_str).unwrap_or(""); let client=value("client_id");
        let mut tx=p.store.pool.begin().await.map_err(db)?;
        sqlx::query("DELETE FROM remote_oauth_tokens WHERE origin=? AND refresh_expires<=?").bind(&p.store.origin).bind(now()).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("DELETE FROM remote_oauth_used_refresh WHERE origin=? AND expires<=?").bind(&p.store.origin).bind(now()).execute(&mut *tx).await.map_err(db)?;
        let row=match value("grant_type") {
            "authorization_code"=>{
                let row=sqlx::query("SELECT * FROM remote_oauth_requests WHERE origin=? AND code_hash=? AND client_id=? AND status='delivered' AND revoked=0 AND expires>?").bind(&p.store.origin).bind(hash(value("code"))).bind(client).bind(now()).fetch_optional(&mut *tx).await.map_err(db)?.ok_or("Invalid authorization grant")?;
                let verifier=value("code_verifier");
                if !(43..=128).contains(&verifier.len()) || !verifier.bytes().all(|b|b.is_ascii_alphanumeric() || b"-._~".contains(&b)) || URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))!=row.get::<String,_>("challenge") || value("redirect_uri")!=row.get::<String,_>("redirect") {return Err("Invalid authorization grant".into());} row
            },
            "refresh_token"=>{
                let row=sqlx::query("SELECT r.* FROM remote_oauth_tokens t JOIN remote_oauth_requests r ON r.origin=t.origin AND r.id=t.request_id WHERE t.origin=? AND t.refresh_hash=? AND t.client_id=? AND t.refresh_expires>? AND r.revoked=0").bind(&p.store.origin).bind(hash(value("refresh_token"))).bind(client).bind(now()).fetch_optional(&mut *tx).await.map_err(db)?;
                if let Some(row)=row {row} else {
                    let id:Option<String>=sqlx::query_scalar("SELECT request_id FROM remote_oauth_used_refresh WHERE origin=? AND hash=? AND client_id=? AND expires>?").bind(&p.store.origin).bind(hash(value("refresh_token"))).bind(client).bind(now()).fetch_optional(&mut *tx).await.map_err(db)?;
                    if let Some(id)=id {sqlx::query("UPDATE remote_oauth_requests SET revoked=1 WHERE origin=? AND id=?").bind(&p.store.origin).bind(id).execute(&mut *tx).await.map_err(db)?;tx.commit().await.map_err(db)?;}
                    return Err("Invalid refresh grant".into());
                }
            },
            _=>return Err("Unsupported grant type".into()),
        };
        if value("resource")!=row.get::<String,_>("resource") {return Err("Invalid resource".into());}
        let id:String=row.get("id");
        if value("grant_type")=="authorization_code" {sqlx::query("UPDATE remote_oauth_requests SET code_hash=NULL WHERE origin=? AND id=?").bind(&p.store.origin).bind(&id).execute(&mut *tx).await.map_err(db)?;}
        else {
            sqlx::query("INSERT INTO remote_oauth_used_refresh VALUES (?,?,?,?,?)").bind(&p.store.origin).bind(hash(value("refresh_token"))).bind(client).bind(&id).bind(now()+30*86400).execute(&mut *tx).await.map_err(db)?;
            sqlx::query("DELETE FROM remote_oauth_tokens WHERE origin=? AND refresh_hash=?").bind(&p.store.origin).bind(hash(value("refresh_token"))).execute(&mut *tx).await.map_err(db)?;
        }
        let access=format!("axo_{}",secret());let refresh=secret();
        sqlx::query("INSERT INTO remote_oauth_tokens VALUES (?,?,?,?,?,?,?)").bind(&p.store.origin).bind(hash(&access)).bind(hash(&refresh)).bind(&id).bind(client).bind(now()+3600).bind(now()+30*86400).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok::<_,String>(json!({"access_token":access,"token_type":"Bearer","expires_in":3600,"refresh_token":refresh,"scope":row.get::<String,_>("scope")}))
    }.await;match result {Ok(v)=>response(StatusCode::OK,v),Err(e)=>response(StatusCode::BAD_REQUEST,json!({"error":"invalid_grant","error_description":e}))}
}
async fn revoke(State(p):State<Provider>,Form(q):Form<BTreeMap<String,String>>)->Response {
    let token=q.get("token").map(String::as_str).unwrap_or("");let client=q.get("client_id").map(String::as_str).unwrap_or("");
    match sqlx::query("UPDATE remote_oauth_requests SET revoked=1 WHERE origin=? AND id IN (SELECT request_id FROM remote_oauth_tokens WHERE origin=? AND client_id=? AND (access_hash=? OR refresh_hash=?))").bind(&p.store.origin).bind(&p.store.origin).bind(client).bind(hash(token)).bind(hash(token)).execute(&p.store.pool).await {Ok(_)=>response(StatusCode::OK,json!({})),Err(_)=>response(StatusCode::INTERNAL_SERVER_ERROR,json!({"error":"server_error"}))}
}
