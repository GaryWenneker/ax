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

/// Work that reconciles the index with the filesystem for one project root.
type CatchUpRunner = Arc<
    dyn Fn(PathBuf) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
        + Send
        + Sync,
>;

/// Seeds an empty memory vault, then syncs files changed while no server ran.
///
/// Runs on its own Ax handle so the engine's handle keeps answering requests.
async fn run_catch_up(root: PathBuf) {
    let mut ax = match Ax::open(&root).await {
        Ok(ax) => ax,
        Err(e) => {
            tracing::warn!("connect-time catch-up could not open the project: {}", e);
            return;
        }
    };
    seed_memories_if_empty(ax.db_pool(), &root).await;
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

fn default_catch_up_runner() -> CatchUpRunner {
    Arc::new(|root| Box::pin(run_catch_up(root)))
}

/// The connect-time catch-up of one project root, shared by every engine of
/// this process: it runs once per root, not once per engine.
#[derive(Clone)]
struct CatchUp {
    running: Arc<AtomicBool>,
    /// Held by the catch-up while it runs; a sync or index call waits for it.
    lock: Arc<Mutex<()>>,
}

fn catch_ups() -> &'static std::sync::Mutex<std::collections::HashMap<PathBuf, CatchUp>> {
    static CATCH_UPS: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<PathBuf, CatchUp>>,
    > = std::sync::OnceLock::new();
    CATCH_UPS.get_or_init(Default::default)
}

/// Tools that sync or index the project themselves, and so wait for a running
/// catch-up instead of contending with it for the project's file lock.
fn waits_for_catch_up(tool: &str) -> bool {
    matches!(tool, "ax_sync" | "ax_index")
}

