//! Verified chunk transfer for the complete project datasets.
use crate::{server_config::{ServerConfig,ProjectRegistration},server_snapshot::Snapshot};
use ax_db::replication::{self,Dataset};

pub fn manifests(snapshot: &Snapshot) -> Result<Vec<Dataset>,String> {
    if snapshot.schema_version != 3 { return Err("complete replication requires protocol 3; update both nodes".into()); }
    let items:Vec<Dataset>=serde_json::from_str(snapshot.files.get(replication::DATA_PATH).ok_or("missing datasets")?).map_err(|e|e.to_string())?;
    replication::validate(&items).map_err(|e|e.to_string())?; Ok(items)
}
fn project<'a>(config:&'a ServerConfig,id:&str)->Result<&'a ProjectRegistration,String> { config.projects.iter().find(|p|p.id==id).ok_or_else(||"project not registered".into()) }

pub async fn verify_contribution(config:&ServerConfig,id:&str,snapshot:&Snapshot)->Result<(),String> {
    let items=manifests(snapshot)?;
    if items.len()!=1 || items[0].origin!=snapshot.primary_node_id { return Err("dataset origin does not match the authenticated node".into()); }
    let root=&project(config,id)?.path;
    let ax=ax_core::Ax::open(root).await.map_err(|e|e.to_string())?;
    replication::verify(root,ax.db_pool(),&items).await.map_err(|e|e.to_string())?;
    for item in items { replication::pin(root,&item).map_err(|e|e.to_string())?; } Ok(())
}
fn url(config:&ServerConfig,project:&ProjectRegistration,hash:&str)->Result<reqwest::Url,String> {
    reqwest::Url::parse(config.primary_url.as_deref().ok_or("missing primary URL")?).map_err(|e|e.to_string())?
        .join(&format!("/v1/projects/{}/chunks/{hash}",project.id)).map_err(|e|e.to_string())
}
pub async fn upload(client:&reqwest::Client,config:&ServerConfig,project:&ProjectRegistration,snapshot:&Snapshot)->Result<(),String> {
    let token=std::env::var(&project.token_env).map_err(|_|"missing project credential")?;
    for hash in replication::hashes(&manifests(snapshot)?) {
        let url=url(config,project,&hash)?;
        let head=client.head(url.clone()).bearer_auth(&token).header("x-ax-node-id",&config.node_id).send().await.map_err(|e|e.to_string())?;
        if head.status().is_success() { continue; } if head.status()!=reqwest::StatusCode::NOT_FOUND { return Err(format!("chunk check failed: {}",head.status())); }
        let root=project.path.clone(); let id=hash.clone(); let bytes=tokio::task::spawn_blocking(move||replication::read_chunk(&root,&id)).await.map_err(|e|e.to_string())?.map_err(|e|e.to_string())?;
        let response=client.put(url).bearer_auth(&token).header("x-ax-node-id",&config.node_id).header("content-type","application/octet-stream").body(bytes).send().await.map_err(|e|e.to_string())?;
        if !response.status().is_success() { return Err(format!("chunk upload failed: {}",response.status())); }
    } Ok(())
}
pub async fn download(client:&reqwest::Client,config:&ServerConfig,project:&ProjectRegistration,snapshot:&Snapshot)->Result<(),String> {
    let items=manifests(snapshot)?; let token=std::env::var(&project.token_env).map_err(|_|"missing project credential")?;
    for hash in replication::hashes(&items) {
        let root=project.path.clone(); let id=hash.clone();
        if tokio::task::spawn_blocking(move||replication::read_chunk(&root,&id)).await.map_err(|e|e.to_string())?.is_ok() {continue;}
        let mut response=client.get(url(config,project,&hash)?).bearer_auth(&token).header("x-ax-node-id",&config.node_id).send().await.map_err(|e|e.to_string())?;
        if !response.status().is_success() {return Err(format!("chunk download failed: {}",response.status()));}
        if response.content_length().is_some_and(|n|n>replication::CHUNK_BYTES as u64) {return Err("chunk response too large".into());}
        let mut bytes=Vec::new(); while let Some(chunk)=response.chunk().await.map_err(|e|e.to_string())? { if bytes.len().saturating_add(chunk.len())>replication::CHUNK_BYTES {return Err("chunk response exceeded limit".into());} bytes.extend_from_slice(&chunk); }
        let root=project.path.clone(); tokio::task::spawn_blocking(move||replication::store_chunk(&root,&hash,&bytes)).await.map_err(|e|e.to_string())?.map_err(|e|e.to_string())?;
    }
    for item in items {replication::pin(&project.path,&item).map_err(|e|e.to_string())?;} Ok(())
}
pub async fn apply(config:&ServerConfig,snapshot:&Snapshot)->Result<(),String> {
    let root=&project(config,&snapshot.project_id)?.path; let ax=ax_core::Ax::open(root).await.map_err(|e|e.to_string())?;
    replication::project(root,&config.node_id,ax.db_pool(),&manifests(snapshot)?).await.map_err(|e|e.to_string())
}
