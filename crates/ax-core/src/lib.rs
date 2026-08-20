//! Ax facade - wires all layers together.

mod project_config;
pub mod okf;
pub mod report;
pub mod stats_format;
pub mod workspace;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ax_context::builder::ContextBuilder;
use ax_context::directory::{get_ax_dir, is_initialized};
use ax_context::explore::ExploreBuilder;
use ax_db::queries::QueryBuilder;
use ax_db::{Database, DB_FILENAME};
use ax_extraction::orchestrator::{ExtractionOrchestrator, IndexOptions, IndexResult};
use ax_extraction::EXTRACTION_VERSION;
use ax_graph::query_parser::parse_query;
use ax_graph::query_utils::matches_parsed_query;
use ax_graph::queries::GraphQueryManager;
use ax_graph::traversal::GraphTraverser;
use ax_resolution::ReferenceResolver;
use ax_sync::watcher::{FileWatcher, WatcherOptions};
use ax_types::{
    BuildContextOptions, ExploreOptions, ExploreResult, GraphStats, IndexPhase, IndexProgress,
    PendingFile, SearchOptions, SearchResult, TaskContext, TaskInput,
};
use ax_utils::file_lock::FileLock;
use ax_utils::mutex::AsyncMutex;

pub use okf::{
    export_okf_bundle, publish_okf_wiki, validate_okf_bundle, OkfConfig, OkfExportOptions,
    OkfExportReport, OkfPublishOptions, OkfPublishReport, OkfValidateReport, OkfWikiConfig,
};
pub use project_config::ProjectConfig;
pub use workspace::{
    discover_members, find_workspace_root, load_workspace_config, member_roots,
    write_workspace_config, WorkspaceConfig, WorkspaceMember,
};

/// How long a *read* waits for the in-process index mutex before giving up and
/// serving stale-labelled source. Reads must stay bounded; a graph query is on
/// the agent's critical path and an index can run for minutes.
const SOURCE_RESYNC_MUTEX_WAIT: std::time::Duration = std::time::Duration::from_millis(750);

/// Same idea for the cross-process `.ax/ax.lock`: try briefly, never camp on it.
const SOURCE_RESYNC_FILE_LOCK_WAIT: std::time::Duration = std::time::Duration::from_millis(750);

pub struct Ax {
    db: Database,
    queries: QueryBuilder,
    project_root: PathBuf,
    config: ProjectConfig,
    orchestrator: ExtractionOrchestrator,
    resolver: ReferenceResolver,
    graph_manager: GraphQueryManager,
    traverser: GraphTraverser,
    context_builder: ContextBuilder,
    explore_builder: ExploreBuilder,
    index_mutex: Arc<AsyncMutex<()>>,
    file_lock: FileLock,
    watcher: Option<FileWatcher>,
}

impl Ax {
    pub async fn init(root: &Path) -> Result<Self, ax_utils::errors::AxError> {
        let root = root.canonicalize().map_err(|e| {
            ax_utils::errors::AxError::File(ax_utils::errors::FileError::with_path(e.to_string(), root.display().to_string()))
        })?;
        let ax_dir = get_ax_dir(&root);
        std::fs::create_dir_all(&ax_dir).map_err(|e| {
            ax_utils::errors::AxError::File(ax_utils::errors::FileError::with_path(e.to_string(), ax_dir.display().to_string()))
        })?;
        let db_path = ax_dir.join(DB_FILENAME);
        ax_policy::ensure_scaffold(&ax_dir).map_err(|e| {
            ax_utils::errors::AxError::File(ax_utils::errors::FileError::with_path(
                e.to_string(),
                ax_dir.display().to_string(),
            ))
        })?;
        let db = Database::open(&db_path).await?;
        Self::from_db(root, db).await
    }

