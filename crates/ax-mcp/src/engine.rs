//! Shared MCP engine with lazy Ax initialization.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ax_context::directory::find_nearest_ax_root;
use ax_core::Ax;
use ax_extraction::orchestrator::IndexOptions;
use sqlx::SqlitePool;
use tokio::sync::Mutex;

use crate::chat_session::TurnCounter;
use crate::policy_session::PolicySessions;
use crate::query_pool::{resolve_pool_size, QueryPool};

async fn seed_memories_if_empty(pool: &SqlitePool, project_root: &Path) {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memories")
        .fetch_one(pool)
        .await
        .unwrap_or(-1);
    if count != 0 {
        return;
    }
    if project_root.join(".git").exists() {
        match ax_memory::capture_git_history(pool, project_root, 100).await {
            Ok(r) => {
                eprintln!(
                    "{}",
                    ax_usage::format_ax_tagged(format!(
                        "auto-seeded memory vault: {} captured, {} trivial skipped",
                        r.captured, r.skipped_trivial
                    ))
                );
                return;
            }
            Err(e) => eprintln!(
                "{}",
                ax_usage::format_ax_tagged(format!(
                    "git memory seed failed, falling back to graph: {e}"
                ))
            ),
        }
    }
    match ax_memory::seed_from_graph(pool).await {
        Ok(n) if n > 0 => eprintln!(
            "{}",
            ax_usage::format_ax_tagged(format!(
                "auto-seeded memory vault from graph: {n} memories"
            ))
        ),
        Ok(_) => {}
        Err(e) => eprintln!(
            "{}",
            ax_usage::format_ax_tagged(format!("graph memory seed failed: {e}"))
        ),
    }
}

pub struct McpEngine {
    remote: bool,
    ax: Arc<Mutex<Option<Ax>>>,
    project_root: Option<PathBuf>,
    query_pool: Option<QueryPool>,
    catch_up_done: Arc<AtomicBool>,
    policy_sessions: PolicySessions,
    turns: TurnCounter,
    /// How many times this engine opened an `Ax` handle.
    opens: u64,
}

impl Default for McpEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl McpEngine {
    pub fn new() -> Self {
        Self {
            remote: false,
            ax: Arc::new(Mutex::new(None)),
            project_root: None,
            query_pool: None,
            catch_up_done: Arc::new(AtomicBool::new(false)),
            policy_sessions: PolicySessions::default(),
            turns: TurnCounter::default(),
            opens: 0,
        }
    }

    pub fn with_project_root(project_root: PathBuf) -> Self {
        let pool_size = resolve_pool_size();
        let query_pool = if pool_size > 0 {
            Some(QueryPool::new(pool_size))
        } else {
            None
        };
        Self {
            remote: false,
            ax: Arc::new(Mutex::new(None)),
            project_root: Some(project_root),
            query_pool,
            catch_up_done: Arc::new(AtomicBool::new(false)),
            policy_sessions: PolicySessions::default(),
            turns: TurnCounter::default(),
            opens: 0,
        }
    }

    /// Start the debounced file watcher and connect-time catch-up background services.
    pub fn start_background_services(project_root: &Path) {
        drop(Ax::spawn_background_watch_as(
            project_root.to_path_buf(),
            "mcp",
        ));
        ax_usage::spawn_ensure_daily_pricing_sync();
    }

    pub fn policy_sessions(&mut self) -> &mut PolicySessions {
        &mut self.policy_sessions
    }

    /// Remote conversations must never inherit a local Cursor hook's chat ID.
    pub fn for_remote(project_root: PathBuf) -> Self {
        let mut engine = Self::with_project_root(project_root);
        engine.remote = true;
        engine
    }

    pub fn is_remote(&self) -> bool {
        self.remote
    }

    /// Preflight turns per chat since its last `ax_session` write.
    pub fn turns(&mut self) -> &mut TurnCounter {
        &mut self.turns
    }

    pub fn query_pool(&self) -> Option<&QueryPool> {
        self.query_pool.as_ref()
    }

    pub async fn ensure_initialized(&mut self) -> Result<(), String> {
        if self.ax.lock().await.is_some() {
            return Ok(());
        }
        let root = if let Some(r) = &self.project_root {
            r.clone()
        } else {
            let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
            find_nearest_ax_root(&cwd).unwrap_or(cwd)
        };
        self.project_root = Some(root.clone());
        let ax = self.open_handle(&root).await?;
        seed_memories_if_empty(ax.db_pool(), &root).await;
        *self.ax.lock().await = Some(ax);
        self.run_catch_up_sync().await;
        Ok(())
    }

