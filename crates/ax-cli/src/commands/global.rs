//! `ax global` — init / sync / status for ~/.ax/global.db

use crate::commands::resolve_path;
use crate::ui::{kv_line, ok_line};

pub async fn run_init() -> Result<(), String> {
    let path = ax_global_db::init_default().await.map_err(|e| e.to_string())?;
    println!("{}", ok_line(format!("global db {}", path.display())));
    Ok(())
}

pub async fn run_sync(path: Option<String>) -> Result<(), String> {
    let root = resolve_path(path);
    let global_path = ax_global_db::global_db_path().map_err(|e| e.to_string())?;
    let pool = ax_global_db::open_and_init(&global_path)
        .await
        .map_err(|e| e.to_string())?;
    let result = ax_global_db::sync::sync_project(&pool, &root)
        .await
        .map_err(|e| e.to_string())?;
    pool.close().await;
    println!("{}", ok_line(format!("synced {}", result.project_name)));
    println!("  {}", kv_line("nodes", result.synced_count.to_string()));
    println!("  {}", kv_line("files", result.document_count.to_string()));
    println!("  {}", kv_line("edges", result.edge_count.to_string()));
    println!("  {}", kv_line("ms", result.duration_ms.to_string()));
    println!("  {}", kv_line("db", global_path.display().to_string()));
    Ok(())
}

pub async fn run_status() -> Result<(), String> {
    let path = ax_global_db::global_db_path().map_err(|e| e.to_string())?;
    let st = ax_global_db::status_at(&path).await.map_err(|e| e.to_string())?;
    println!("{}", kv_line("path", st.path));
    println!("  {}", kv_line("projects", st.project_count.to_string()));
    println!("  {}", kv_line("nodes", st.node_count.to_string()));
    println!("  {}", kv_line("documents", st.document_count.to_string()));
    println!("  {}", kv_line("shared", st.shared_knowledge_count.to_string()));
    Ok(())
}