    pub async fn open(root: &Path) -> Result<Self, ax_utils::errors::AxError> {
        let root = root.canonicalize().map_err(|e| {
            ax_utils::errors::AxError::File(ax_utils::errors::FileError::with_path(e.to_string(), root.display().to_string()))
        })?;
        if !is_initialized(&root) {
            return Err(ax_utils::errors::AxError::Other(
                "project not initialized - run ax init".to_string(),
            ));
        }
        let db_path = get_ax_dir(&root).join(DB_FILENAME);
        let db = Database::open(&db_path).await?;
        Self::from_db(root, db).await
    }

    async fn from_db(project_root: PathBuf, db: Database) -> Result<Self, ax_utils::errors::AxError> {
        let root = project_root.clone();
        let config = ProjectConfig::load(&root);
        let ax_dir = get_ax_dir(&root);
        ax_utils::clear_stale_lock(&ax_dir.join("ax.lock"));
        let file_lock = FileLock::new(&ax_dir);
        let pool = db.pool().clone();
        let mut ax = Self {
            db,
            queries: QueryBuilder::new(pool.clone()),
            project_root: root.clone(),
            config,
            orchestrator: ExtractionOrchestrator::new(root.clone()),
            resolver: ReferenceResolver::new(&root),
            graph_manager: GraphQueryManager::new(QueryBuilder::new(pool.clone())),
            traverser: GraphTraverser::new(QueryBuilder::new(pool.clone())),
            context_builder: ContextBuilder::new(
                QueryBuilder::new(pool.clone()),
                GraphTraverser::new(QueryBuilder::new(pool.clone())),
            ),
            explore_builder: ExploreBuilder::new(
                QueryBuilder::new(pool.clone()),
                GraphTraverser::new(QueryBuilder::new(pool.clone())),
            ),
            index_mutex: Arc::new(AsyncMutex::new(())),
            file_lock,
            watcher: None,
        };
        ax.wire_layers();
        Ok(ax)
    }
    fn wire_layers(&mut self) {
        let pool = self.db.pool().clone();
        self.queries = QueryBuilder::new(pool.clone());
        self.traverser = GraphTraverser::new(QueryBuilder::new(pool.clone()));
        self.graph_manager = GraphQueryManager::new(QueryBuilder::new(pool.clone()));
        self.orchestrator = ExtractionOrchestrator::new(self.project_root.clone());
        self.resolver = ReferenceResolver::new(&self.project_root);
        self.context_builder = ContextBuilder::new(
            QueryBuilder::new(pool.clone()),
            GraphTraverser::new(QueryBuilder::new(pool.clone())),
        );
        self.explore_builder = ExploreBuilder::new(
            QueryBuilder::new(pool.clone()),
            GraphTraverser::new(QueryBuilder::new(pool)),
        );
    }

    /// CG: `reopenIfReplaced` — heal stale DB handle when `.ax/` was recreated (#925).
    pub async fn reopen_if_replaced(&mut self) -> Result<bool, ax_utils::errors::AxError> {
        if !self.db.is_replaced_on_disk() {
            return Ok(false);
        }
        self.reopen_db().await?;
        Ok(true)
    }

    /// Reopen SQLite from disk — picks up WAL commits from other processes (CLI, ax web).
    pub async fn reopen_db(&mut self) -> Result<(), ax_utils::errors::AxError> {
        let db_path = self.db.path().to_path_buf();
        let fresh = Database::open(&db_path).await?;
        self.db.close().await;
        self.db = fresh;
        self.wire_layers();
        Ok(())
    }

    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    pub fn config(&self) -> &ProjectConfig {
        &self.config
    }

