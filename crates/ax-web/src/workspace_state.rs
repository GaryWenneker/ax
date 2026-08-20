//! Hot-swappable workspace bundle (graph + policy + ship).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ax_db::queries::QueryBuilder;
use ax_policy::PolicyStore;
use ax_resolution::prune_stale_unresolved_refs;
use ax_ship::ShipDaemon;
use axum::Router;
use sqlx::sqlite::SqlitePool;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;

use crate::agent;
use crate::policy::PolicyApiState;
use crate::ship::ShipApiState;
use crate::sonar_proxy::SonarProxyCache;
use crate::workspace;

#[derive(Clone)]
pub struct WebHub {
    inner: Arc<RwLock<WorkspaceBundle>>,
    pub readonly: bool,
    pub port: u16,
    switching: Arc<AtomicBool>,
    pub(crate) sonar_proxy: Arc<Mutex<SonarProxyCache>>,
}

pub struct WorkspaceBundle {
    pub project_root: PathBuf,
    pub db_path: PathBuf,
    pub graph_pool: SqlitePool,
    pub policy: PolicyApiState,
    pub ship: ShipApiState,
    _watch_task: Option<JoinHandle<()>>,
    _cleanup_task: Option<JoinHandle<()>>,
}

impl WebHub {
    pub async fn open(root: PathBuf, readonly: bool, port: u16) -> Result<Self, String> {
        let bundle = WorkspaceBundle::open(root, readonly).await?;
        Ok(Self {
            inner: Arc::new(RwLock::new(bundle)),
            readonly,
            port,
            switching: Arc::new(AtomicBool::new(false)),
            sonar_proxy: Arc::new(Mutex::new(SonarProxyCache::default())),
        })
    }

    pub async fn read(&self) -> tokio::sync::RwLockReadGuard<'_, WorkspaceBundle> {
        self.inner.read().await
    }

    pub async fn switch(&self, new_root: PathBuf) -> Result<SwitchInfo, String> {
        if self.switching.swap(true, Ordering::SeqCst) {
            return Err("Workspace switch already in progress".into());
        }
        let result = async {
            if self.readonly {
                return Err("Read-only mode".into());
            }
            let new_root = new_root
                .canonicalize()
                .unwrap_or(new_root);
            if !new_root.is_dir() {
                return Err("Not a directory".into());
            }
            if !new_root.join(".ax").join("ax.db").exists() {
                return Err("Project not initialized — run ax init first".into());
            }
            if self.read().await.ship.evaluating.load(Ordering::SeqCst) {
                return Err("Cannot switch while ship evaluation is running".into());
            }
            let new_bundle = WorkspaceBundle::open(new_root.clone(), self.readonly).await?;
            let mut guard = self.inner.write().await;
            let old = std::mem::replace(&mut *guard, new_bundle);
            drop(guard);
            old.close().await;
            let _ = ax_agent::config::touch_recent_project(&new_root, true);
            self.sonar_proxy.lock().await.invalidate();
            let info = SwitchInfo {
                path: crate::workspace::display_path(&new_root),
                label: new_root
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("project")
                    .to_string(),
            };
            crate::actions::publish(
                "workspace",
                format!("switched to {}", info.label),
                Some(serde_json::json!({ "path": info.path })),
            );
            ax_usage::log_workspace(
                Some(&new_root),
                format!("switch path={}", info.path),
            );
            Ok(info)
        }
        .await;
        self.switching.store(false, Ordering::SeqCst);
        result
    }

    pub fn nest_routers(&self, cors: tower_http::cors::CorsLayer) -> Router {
        let hub = self.clone();
        let graph = graph_router(hub.clone());
        // Specific `/api/...` nests MUST be registered before the catch-all `.nest("/api", graph)`.
        // Otherwise paths like `/api/memory/` are swallowed by the graph nest and fall through to the SPA shell.
        Router::new()
            .route(
                "/api/reset-client-cache",
                axum::routing::get(crate::handle_reset_client_cache),
            )
            // `/api/policy/share` before `/api/policy` — otherwise `/policy` swallows `/policy/share/*`.
            .nest(
                "/api/policy/share",
                crate::policy_share::policy_share_router(hub.clone()),
            )
            .nest(
                "/api/auth/microsoft",
                crate::policy_share::microsoft_auth_router(),
            )
            .nest("/api/policy", policy::router_hub(hub.clone()))
            .nest("/api/ship", ship::router_hub(hub.clone()))
            .nest("/api/usage", crate::savings::router(hub.clone()))
            .nest("/api/memory", crate::memory::router_hub(hub.clone()))
            .nest(
                "/api/docs-catalog",
                crate::docs_catalog::router_hub(hub.clone()),
            )
            .nest("/api/okf", crate::okf_api::router_hub(hub.clone()))
            .nest("/api/workspace", workspace::router_hub(hub.clone()))
            .nest("/api/agent", agent::router_hub(hub.clone()))
            .nest("/api/actions", crate::actions::router_hub(hub.clone()))
            .nest("/api/share", crate::share_api::router_hub(hub.clone()))
            .nest("/api/lsp", crate::lsp_api::router_hub(hub.clone()))
            .nest("/api/plugins", crate::plugins_api::router_hub(hub.clone()))
            .nest("/api/ops", crate::mcp_ops::router_hub(hub.clone()))
            .nest("/api", graph)
            .fallback(crate::handle_spa)
            .layer(axum::middleware::from_fn(crate::share_auth::share_token_middleware))
            .layer(cors)
    }
}

