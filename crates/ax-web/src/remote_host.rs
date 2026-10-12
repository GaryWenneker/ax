//! Persistent remote MCP service controlled only by the local owner API.
use ax_mcp::{remote::{RemoteConfig,config::{OAuthConfig,Project},oauth::OAuthStore,publications},replication_credentials::{self,Credentials}};
use ax_global_db::remote_mcp_publications::{self as catalog,ClientRegistrations};
use fs2::FileExt;
use serde::{Serialize,Deserialize};
use std::{path::{Path,PathBuf},time::Duration,process::Stdio};
use tokio::process::Command;
const FILE:&str="command-center-remote-host.json";
const RUNTIME:&str="command-center-remote-mcp.json";
const KEY:&str="remote-mcp-host";
#[derive(Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Host {pub config:RemoteConfig,pub port:u16,pub enabled:bool}
pub(crate) fn read(root:&Path)->Result<Option<Host>,String> {
    let path=root.join(".ax").join(FILE);match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(None),Err(e)=>Err(e.to_string()),
        Ok(m) if !m.is_file() || m.file_type().is_symlink() || m.len()>256*1024=>Err("Unsafe remote host settings".into()),
        Ok(_)=>serde_json::from_slice(&std::fs::read(path).map_err(|e|e.to_string())?).map(Some).map_err(|e|e.to_string()),
    }
}
fn write<T:Serialize>(root:&Path,name:&str,value:&T)->Result<PathBuf,String> {
    let path=root.join(".ax").join(name);
    if let Ok(m)=std::fs::symlink_metadata(&path) {if !m.is_file() || m.file_type().is_symlink() {return Err("Unsafe host configuration file".into());}}
    let mut file=tempfile::NamedTempFile::new_in(root.join(".ax")).map_err(|e|e.to_string())?;
    serde_json::to_writer_pretty(&mut file,value).map_err(|e|e.to_string())?;file.as_file().sync_all().map_err(|e|e.to_string())?;
    file.persist(&path).map_err(|e|e.error.to_string())?;Ok(path)
}
fn lock(root:&Path)->Result<std::fs::File,String> {
    let path=root.join(".ax/command-center-remote-host.lock");
    if std::fs::symlink_metadata(&path).is_ok_and(|m|m.file_type().is_symlink()) {return Err("Unsafe host lock".into());}
    let file=std::fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).open(path).map_err(|e|e.to_string())?;
    file.try_lock_exclusive().map_err(|_|"Remote host is being changed by another Ax process")?;Ok(file)
}
async fn credentials(root:&Path,create:bool)->Result<Credentials,String> {
    if let Ok(control)=std::env::var("AX_REMOTE_HOST_CONTROL") {if control.len()>=32 && control.len()<=4096 && control.is_ascii() && !control.bytes().any(|b|b.is_ascii_whitespace() || b.is_ascii_control()) {return Ok(Credentials{token:control.clone(),control});}}
    let root=root.to_owned();tokio::task::spawn_blocking(move|| {
        match replication_credentials::load(&root,KEY) {Ok(value)=>Ok(value),Err(error) if !create=>Err(error),Err(_)=>{
            let control=format!("{}{}",uuid::Uuid::new_v4().simple(),uuid::Uuid::new_v4().simple());let value=Credentials{token:control.clone(),control};replication_credentials::save(&root,KEY,&value)?;Ok(value)
        }}
    }).await.map_err(|e|e.to_string())?
}
fn scoped(host:&mut Host,root:&Path)->Result<(),String> {
    host.config.validate()?; let root=root.canonicalize().map_err(|e|e.to_string())?;
    if host.config.projects.len()!=1 || host.config.projects[0].root!=root {return Err("The host configuration must register only the currently open project".into());} Ok(())
}
fn client()->Result<reqwest::Client,String> {reqwest::Client::builder().no_proxy().timeout(Duration::from_millis(700)).redirect(reqwest::redirect::Policy::none()).build().map_err(|e|e.to_string())}
async fn probe(host:&Host,credentials:&Credentials)->Result<bool,String> {
    let response=client()?.post(format!("http://127.0.0.1:{}/_ax/host/status",host.port)).bearer_auth(&credentials.control).send().await;
    match response {
        Err(_)=>Ok(false),Ok(response) if !response.status().is_success()=>Err("Host port is occupied by a service that rejected local control".into()),
        Ok(response)=>{let value:serde_json::Value=response.json().await.map_err(|_|"Invalid host status")?;if value["publicUrl"].as_str()!=Some(&host.config.public_url) || value["ready"]!=true {return Err("Host port belongs to another service".into());}Ok(true)}
    }
}
pub(crate) async fn running(root:&Path,host:&Host)->Result<bool,String> {probe(host,&credentials(root,false).await?).await}
async fn spawn(root:&Path,host:&Host,credentials:&Credentials)->Result<(),String> {
    let executable=std::env::current_exe().map_err(|e|e.to_string())?;
    if executable.file_stem().and_then(|s|s.to_str())!=Some("ax") {return Err("Start Command Center using the Ax CLI to manage its host".into());}
    let probe_socket=tokio::net::TcpListener::bind(("127.0.0.1",host.port)).await.map_err(|_|"Remote MCP port is already in use")?;
    let path=write(root,RUNTIME,&host.config)?;let mut command=Command::new(executable);
    command.args(["mcp","remote","serve","--config"]).arg(path).arg("--port").arg(host.port.to_string()).env("AX_REMOTE_HOST_CONTROL",&credentials.control).env("AX_REMOTE_HOST_CHILD","1").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(false);
    #[cfg(unix)] command.process_group(0);
    #[cfg(windows)] command.creation_flags(0x0000_0008|0x0000_0200);
    drop(probe_socket);let mut child=command.spawn().map_err(|e|e.to_string())?;
    for _ in 0..60 {if child.try_wait().map_err(|e|e.to_string())?.is_some() {break;}if probe(host,credentials).await.unwrap_or(false) {return Ok(());}tokio::time::sleep(Duration::from_millis(100)).await;}
    let _=child.kill().await;Err("Remote MCP host did not start. Check the configuration and Ax installation".into())
}
async fn stop(host:&Host,credentials:&Credentials)->Result<(),String> {
    if !probe(host,credentials).await? {return Ok(());}
    let result=client()?.post(format!("http://127.0.0.1:{}/_ax/host/stop",host.port)).bearer_auth(&credentials.control).send().await.map_err(|e|e.to_string())?;
    if !result.status().is_success() {return Err("Host refused shutdown".into());}
    for _ in 0..60 {if !probe(host,credentials).await? {return Ok(());}tokio::time::sleep(Duration::from_millis(100)).await;}Err("Remote MCP host is still stopping".into())
}
pub(crate) async fn setup(root:&Path,public_url:Option<String>,config:Option<RemoteConfig>,clients:Option<ClientRegistrations>)->Result<(),String> {
    let _lock=lock(root)?;if read(root)?.is_some() {return Err("Remove or restart the existing host before changing its configuration".into());}
    let config=if let Some(config)=config {config} else {
        let value=public_url.ok_or("Enter the public HTTPS origin")?;let url=ax_mcp::remote::config::https_url(&value)?;if url.path()!="/" {return Err("Use an HTTPS origin without a path".into());}let origin=url.origin().ascii_serialization();
        let workspace=catalog::workspace_id(root).map_err(|e|e.to_string())?;
        RemoteConfig {public_url:origin.clone(),oauth:OAuthConfig{built_in:true,issuer:format!("{origin}/oauth"),jwks_uri:format!("{origin}/oauth/jwks")},projects:vec![Project{id:format!("project_{}",&workspace[..24]),root:root.to_owned(),subjects:vec!["local-owner".into()],write_subjects:vec![]}],api_keys:vec![],allowed_origins:vec![]}
    };
    let listener=tokio::net::TcpListener::bind(("127.0.0.1",0)).await.map_err(|e|e.to_string())?;let port=listener.local_addr().map_err(|e|e.to_string())?.port();drop(listener);
    let mut host=Host{config,port,enabled:true};scoped(&mut host,root)?;
    let clients=if host.config.oauth.built_in {Some(ClientRegistrations{ax_proxy:Some(OAuthStore::open(&host.config.public_url).await?.native_client().await?),cursor:None,vscode:None})} else {clients};
    publications::register(&host.config,None,clients).await?;
    let credentials=credentials(root,true).await?;spawn(root,&host,&credentials).await?;
    if let Err(error)=write(root,FILE,&host) {let _=stop(&host,&credentials).await;return Err(error);} Ok(())
}
pub async fn auto_start(root:&Path)->Result<(),String> {
    if std::env::var("AX_REMOTE_HOST_CHILD").as_deref()==Ok("1") {return Ok(());}
    let Some(mut host)=read(root)? else {return Ok(());};if !host.enabled {return Ok(());}scoped(&mut host,root)?;
    let _lock=lock(root)?;let credentials=credentials(root,false).await?;
    if !probe(&host,&credentials).await? {spawn(root,&host,&credentials).await?;}Ok(())
}
pub(crate) async fn change(root:&Path,operation:&str)->Result<(),String> {
    let _lock=lock(root)?;let mut host=read(root)?.ok_or("No remote host configured")?;scoped(&mut host,root)?;let credentials=credentials(root,false).await?;
    match operation {
        "start"=>{if !probe(&host,&credentials).await? {spawn(root,&host,&credentials).await?;}host.enabled=true;write(root,FILE,&host)?;},
        "stop"=>{stop(&host,&credentials).await?;host.enabled=false;write(root,FILE,&host)?;},
        "remove"=>{
            stop(&host,&credentials).await?;
            if host.config.oauth.built_in {OAuthStore::open(&host.config.public_url).await?.revoke_project(&host.config.projects[0].id).await?;}
            let pool=crate::access_api::pool().await?;
            for publication in catalog::list_for_root(&pool,root).await.map_err(|e|e.to_string())? {if publication.public_origin==host.config.public_url {catalog::remove(&pool,&publication.id,root).await.map_err(|e|e.to_string())?;}}
            if std::env::var("AX_REMOTE_HOST_CONTROL").is_err() {let root=root.to_owned();tokio::task::spawn_blocking(move||replication_credentials::remove(&root,KEY)).await.map_err(|e|e.to_string())??;}
            std::fs::remove_file(root.join(".ax").join(FILE)).map_err(|e|e.to_string())?;let runtime=root.join(".ax").join(RUNTIME);if runtime.exists() {std::fs::remove_file(runtime).map_err(|e|e.to_string())?;}
        },_=>return Err("Unknown host action".into()),
    }Ok(())
}