    pub async fn index_all(
        &mut self,
        opts: IndexOptions,
        mut on_progress: Option<Box<dyn FnMut(IndexProgress) + Send>>,
    ) -> Result<IndexResult, ax_utils::errors::AxError> {
        let _guard = self.index_mutex.lock().await;
        self.file_lock.acquire()?;
        let index_opts = self.merge_index_opts(&opts);
        let result = self
            .orchestrator
            .index_all(&self.queries, &index_opts, on_progress.as_mut())
            .await;
        let result = match result {
            Ok(result) => {
                finalize_after_extract(
                    &mut self.resolver,
                    &self.queries,
                    &self.db,
                    &self.project_root,
                    &index_opts.exclude,
                    None,
                    &mut on_progress,
                )
                .await?;
                Ok(result)
            }
            Err(e) => Err(e),
        };
        let _ = self.file_lock.release();
        if result.is_ok() {
            let _ = ax_policy::index_policy(self.db.pool(), &self.project_root, false).await;
        }
        result
    }

    pub async fn sync(
        &mut self,
        opts: IndexOptions,
        mut on_progress: Option<Box<dyn FnMut(IndexProgress) + Send>>,
    ) -> Result<IndexResult, ax_utils::errors::AxError> {
        // A different extractor version means the whole graph is stale —
        // incremental sync would silently keep old nodes, so reindex fully.
        let stored_version = self
            .queries
            .get_metadata("extraction_version")
            .await
            .unwrap_or(None);
        if stored_version.as_deref() != Some(EXTRACTION_VERSION) {
            return self.index_all(opts, on_progress).await;
        }
        let _guard = self.index_mutex.lock().await;
        self.file_lock.acquire()?;
        let index_opts = self.merge_index_opts(&opts);
        let result = self
            .orchestrator
            .sync_changed(&self.queries, &index_opts, on_progress.as_mut())
            .await;
        let result = match result {
            Ok(sync) => {
                if sync.had_changes() {
                    finalize_after_extract(
                        &mut self.resolver,
                        &self.queries,
                        &self.db,
                        &self.project_root,
                        &index_opts.exclude,
                        Some(&sync.affected_files),
                        &mut on_progress,
                    )
                    .await?;
                }
                Ok(IndexResult {
                    files_indexed: sync.files_indexed + sync.files_removed,
                    duration_ms: sync.duration_ms,
                })
            }
            Err(e) => Err(e),
        };
        let _ = self.file_lock.release();
        if result.is_ok() {
            let _ = ax_policy::index_policy(self.db.pool(), &self.project_root, false).await;
        }
        result
    }

    fn merge_index_opts(&self, opts: &IndexOptions) -> IndexOptions {
        IndexOptions {
            force: opts.force,
            quiet: opts.quiet,
            custom_extensions: self.config.extensions.clone(),
            exclude: self.config.exclude.clone(),
        }
    }

    /// CG: `indexFiles` — incremental re-index for changed paths only.
    pub async fn index_files(
        &mut self,
        paths: &[String],
        opts: IndexOptions,
        on_progress: &mut Option<Box<dyn FnMut(IndexProgress) + Send>>,
    ) -> Result<IndexResult, ax_utils::errors::AxError> {
        if paths.is_empty() {
            return Ok(IndexResult {
                files_indexed: 0,
                duration_ms: 0,
            });
        }
        // Lock through a cloned Arc so the guard borrows the local, leaving
        // `&mut self` available for the indexing call below.
        let mutex = Arc::clone(&self.index_mutex);
        let _guard = mutex.lock().await;
        self.index_files_locked(paths, opts, on_progress, ax_utils::file_lock::DEFAULT_LOCK_WAIT)
            .await
    }