pub struct SwitchInfo {
    pub path: String,
    pub label: String,
}

impl WorkspaceBundle {
    pub async fn open(root: PathBuf, readonly: bool) -> Result<Self, String> {
        let db_path = root.join(".ax").join("ax.db");
        if !db_path.exists() {
            return Err(format!(
                "No ax index at {}. Run `ax init` first.",
                db_path.display()
            ));
        }

        let mut graph_opts = ax_db::connect_options(&db_path, false);
        if readonly {
            graph_opts = graph_opts.read_only(true);
        }

        let graph_pool = if readonly {
            SqlitePool::connect_with(graph_opts)
                .await
                .map_err(|e| format!("Failed to open ax.db: {e}"))?
        } else {
            let db = ax_db::Database::open(&db_path)
                .await
                .map_err(|e| format!("Failed to open ax.db: {e}"))?;
            db.pool().clone()
        };

        let policy_pool = ax_policy::open_rw_pool(&db_path)
            .await
            .map_err(|e| e.to_string())?;
        ax_policy::ensure_scaffold(&root.join(".ax")).map_err(|e| e.to_string())?;
        let store = PolicyStore::new(policy_pool, root.clone());
        let _ = store.reindex(false).await;

        let policy = PolicyApiState {
            store: Arc::new(store),
            readonly,
        };

        let ship_daemon = Arc::new(ShipDaemon::new(root.clone()));
        ship_daemon.spawn_sonar_auto_provision();
        let ship = ShipApiState {
            daemon: ship_daemon.clone(),
            report: Arc::new(Mutex::new(None)),
            readonly,
            evaluating: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };

        let watch_daemon = ship_daemon.clone();
        let watch_task = tokio::spawn(async move {
            let _ = watch_daemon.run_watch().await;
        });

        let cleanup_pool = graph_pool.clone();
        let cleanup_task = if readonly {
            None
        } else {
            Some(tokio::spawn(async move {
                spawn_unresolved_cleanup_inner(cleanup_pool).await;
            }))
        };

        if !readonly {
            let seed_pool = graph_pool.clone();
            let seed_root = root.clone();
            tokio::spawn(async move {
                seed_memories_if_empty(seed_pool, seed_root).await;
            });
        }

        Ok(Self {
            project_root: root,
            db_path,
            graph_pool,
            policy,
            ship,
            _watch_task: Some(watch_task),
            _cleanup_task: cleanup_task,
        })
    }

    pub async fn close(self) {
        if let Some(h) = self._watch_task {
            h.abort();
        }
        if let Some(h) = self._cleanup_task {
            h.abort();
        }
        self.graph_pool.close().await;
    }
}

async fn seed_memories_if_empty(pool: SqlitePool, project_root: PathBuf) {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memories")
        .fetch_one(&pool)
        .await
        .unwrap_or(-1);
    if count != 0 {
        return;
    }
    if project_root.join(".git").exists() {
        match ax_memory::capture_git_history(&pool, &project_root, 100).await {
            Ok(r) => {
                tracing::info!(
                    captured = r.captured,
                    skipped = r.skipped_trivial,
                    "auto-seeded memory vault from git history"
                );
                return;
            }
            Err(e) => tracing::warn!("git memory seed failed, falling back to graph: {e}"),
        }
    }
    match ax_memory::seed_from_graph(&pool).await {
        Ok(n) if n > 0 => tracing::info!(captured = n, "auto-seeded memory vault from knowledge graph"),
        Ok(_) => {}
        Err(e) => tracing::warn!("graph memory seed failed: {e}"),
    }
}

async fn spawn_unresolved_cleanup_inner(pool: SqlitePool) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(30 * 60));
    interval.tick().await;
    loop {
        interval.tick().await;
        let queries = QueryBuilder::new(pool.clone());
        let _ = prune_stale_unresolved_refs(&queries).await;
    }
}

fn graph_router(hub: WebHub) -> Router {
    use axum::routing::{get, post};
    Router::new()
        .route("/stats", get(crate::handle_stats))
        .route("/version", get(crate::handle_version))
        .route("/nodes", get(crate::handle_nodes))
        .route("/node/{id}", get(crate::handle_node))
        .route("/graph", get(crate::handle_graph))
        .route("/graph/stream", get(crate::handle_graph_stream))
        .route("/graph/export", get(crate::graph_export::handle_export))
        .route("/insights", get(crate::handle_insights))
        .route(
            "/domain-graph",
            get(crate::domain_graph::handle_get).put(crate::domain_graph::handle_put),
        )
        .route("/files", get(crate::handle_files))
        .route("/files/roots", get(crate::handle_file_roots))
        .route("/search", get(crate::handle_search))
        .route("/source", get(crate::handle_source))
        .route("/unresolved", get(crate::handle_unresolved))
        .route("/unresolved/summary", get(crate::handle_unresolved_summary))
        .route("/unresolved/reconcile", post(crate::handle_unresolved_reconcile))
        .with_state(hub)
}

// Re-export policy module for router_hub - will add to policy.rs
use crate::policy;
use crate::ship;
