//! Ax facade - wires all layers together.

mod project_config;

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

pub use project_config::ProjectConfig;

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
                root.clone(),
            ),
            explore_builder: ExploreBuilder::new(
                QueryBuilder::new(pool.clone()),
                GraphTraverser::new(QueryBuilder::new(pool.clone())),
                root.clone(),
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
            self.project_root.clone(),
        );
        self.explore_builder = ExploreBuilder::new(
            QueryBuilder::new(pool.clone()),
            GraphTraverser::new(QueryBuilder::new(pool)),
            self.project_root.clone(),
        );
    }

    /// CG: `reopenIfReplaced` — heal stale DB handle when `.ax/` was recreated (#925).
    pub async fn reopen_if_replaced(&mut self) -> Result<bool, ax_utils::errors::AxError> {
        if !self.db.is_replaced_on_disk() {
            return Ok(false);
        }
        let db_path = self.db.path().to_path_buf();
        let fresh = Database::open(&db_path).await?;
        self.db.close().await;
        self.db = fresh;
        self.wire_layers();
        Ok(true)
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
        let _guard = self.index_mutex.lock().await;
        self.file_lock.acquire()?;
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

    /// Debounced watch loop: re-index files after they stop changing (CG watcher sync).
    pub async fn watch_and_sync(
        &mut self,
        opts: IndexOptions,
        mut on_progress: Option<Box<dyn FnMut(IndexProgress) + Send>>,
    ) -> Result<(), ax_utils::errors::AxError> {
        if !self.is_watching().await {
            self.watch().await?;
        }
        let debounce_ms = WatcherOptions::default().debounce_ms;
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

    pub async fn build_context(
        &self,
        input: TaskInput,
        opts: BuildContextOptions,
    ) -> Result<TaskContext, ax_utils::errors::AxError> {
        self.context_builder.build_context(input, opts).await
    }

    pub async fn explore(
        &self,
        query: &str,
        opts: ExploreOptions,
    ) -> Result<ExploreResult, ax_utils::errors::AxError> {
        self.explore_builder.explore(query, opts).await
    }

    pub async fn get_impact_radius(
        &self,
        node_id: &str,
        depth: u32,
    ) -> Result<ax_types::Subgraph, ax_utils::errors::AxError> {
        self.traverser.get_impact_radius(node_id, depth).await
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

    pub async fn match_policy(
        &self,
        input: ax_policy::MatchInput,
    ) -> Result<ax_policy::MatchResult, ax_utils::errors::AxError> {
        ax_policy::match_policy(self.db.pool(), &input).await
    }

    pub fn policy_exists(&self) -> bool {
        ax_policy::policy_exists(&self.project_root)
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

async fn finalize_after_extract(
    resolver: &mut ReferenceResolver,
    queries: &QueryBuilder,
    db: &Database,
    on_progress: &mut Option<Box<dyn FnMut(IndexProgress) + Send>>,
) -> Result<(), ax_utils::errors::AxError> {
    let resolution = resolver
        .resolve_all(queries, on_progress.as_mut())
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
    db.run_maintenance().await?;
    queries
        .set_metadata("extraction_version", EXTRACTION_VERSION)
        .await?;
    queries
        .set_metadata("package_version", env!("CARGO_PKG_VERSION"))
        .await?;
    Ok(())
}