    /// Re-index `paths`, assuming the caller already holds `index_mutex`.
    ///
    /// `lock_wait` bounds the on-disk lock acquisition so a read-path caller
    /// cannot hang behind another process's long index.
    async fn index_files_locked(
        &mut self,
        paths: &[String],
        opts: IndexOptions,
        on_progress: &mut Option<Box<dyn FnMut(IndexProgress) + Send>>,
        lock_wait: std::time::Duration,
    ) -> Result<IndexResult, ax_utils::errors::AxError> {
        self.file_lock.acquire_wait(lock_wait)?;
        let index_opts = self.merge_index_opts(&opts);
        let result = self
            .orchestrator
            .index_files(&self.queries, paths, &index_opts, on_progress.as_mut())
            .await;
        let result = match result {
            Ok(result) => {
                finalize_after_extract(
                    &mut self.resolver,
                    &self.queries,
                    &self.db,
                    &self.project_root,
                    &index_opts.exclude,
                    Some(paths),
                    on_progress,
                )
                .await?;
                Ok(result)
            }
            Err(e) => Err(e),
        };
        let _ = self.file_lock.release();
        result
    }

    /// Re-index paths whose stored source drifted from the working tree, so the
    /// next graph read can serve verified-fresh snippets.
    ///
    /// Best-effort and strictly bounded: if either lock is busy, or the re-index
    /// fails, this returns `false` and the caller serves stale-labelled text.
    /// Blocking here would turn a read into an unbounded wait, and reads are on
    /// the agent's critical path.
    async fn try_resync_source(&mut self, paths: &[String]) -> bool {
        if paths.is_empty() {
            return false;
        }
        let mutex = Arc::clone(&self.index_mutex);
        let guard = match tokio::time::timeout(SOURCE_RESYNC_MUTEX_WAIT, mutex.lock()).await {
            Ok(g) => g,
            Err(_) => {
                tracing::debug!("source resync skipped: index busy");
                return false;
            }
        };
        let mut no_progress: Option<Box<dyn FnMut(IndexProgress) + Send>> = None;
        let outcome = self
            .index_files_locked(
                paths,
                IndexOptions {
                    quiet: true,
                    ..IndexOptions::default()
                },
                &mut no_progress,
                SOURCE_RESYNC_FILE_LOCK_WAIT,
            )
            .await;
        drop(guard);
        match outcome {
            Ok(_) => true,
            Err(e) => {
                tracing::debug!("source resync failed: {e}");
                false
            }
        }
    }