pub struct McpEngine {
    remote: bool,
    ax: Arc<Mutex<Option<Ax>>>,
    project_root: Option<PathBuf>,
    catch_up_started: Arc<AtomicBool>,
    /// The process-wide catch-up of this engine's root, once it started.
    catch_up: Option<CatchUp>,
    catch_up_runner: CatchUpRunner,
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
            catch_up_started: Arc::new(AtomicBool::new(false)),
            catch_up: None,
            catch_up_runner: default_catch_up_runner(),
            policy_sessions: PolicySessions::default(),
            turns: TurnCounter::default(),
            opens: 0,
        }
    }

    pub fn with_project_root(project_root: PathBuf) -> Self {
        Self {
            remote: false,
            ax: Arc::new(Mutex::new(None)),
            project_root: Some(project_root),
            catch_up_started: Arc::new(AtomicBool::new(false)),
            catch_up: None,
            catch_up_runner: default_catch_up_runner(),
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
    ///
    /// A remote engine starts no catch-up: the remote server's background watch
    /// syncs each registered project once at startup and keeps it current, and
    /// a session must not start index work of its own.
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
        *self.ax.lock().await = Some(ax);
        self.start_catch_up(root);
        Ok(())
    }

    /// Starts the connect-time catch-up in the background, so the request that
    /// opened the handle is answered without waiting for it. It runs once per
    /// project root in this process; a later engine for the same root joins
    /// the one already started. Remote engines start none.
    fn start_catch_up(&mut self, root: PathBuf) {
        if self.catch_up_started.swap(true, Ordering::SeqCst) || self.remote {
            return;
        }
        let mut started = catch_ups().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(existing) = started.get(&root) {
            self.catch_up = Some(existing.clone());
            return;
        }
        let catch_up = CatchUp {
            running: Arc::new(AtomicBool::new(true)),
            lock: Arc::new(Mutex::new(())),
        };
        // A new mutex is free; holding its guard from here on means a sync or
        // index call cannot slip in before the task starts.
        let Ok(held) = Arc::clone(&catch_up.lock).try_lock_owned() else {
            return;
        };
        started.insert(root.clone(), catch_up.clone());
        drop(started);
        self.catch_up = Some(catch_up.clone());
        let work = (self.catch_up_runner)(root);
        tokio::spawn(async move {
            /// Clears the flag and releases the lock also when the catch-up panics.
            struct Done {
                running: Arc<AtomicBool>,
                _held: tokio::sync::OwnedMutexGuard<()>,
            }
            impl Drop for Done {
                fn drop(&mut self) {
                    self.running.store(false, Ordering::SeqCst);
                }
            }
            let _done = Done {
                running: catch_up.running,
                _held: held,
            };
            work.await;
        });
    }

    /// True while the connect-time catch-up sync is still running.
    pub fn catch_up_running(&self) -> bool {
        self.catch_up
            .as_ref()
            .is_some_and(|c| c.running.load(Ordering::SeqCst))
    }

    /// Waits for a running catch-up before a tool that syncs or indexes the
    /// project itself, so the two do not contend for the project's file lock.
    pub async fn wait_for_catch_up(&self, tool: &str) {
        if !waits_for_catch_up(tool) {
            return;
        }
        if let Some(catch_up) = &self.catch_up {
            drop(catch_up.lock.lock().await);
        }
    }

    /// Marks an `ax_status` reply while the catch-up sync still runs, so an agent
    /// knows the index may not show files changed while no server ran yet.
    pub fn note_catch_up(&self, tool: &str, mut value: serde_json::Value) -> serde_json::Value {
        if tool != "ax_status" || !self.catch_up_running() {
            return value;
        }
        let Some(obj) = value.as_object_mut() else {
            return value;
        };
        obj.insert("catchUp".into(), serde_json::Value::from("running"));
        if let Some(text) = obj.get("text").and_then(|t| t.as_str()) {
            let text = format!(
                "{text}\n### Catch-up sync: in progress\nFiles changed while no ax server ran are still being indexed.\n"
            );
            obj.insert("text".into(), serde_json::Value::String(text));
        }
        value
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
            self.start_catch_up(root);
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

    /// A catch-up that never finishes, like a sync of thousands of files.
    fn stalled_catch_up() -> CatchUpRunner {
        Arc::new(|_root| Box::pin(std::future::pending::<()>()))
    }

    #[tokio::test]
    async fn first_request_does_not_wait_for_the_catch_up_sync() {
        let _env = crate::test_env::shared().await;
        let (_dir, root) = project().await;
        let mut engine = McpEngine::with_project_root(root);
        engine.catch_up_runner = stalled_catch_up();
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            engine.ensure_initialized(),
        )
        .await
        .expect("ensure_initialized waited for the catch-up")
        .expect("initialized");
        assert!(engine.lock_ax().await.is_some());
        assert!(engine.catch_up_running());
    }

    fn no_catch_up() -> CatchUpRunner {
        Arc::new(|_root| Box::pin(async {}))
    }

    #[tokio::test]
    async fn a_policy_tool_first_also_starts_the_catch_up() {
        let _env = crate::test_env::shared().await;
        let (_dir, root) = project().await;
        let mut engine = McpEngine::with_project_root(root);
        engine.catch_up_runner = stalled_catch_up();
        engine.ensure_policy_fresh().await.expect("policy fresh");
        assert!(engine.catch_up_running());
        // A later first non-policy call does not start a second catch-up.
        engine.ensure_initialized().await.expect("initialized");
        assert!(engine.catch_up_started.load(Ordering::SeqCst));
    }

    fn counting_catch_up(count: Arc<std::sync::atomic::AtomicUsize>) -> CatchUpRunner {
        Arc::new(move |_root| {
            count.fetch_add(1, Ordering::SeqCst);
            Box::pin(std::future::pending::<()>())
        })
    }

    #[tokio::test]
    async fn remote_engines_start_no_catch_up() {
        let _env = crate::test_env::shared().await;
        let (_dir, root) = project().await;
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        for _ in 0..4 {
            let mut engine = McpEngine::for_remote(root.clone());
            engine.catch_up_runner = counting_catch_up(Arc::clone(&count));
            engine.ensure_initialized().await.expect("initialized");
            engine.ensure_policy_fresh().await.expect("policy fresh");
            assert!(!engine.catch_up_running());
        }
        assert_eq!(
            count.load(Ordering::SeqCst),
            0,
            "a remote session started a catch-up"
        );
    }

    #[tokio::test]
    async fn engines_of_one_project_share_a_single_catch_up() {
        let _env = crate::test_env::shared().await;
        let (_dir, root) = project().await;
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut engines = Vec::new();
        for _ in 0..4 {
            let mut engine = McpEngine::with_project_root(root.clone());
            engine.catch_up_runner = counting_catch_up(Arc::clone(&count));
            engine.ensure_initialized().await.expect("initialized");
            assert!(engine.catch_up_running());
            engines.push(engine);
        }
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "each engine started its own catch-up"
        );
    }

    #[tokio::test]
    async fn sync_and_index_calls_wait_for_the_catch_up() {
        let _env = crate::test_env::shared().await;
        let (_dir, root) = project().await;
        let mut engine = McpEngine::with_project_root(root);
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let rx = Arc::new(std::sync::Mutex::new(Some(rx)));
        engine.catch_up_runner = Arc::new(move |_root| {
            let rx = rx.lock().unwrap().take();
            Box::pin(async move {
                if let Some(rx) = rx {
                    let _ = rx.await;
                }
            })
        });
        engine.ensure_initialized().await.expect("initialized");
        let short = std::time::Duration::from_millis(200);
        for tool in ["ax_sync", "ax_index"] {
            assert!(
                tokio::time::timeout(short, engine.wait_for_catch_up(tool))
                    .await
                    .is_err(),
                "{tool} did not wait for the running catch-up"
            );
        }
        tokio::time::timeout(short, engine.wait_for_catch_up("ax_search"))
            .await
            .expect("a read-only tool waited for the catch-up");
        tx.send(()).unwrap();
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            engine.wait_for_catch_up("ax_sync"),
        )
        .await
        .expect("ax_sync still waited after the catch-up finished");
        assert!(!engine.catch_up_running());
    }

    #[tokio::test]
    async fn the_catch_up_flag_clears_when_the_sync_finishes() {
        let _env = crate::test_env::shared().await;
        let (_dir, root) = project().await;
        let mut engine = McpEngine::with_project_root(root);
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let rx = Arc::new(std::sync::Mutex::new(Some(rx)));
        engine.catch_up_runner = Arc::new(move |_root| {
            let rx = rx.lock().unwrap().take();
            Box::pin(async move {
                if let Some(rx) = rx {
                    let _ = rx.await;
                }
            })
        });
        engine.ensure_initialized().await.expect("initialized");
        assert!(engine.catch_up_running());
        let status = serde_json::json!({"text": "## ax Status\n"});
        let noted = engine.note_catch_up("ax_status", status.clone());
        assert_eq!(noted["catchUp"], "running");
        assert!(noted["text"]
            .as_str()
            .unwrap()
            .contains("Catch-up sync: in progress"));
        assert_eq!(engine.note_catch_up("ax_search", status.clone()), status);
        tx.send(()).unwrap();
        for _ in 0..100 {
            if !engine.catch_up_running() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(!engine.catch_up_running());
        assert_eq!(engine.note_catch_up("ax_status", status.clone()), status);
    }

    #[tokio::test]
    async fn the_real_catch_up_indexes_files_changed_while_no_server_ran() {
        let _env = crate::test_env::shared().await;
        let (_dir, root) = project().await;
        std::fs::write(root.join("late.rs"), "pub fn written_while_offline() {}\n").unwrap();
        run_catch_up(root.clone()).await;
        let mut ax = Ax::open(&root).await.unwrap();
        let found = crate::tools::ToolHandler::call_tool(
            &mut ax,
            "ax_search",
            serde_json::json!({"query": "written_while_offline"}),
        )
        .await
        .expect("ax_search");
        assert!(
            found.to_string().contains("written_while_offline"),
            "{found}"
        );
    }

    #[tokio::test]
    async fn policy_tool_calls_keep_one_ax_handle() {
        let _env = crate::test_env::shared().await;
        let (_dir, root) = project().await;
        let mut engine = McpEngine::with_project_root(root);
        engine.catch_up_runner = no_catch_up();
        for _ in 0..3 {
            engine.ensure_policy_fresh().await.expect("policy fresh");
        }
        assert_eq!(engine.opens, 1, "policy calls reopened the database");
        assert!(engine.lock_ax().await.is_some());
    }

    #[tokio::test]
    async fn policy_tool_calls_see_rules_saved_by_another_process() {
        let _env = crate::test_env::shared().await;
        let (_dir, root) = project().await;
        let mut engine = McpEngine::with_project_root(root.clone());
        engine.catch_up_runner = no_catch_up();
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