    /// Filesystem reconciliation on first MCP session — catches edits made while the server was down.
    async fn run_catch_up_sync(&self) {
        if self.catch_up_done.swap(true, Ordering::SeqCst) {
            return;
        }
        let mut guard = self.ax.lock().await;
        let Some(ax) = guard.as_mut() else {
            return;
        };
        let opts = IndexOptions {
            quiet: true,
            ..IndexOptions::default()
        };
        match ax.sync(opts, None).await {
            Ok(result) if result.files_indexed > 0 => {
                tracing::info!(
                    "connect-time catch-up: synced {} file(s) in {}ms",
                    result.files_indexed,
                    result.duration_ms
                );
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("connect-time catch-up failed: {}", e),
        }
    }

    /// Policy sync on the engine's one Ax handle.
    ///
    /// Opens the handle when there is none yet and reopens it only when the
    /// database file was replaced on disk; every call re-checks `.agents/` policy.
    pub async fn ensure_policy_fresh(&mut self) -> Result<(), String> {
        let root = if let Some(r) = &self.project_root {
            r.clone()
        } else {
            let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
            find_nearest_ax_root(&cwd).ok_or_else(|| "no ax project root".to_string())?
        };
        self.project_root = Some(root.clone());
        if self.ax.lock().await.is_none() {
            let ax = self.open_handle(&root).await?;
            *self.ax.lock().await = Some(ax);
        } else {
            self.reopen_if_replaced().await?;
        }
        let guard = self.ax.lock().await;
        let ax = guard.as_ref().ok_or("ax not initialized")?;
        ax.ensure_policy_ready().await.map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn open_handle(&mut self, root: &Path) -> Result<Ax, String> {
        let ax = Ax::open(root).await.map_err(|e| e.to_string())?;
        self.opens += 1;
        Ok(ax)
    }

    pub async fn reopen_if_replaced(&mut self) -> Result<bool, String> {
        let mut guard = self.ax.lock().await;
        if let Some(ax) = guard.as_mut() {
            ax.reopen_if_replaced().await.map_err(|e| e.to_string())
        } else {
            Ok(false)
        }
    }

    pub async fn lock_ax(&self) -> tokio::sync::MutexGuard<'_, Option<Ax>> {
        self.ax.lock().await
    }

    pub fn project_root(&self) -> Option<&PathBuf> {
        self.project_root.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn project() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        std::fs::create_dir_all(&root).unwrap();
        drop(Ax::init(&root).await.expect("init ax project"));
        let root = root.canonicalize().unwrap();
        (dir, root)
    }

    #[tokio::test]
    async fn policy_tool_calls_keep_one_ax_handle() {
        let (_dir, root) = project().await;
        let mut engine = McpEngine::with_project_root(root);
        for _ in 0..3 {
            engine.ensure_policy_fresh().await.expect("policy fresh");
        }
        assert_eq!(engine.opens, 1, "policy calls reopened the database");
        assert!(engine.lock_ax().await.is_some());
    }

    #[tokio::test]
    async fn policy_tool_calls_see_rules_saved_by_another_process() {
        let (_dir, root) = project().await;
        let mut engine = McpEngine::with_project_root(root.clone());
        engine.ensure_policy_fresh().await.expect("policy fresh");
        // Another process (the CLI, another daemon) saves a rule with its own handle.
        let mut other = Ax::open(&root).await.expect("open second handle");
        let saved = crate::tools::ToolHandler::call_tool(
            &mut other,
            "ax_policy_capture",
            serde_json::json!({"action":"save","rule":{
                "frontmatter":{"id":"fresh-rule","level":"CRITICAL","alwaysApply":true},
                "body":"Never write the marker.\n\nguard: forbid-content: \"FRESH_MARKER\"\n"}}),
        )
        .await
        .expect("save rule");
        assert_eq!(saved["ok"], true, "{saved}");
        drop(other);
        engine.ensure_policy_fresh().await.expect("policy fresh");
        let mut guard = engine.lock_ax().await;
        let ax = guard.as_mut().unwrap();
        let listed = crate::tools::ToolHandler::call_tool(ax, "ax_rules", serde_json::json!({}))
            .await
            .expect("ax_rules");
        assert!(listed.to_string().contains("fresh-rule"), "{listed}");
        drop(guard);
        assert_eq!(engine.opens, 1);
    }
}