    /// Spawn a background task that watches the project and incrementally re-indexes changed files.
    /// Coordinates with other index operations via the on-disk file lock.
    pub fn spawn_background_watch(project_root: PathBuf) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ax = match Ax::open(&project_root).await {
                Ok(ax) => ax,
                Err(e) => {
                    tracing::warn!("background watch: could not open project: {}", e);
                    return;
                }
            };
            let opts = IndexOptions {
                quiet: true,
                ..IndexOptions::default()
            };
            if let Err(e) = ax.watch_and_sync(opts, None).await {
                tracing::warn!("background watch stopped: {}", e);
            }
        })
    }

    /// Debounced watch loop: re-index files after they stop changing (CG watcher sync).
    pub async fn watch_and_sync(
        &mut self,
        opts: IndexOptions,
        mut on_progress: Option<Box<dyn FnMut(IndexProgress) + Send>>,
    ) -> Result<(), ax_utils::errors::AxError> {
        if !self.is_watching().await {
            self.watch().await?;
        }
        let debounce_ms = ax_sync::watcher::resolve_watch_debounce_ms();
        let poll_ms = 200u64;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(poll_ms)).await;
            if self.is_indexing().await {
                continue;
            }
            let ready = if let Some(w) = &self.watcher {
                w.get_ready_files(debounce_ms).await
            } else {
                vec![]
            };
            if ready.is_empty() {
                continue;
            }
            if let Some(w) = &self.watcher {
                w.mark_indexing(&ready).await;
            }
            match self.index_files(&ready, opts.clone(), &mut on_progress).await {
                Ok(r) => {
                    if !opts.quiet {
                        tracing::info!("auto-sync: {} file(s) in {}ms", r.files_indexed, r.duration_ms);
                    }
                }
                Err(e) => tracing::warn!("auto-sync failed: {}", e),
            }
            if let Some(w) = &self.watcher {
                w.clear_pending(&ready).await;
            }
        }
    }

    pub async fn is_indexing(&self) -> bool {
        self.index_mutex.try_lock().is_err()
    }

    pub async fn watch(&mut self) -> Result<(), ax_utils::errors::AxError> {
        let mut watcher = FileWatcher::new(self.project_root.clone());
        watcher.start(WatcherOptions::default()).await?;
        self.watcher = Some(watcher);
        Ok(())
    }

    pub async fn unwatch(&mut self) {
        if let Some(mut w) = self.watcher.take() {
            w.stop().await;
        }
    }

    pub async fn is_watching(&self) -> bool {
        match &self.watcher {
            Some(w) => w.is_active().await,
            None => false,
        }
    }

    pub async fn get_pending_files(&self) -> Vec<PendingFile> {
        // Prefer the process-global registry so MCP tool Ax instances see
        // pending files owned by the background watcher Ax.
        let global = ax_sync::global_pending_files(&self.project_root);
        if !global.is_empty() {
            return global;
        }
        if let Some(w) = &self.watcher {
            w.get_pending_files().await
        } else {
            vec![]
        }
    }

    pub async fn get_stats(&self) -> Result<GraphStats, ax_utils::errors::AxError> {
        self.queries.get_stats().await
    }

    pub async fn get_last_indexed_at(&self) -> Result<i64, ax_utils::errors::AxError> {
        self.queries.get_last_indexed_at().await
    }

    pub async fn search_nodes(
        &self,
        query: &str,
        opts: &SearchOptions,
    ) -> Result<Vec<SearchResult>, ax_utils::errors::AxError> {
        let parsed = parse_query(query);
        let mut merged = opts.clone();
        if !parsed.kinds.is_empty() {
            merged.kinds = Some(parsed.kinds.clone());
        }
        if !parsed.languages.is_empty() {
            merged.languages = Some(parsed.languages.clone());
        }
        if !parsed.path_filters.is_empty() {
            merged.include_patterns = Some(parsed.path_filters.clone());
        }
        let results = self.queries.search_nodes(&parsed.text, &merged).await?;
        Ok(results
            .into_iter()
            .filter(|r| matches_parsed_query(&r.node, &parsed))
            .collect())
    }

    pub async fn get_node(&self, id: &str) -> Result<Option<ax_types::Node>, ax_utils::errors::AxError> {
        self.queries.get_node_by_id(id).await
    }

    /// Task context from the graph, with one bounded freshness repair.
    ///
    /// Code blocks come from the source store. If any entry point's stored source
    /// drifted from the working tree, re-index those files once and rebuild —
    /// never more than once, so a file that keeps failing cannot spin.
    pub async fn build_context(
        &mut self,
        input: TaskInput,
        opts: BuildContextOptions,
    ) -> Result<TaskContext, ax_utils::errors::AxError> {
        let ctx = self
            .context_builder
            .build_context(input.clone(), opts.clone())
            .await?;
        let paths: Vec<String> = ctx
            .code_blocks
            .iter()
            .map(|b| b.file_path.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        let stale = self.queries.stale_source_paths(&paths).await?;
        if stale.is_empty() || !self.try_resync_source(&stale).await {
            return Ok(ctx);
        }
        self.context_builder.build_context(input, opts).await
    }

    /// Explore the graph, with one bounded freshness repair.
    ///
    /// Snippets are served from the source store (never the working tree). A
    /// hash mismatch triggers a single scoped re-index of the affected files and
    /// one retry; if the index is busy the first result stands, with its stale
    /// labels intact.
    pub async fn explore(
        &mut self,
        query: &str,
        opts: ExploreOptions,
    ) -> Result<ExploreResult, ax_utils::errors::AxError> {
        let result = self.explore_builder.explore(query, opts.clone()).await?;
        let stale = self.explore_builder.stale_files(&result).await?;
        if stale.is_empty() || !self.try_resync_source(&stale).await {
            return Ok(result);
        }
        self.explore_builder.explore(query, opts).await
    }

    pub async fn get_impact_radius(
        &self,
        node_id: &str,
        depth: u32,
    ) -> Result<ax_types::Subgraph, ax_utils::errors::AxError> {
        self.traverser.get_impact_radius(node_id, depth).await
    }

    /// Whole-graph insights: Leiden communities, god nodes, and surprising
    /// cross-community connections. Persists community assignments as a side
    /// effect so later queries and the web API can reuse them.
    pub async fn insights(
        &self,
        resolution: f64,
        god_limit: usize,
        surprising_limit: usize,
    ) -> Result<ax_graph::GraphInsights, ax_utils::errors::AxError> {
        self.graph_manager
            .compute_insights(resolution, god_limit, surprising_limit)
            .await
    }

    /// Non-exported symbols with no inbound references (best-effort dead code).
    pub async fn find_dead_code(&self) -> Result<Vec<ax_types::Node>, ax_utils::errors::AxError> {
        self.graph_manager.find_dead_code().await
    }

    /// Render a full Markdown architecture report from the current graph.
    pub async fn architecture_report(
        &self,
        resolution: f64,
    ) -> Result<String, ax_utils::errors::AxError> {
        let insights = self.insights(resolution, 15, 15).await?;
        let dead_code = self.find_dead_code().await?;
        let unresolved = self.queries.get_unresolved_refs().await?;
        let names: Vec<String> = unresolved.into_iter().map(|u| u.reference_name).collect();
        let (unresolved_total, unresolved_top) = report::summarize_unresolved(&names, 20);
        let project_name = self
            .project_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project");
        let input = report::ReportInput {
            project_name,
            insights: &insights,
            dead_code: &dead_code,
            unresolved_total,
            unresolved_top: &unresolved_top,
        };
        Ok(report::render_architecture_report(&input))
    }

    pub async fn get_callers(
        &self,
        node_id: &str,
        depth: u32,
    ) -> Result<Vec<ax_types::Node>, ax_utils::errors::AxError> {
        self.traverser.get_callers(node_id, depth).await
    }

    pub async fn get_callees(
        &self,
        node_id: &str,
        depth: u32,
    ) -> Result<Vec<ax_types::Node>, ax_utils::errors::AxError> {
        self.traverser.get_callees(node_id, depth).await
    }

    /// Call-graph cycles (non-trivial SCCs on Calls/References edges).
    pub async fn find_cycles(
        &self,
        limit: usize,
    ) -> Result<Vec<ax_graph::CallCycle>, ax_utils::errors::AxError> {
        let edges = self.queries.get_all_edges().await?;
        // Over-fetch then drop cycles that only live in bundles/dist.
        let fetch = if limit == 0 { 0 } else { limit.saturating_mul(8).max(64) };
        let mut cycles = ax_graph::find_call_cycles(&edges, fetch);
        let mut kept = Vec::new();
        for c in cycles.drain(..) {
            let mut bundleish = false;
            for id in &c.nodes {
                if let Ok(Some(n)) = self.get_node(id).await {
                    let fp = n.file_path.replace('\\', "/").to_ascii_lowercase();
                    if fp.contains("/dist/")
                        || fp.contains("/node_modules/")
                        || fp.contains("/vendor/")
                        || fp.contains(".min.js")
                    {
                        bundleish = true;
                        break;
                    }
                }
            }
            if !bundleish {
                kept.push(c);
            }
            if limit > 0 && kept.len() >= limit {
                break;
            }
        }
        Ok(kept)
    }

    /// Shortest Calls/References path between two node ids.
    pub async fn find_path(
        &self,
        from_id: &str,
        to_id: &str,
    ) -> Result<Option<Vec<String>>, ax_utils::errors::AxError> {
        let edges = self.queries.get_all_edges().await?;
        Ok(ax_graph::shortest_call_path(&edges, from_id, to_id))
    }

    /// Public API surface for a module/path prefix.
    ///
    /// Matches exported symbols (`is_exported`) or `Visibility::Public`, whose
    /// file path / qualified name contains the module needle (e.g. `ax-mcp`
    /// or `crates/ax-mcp`).
    pub async fn module_api(
        &self,
        module: &str,
        limit: usize,
    ) -> Result<Vec<ax_types::Node>, ax_utils::errors::AxError> {
        use ax_types::{NodeKind, Visibility};

        let needle = module.trim().trim_matches('/').replace('\\', "/");
        let needle_l = needle.to_ascii_lowercase();
        let nodes = self.queries.get_all_nodes().await?;
        let in_module = |n: &ax_types::Node| {
            let fp = n.file_path.replace('\\', "/").to_ascii_lowercase();
            let qn = n.qualified_name.replace('\\', "/").to_ascii_lowercase();
            // Skip build artifacts / bundles.
            if fp.contains("/dist/") || fp.contains("\\dist\\") || fp.contains("/node_modules/") {
                return false;
            }
            path_matches_module(&fp, &needle_l) || path_matches_module(&qn, &needle_l)
        };
        let is_api_kind = |n: &ax_types::Node| {
            matches!(
                n.kind,
                NodeKind::Function
                    | NodeKind::Method
                    | NodeKind::Struct
                    | NodeKind::Class
                    | NodeKind::Trait
                    | NodeKind::Interface
                    | NodeKind::Enum
                    | NodeKind::TypeAlias
                    | NodeKind::Module
                    | NodeKind::Component
                    | NodeKind::Route
            )
        };
        let is_public = |n: &ax_types::Node| {
            n.is_exported.unwrap_or(false) || matches!(n.visibility, Some(Visibility::Public))
        };

        let mut out: Vec<_> = nodes
            .iter()
            .filter(|n| in_module(n) && is_api_kind(n) && is_public(n))
            .cloned()
            .collect();
        // Many extractors leave is_exported unset/false for Rust `pub` items.
        // Fall back to API-kind symbols under the module path.
        if out.is_empty() {
            out = nodes
                .into_iter()
                .filter(|n| in_module(n) && is_api_kind(n))
                .collect();
        }
        out.sort_by(|a, b| a.qualified_name.cmp(&b.qualified_name));
        if limit > 0 && out.len() > limit {
            out.truncate(limit);
        }
        Ok(out)
    }

    pub async fn clear(&mut self) -> Result<(), ax_utils::errors::AxError> {
        self.queries.clear_all().await
    }

    pub async fn destroy(&mut self) -> Result<(), ax_utils::errors::AxError> {
        self.unwatch().await;
        self.file_lock.release()?;
        self.db.close().await;
        Ok(())
    }

    pub async fn get_affected_files(
        &self,
        changed_files: &[String],
    ) -> Result<Vec<String>, ax_utils::errors::AxError> {
        use std::collections::HashSet;
        use ax_graph::query_utils::is_test_file;

        let mut affected = HashSet::new();
        for path in changed_files {
            if is_test_file(path) {
                affected.insert(path.clone());
            }
            let nodes = self.queries.get_nodes_by_file(path).await?;
            for node in nodes {
                let sg = self.traverser.get_impact_radius(&node.id, 2).await?;
                for n in sg.nodes.values() {
                    if is_test_file(&n.file_path) {
                        affected.insert(n.file_path.clone());
                    }
                }
            }
        }
        Ok(affected.into_iter().collect())
    }

    pub fn queries(&self) -> &QueryBuilder {
        &self.queries
    }

    pub fn db_pool(&self) -> &sqlx::SqlitePool {
        self.db.pool()
    }

    pub async fn index_policy(&self, force: bool) -> Result<ax_policy::PolicyIndexResult, ax_utils::errors::AxError> {
        ax_policy::index_policy(self.db.pool(), &self.project_root, force).await
    }

    pub async fn ensure_policy_ready(&self) -> Result<ax_policy::PolicyIndexResult, ax_utils::errors::AxError> {
        ax_policy::ensure_policy_ready(self.db.pool(), &self.project_root).await
    }

    pub async fn policy_status(&self) -> Result<ax_policy::PolicyStatus, ax_utils::errors::AxError> {
        ax_policy::policy_status(self.db.pool(), &self.project_root).await
    }

    pub async fn match_policy(
        &self,
        input: ax_policy::MatchInput,
    ) -> Result<ax_policy::MatchResult, ax_utils::errors::AxError> {
        ax_policy::match_policy(self.db.pool(), &input).await
    }

    pub fn policy_exists(&self) -> bool {
        ax_policy::policy_tools_enabled(&self.project_root)
    }

    pub async fn guard_operation(
        &self,
        path: &Path,
        op: ax_policy::GuardOp,
        content: Option<&[u8]>,
    ) -> Result<ax_policy::GuardResult, ax_utils::errors::AxError> {
        ax_policy::guard_operation(self.db.pool(), &self.project_root, path, op, content).await
    }
}

fn path_matches_module(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    if haystack == needle
        || haystack.starts_with(&format!("{needle}/"))
        || haystack.starts_with(&format!("{needle}::"))
        || haystack.contains(&format!("/{needle}/"))
        || haystack.contains(&format!("/{needle}::"))
        || haystack.contains(&format!("/{needle}"))
        || haystack.contains(&format!("::{needle}"))
        || haystack.contains(&format!("::{needle}::"))
        || haystack.ends_with(&format!("/{needle}"))
    {
        return true;
    }
    // Bare crate name → crates/<name>/
    if !needle.contains('/') {
        let under_crates = format!("crates/{needle}/");
        let under_crates_exact = format!("crates/{needle}");
        if haystack.contains(&under_crates) || haystack.contains(&under_crates_exact) {
            return true;
        }
    }
    false
}

async fn finalize_after_extract(
    resolver: &mut ReferenceResolver,
    queries: &QueryBuilder,
    db: &Database,
    project_root: &Path,
    exclude: &[String],
    scope: ax_resolution::ResolutionScope<'_>,
    on_progress: &mut Option<Box<dyn FnMut(IndexProgress) + Send>>,
) -> Result<(), ax_utils::errors::AxError> {
    let _ = ax_resolution::prune_stale_unresolved_refs(queries).await?;
    let resolution = resolver
        .resolve_all(queries, scope, on_progress.as_mut())
        .await?;
    if let Some(cb) = on_progress.as_mut() {
        cb(IndexProgress {
            phase: IndexPhase::Optimizing,
            current: 1,
            total: 1,
            file_path: Some("SQLite maintenance".into()),
        });
    }
    queries
        .set_metadata("resolution_total", &resolution.stats.total.to_string())
        .await?;
    queries
        .set_metadata("resolution_resolved", &resolution.stats.resolved.to_string())
        .await?;
    queries
        .set_metadata("resolution_unresolved", &resolution.stats.unresolved.to_string())
        .await?;
    let docs_indexed =
        ax_extraction::markdown::index_markdown(project_root, queries, exclude).await?;
    queries
        .set_metadata("docs_indexed", &docs_indexed.to_string())
        .await?;
    let contracts_indexed =
        ax_extraction::contracts::index_contracts(project_root, queries, exclude).await?;
    queries
        .set_metadata("contracts_indexed", &contracts_indexed.to_string())
        .await?;
    db.run_maintenance().await?;
    queries
        .set_metadata("extraction_version", EXTRACTION_VERSION)
        .await?;
    queries
        .set_metadata("package_version", env!("CARGO_PKG_VERSION"))
        .await?;
    Ok(())
}
