//! Extraction orchestrator — scan, parse, persist.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use ax_db::queries::QueryBuilder;
use ax_types::{ExtractionError, ExtractionResult, ExtractionSeverity, FileRecord, IndexPhase, IndexProgress, Language};
use blake3::hash;
use ignore::gitignore::Gitignore;
use ignore::WalkBuilder;

use crate::generated_detection::is_generated;
use crate::grammars::{extension_map, language_for_extension};
use crate::parse_pool::{ParsePool, ParseTask};

/// Path segments always skipped during scan (even when not gitignored).
const BUILTIN_SKIP_DIRS: &[&str] = &[
    "node_modules", "target", "dist", "build", ".git", "vendor", "tmp", "temp", ".ax",
    ".fastembed_cache",
];

#[derive(Clone)]
pub struct IndexOptions {
    pub force: bool,
    pub quiet: bool,
    pub custom_extensions: HashMap<String, Language>,
    pub exclude: Vec<String>,
}

impl Default for IndexOptions {
    fn default() -> Self {
        Self {
            force: false,
            quiet: false,
            custom_extensions: HashMap::new(),
            exclude: Vec::new(),
        }
    }
}

pub struct ExtractionOrchestrator {
    project_root: PathBuf,
    parse_pool: ParsePool,
}

impl ExtractionOrchestrator {
    pub fn new(project_root: PathBuf) -> Self {
        Self { project_root, parse_pool: ParsePool::new() }
    }

    /// Run out-of-tree plugins first, then tree-sitter parse pool.
    fn parse_with_plugins(
        &self,
        tasks: Vec<ParseTask>,
        on_progress: &mut Option<&mut Box<dyn FnMut(IndexProgress) + Send>>,
    ) -> (Vec<(String, Result<ExtractionResult, String>)>, u32) {
        let plugins = ax_plugins::load_plugins(&self.project_root);
        let mut results: Vec<(String, Result<ExtractionResult, String>)> =
            Vec::with_capacity(tasks.len());
        let mut remaining = Vec::new();
        for task in tasks {
            if !plugins.is_empty() {
                if let Some((plugin_name, plugin_res)) = plugins.extract(&task.file_path, &task.content) {
                    let ok = plugin_res.is_ok();
                    let ext = std::path::Path::new(&task.file_path)
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("");
                    let status = if ok { "ok" } else { "fail" };
                    ax_usage::log_plugin(
                        Some(&self.project_root),
                        format!(
                            "extract name={plugin_name} ext=.{ext} path={} {status}",
                            task.file_path
                        ),
                    );
                    tracing::debug!(
                        path = %task.file_path,
                        ok,
                        "plugin extract"
                    );
                    results.push((task.file_path, plugin_res.map_err(|e| e.to_string())));
                    continue;
                }
            }
            remaining.push(task);
        }
        let mut tasks = remaining;
        let parse_total = (results.len() + tasks.len()) as u32;
        const PARSE_CHUNK: usize = 48;
        let mut parsed_count = results.len() as u32;
        while !tasks.is_empty() {
            let chunk_len = PARSE_CHUNK.min(tasks.len());
            let chunk: Vec<ParseTask> = tasks.drain(..chunk_len).collect();
            let parsed = self.parse_pool.parse_batch(chunk);
            parsed_count += parsed.len() as u32;
            if let Some(ref mut cb) = on_progress {
                cb(IndexProgress {
                    phase: IndexPhase::Parsing,
                    current: parsed_count,
                    total: parse_total.max(1),
                    file_path: None,
                });
            }
            results.extend(parsed);
        }
        (results, parse_total)
    }

    /// Whether any parser claims this extension.
    ///
    /// `scan_files` uses this to decide what to walk, and the source store uses
    /// it to decide what to keep: snippets are only ever served for graph nodes,
    /// and only a file some parser claims can produce one. `index_files` takes
    /// paths straight from its caller — the daemon's watcher reports every `.o`
    /// and `.rmeta` a build writes — so it has to apply the test itself.
    fn is_extractable(
        &self,
        path: &Path,
        opts: &IndexOptions,
        ext_map: &HashMap<String, Language>,
        plugin_exts: &[String],
    ) -> bool {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let dotted = format!(".{}", ext);
        let known = opts
            .custom_extensions
            .get(&dotted)
            .copied()
            .or_else(|| language_for_extension(&dotted))
            .is_some();
        known
            || ext_map.contains_key(&dotted)
            || plugin_exts
                .iter()
                .any(|e| e.eq_ignore_ascii_case(&dotted) || e.eq_ignore_ascii_case(ext))
    }

    pub async fn scan_files(&self, opts: &IndexOptions) -> Result<Vec<PathBuf>, ax_utils::errors::AxError> {
        let mut files = Vec::new();
        let exclude_matcher = build_exclude_matcher(&self.project_root, &opts.exclude);
        let walker = WalkBuilder::new(&self.project_root)
            .hidden(true).git_ignore(true).git_global(true).git_exclude(true).build();
        let ext_map = extension_map();
        let plugin_exts = ax_plugins::load_plugins(&self.project_root).extensions();
        for entry in walker {
            let entry = entry.map_err(|e| ax_utils::errors::AxError::File(ax_utils::errors::FileError::new(e.to_string())))?;
            if !entry.file_type().is_some_and(|t| t.is_file()) { continue; }
            let path = entry.path();
            let rel = path
                .strip_prefix(&self.project_root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            if should_skip_path(&rel, exclude_matcher.as_ref()) {
                continue;
            }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let dotted = format!(".{}", ext);
            let lang = opts.custom_extensions.get(&dotted).copied().or_else(|| language_for_extension(&dotted));
            let plugin_hit = plugin_exts.iter().any(|e| {
                e.eq_ignore_ascii_case(&dotted) || e.eq_ignore_ascii_case(ext)
            });
            if lang.is_some() || ext_map.contains_key(&dotted) || plugin_hit {
                files.push(path.to_path_buf());
            }
        }
        Ok(files)
    }

    pub async fn index_all(
        &self,
        queries: &QueryBuilder,
        opts: &IndexOptions,
        mut on_progress: Option<&mut Box<dyn FnMut(IndexProgress) + Send>>,
    ) -> Result<IndexResult, ax_utils::errors::AxError> {
        let start = Instant::now();
        let files = self.scan_files(opts).await?;
        let total = files.len() as u32;
        if let Some(ref mut cb) = on_progress {
            cb(IndexProgress { phase: IndexPhase::Scanning, current: total, total, file_path: None });
        }
        let mut tasks = Vec::new();
        let mut content_hashes: HashMap<String, String> = HashMap::new();
        for path in &files {
            let rel = path.strip_prefix(&self.project_root).unwrap_or(path).to_string_lossy().replace('\\', "/");
            let content = ax_utils::read_text_file(path)?;
            if is_generated(&content) { continue; }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let lang = opts.custom_extensions.get(&format!(".{}", ext)).copied().or_else(|| language_for_extension(ext)).unwrap_or(Language::Unknown);
            let content_hash = hash(content.as_bytes()).to_hex().to_string();
            // Store the text now, while we still own it — query-time snippets are
            // served from here so a graph read never touches the working tree.
            queries.upsert_file_content(&rel, &content_hash, &content).await?;
            content_hashes.insert(rel.clone(), content_hash);
            tasks.push(ParseTask { file_path: rel, content, language: lang });
        }

        let (results, parse_total) = self.parse_with_plugins(tasks, &mut on_progress);

        for (i, (file_path, parse_result)) in results.into_iter().enumerate() {
            if let Some(ref mut cb) = on_progress {
                cb(IndexProgress {
                    phase: IndexPhase::Extracting,
                    current: i as u32 + 1,
                    total: parse_total,
                    file_path: Some(file_path.clone()),
                });
            }
            // Keeps file_contents: the text was stored above and the hash we are
            // about to write must stay paired with it.
            queries.clear_file_for_reindex(&file_path).await?;
            match parse_result {
                Ok(extraction) => {
                    queries.upsert_nodes(&extraction.nodes).await?;
                    queries.upsert_edges(&extraction.edges).await?;
                    let mut refs: Vec<ax_types::UnresolvedReference> = Vec::new();
                    for ref_ in &extraction.unresolved_references {
                        let mut r = ref_.clone();
                        if r.file_path.is_none() {
                            r.file_path = Some(file_path.clone());
                        }
                        if r.language.is_none() {
                            r.language = extraction.nodes.first().map(|n| n.language);
                        }
                        refs.push(r);
                    }
                    queries.insert_unresolved_refs(&refs).await?;
                    let full_path = self.project_root.join(&file_path);
                    let meta = std::fs::metadata(&full_path).ok();
                    let size = meta.as_ref().map(|m| m.len() as i64).unwrap_or(0);
                    let modified = meta.and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64).unwrap_or(now_ms());
                    let content_hash = content_hashes.remove(&file_path).unwrap_or_default();
                    queries.upsert_file(&FileRecord {
                        path: file_path,
                        content_hash,
                        language: extraction.nodes.first().map(|n| n.language).unwrap_or(Language::Unknown),
                        size, modified_at: modified, indexed_at: now_ms(),
                        node_count: extraction.nodes.len() as i64,
                        errors: if extraction.errors.is_empty() { None } else { Some(extraction.errors.clone()) },
                    }).await?;
                }
                Err(msg) => {
                    // A failed parse writes an empty content_hash, which no stored
                    // text could ever match. Drop the row so reads say "not stored"
                    // instead of looking permanently stale and retrying forever.
                    queries.delete_file_content(&file_path).await?;
                    queries.upsert_file(&FileRecord {
                        path: file_path.clone(), content_hash: String::new(), language: Language::Unknown,
                        size: 0, modified_at: now_ms(), indexed_at: now_ms(), node_count: 0,
                        errors: Some(vec![ExtractionError { message: msg, file_path: Some(file_path), line: None, column: None, severity: ExtractionSeverity::Error, code: None }]),
                    }).await?;
                }
            }
        }
        Ok(IndexResult { files_indexed: parse_total, duration_ms: start.elapsed().as_millis() as u64 })
    }

    /// Incremental sync — scan project, re-index only new/changed files, drop stale entries.
    pub async fn sync_changed(
        &self,
        queries: &QueryBuilder,
        opts: &IndexOptions,
        mut on_progress: Option<&mut Box<dyn FnMut(IndexProgress) + Send>>,
    ) -> Result<SyncResult, ax_utils::errors::AxError> {
        let start = Instant::now();
        let files = self.scan_files(opts).await?;
        let scan_total = files.len() as u32;

        if let Some(ref mut cb) = on_progress {
            cb(IndexProgress {
                phase: IndexPhase::Scanning,
                current: 0,
                total: scan_total,
                file_path: None,
            });
        }

        let indexed = queries.get_all_files().await?;
        let indexed_map: HashMap<String, FileRecord> =
            indexed.into_iter().map(|f| (f.path.clone(), f)).collect();

        // Databases indexed before schema v17 have file rows but no stored source.
        // Unchanged files would never be revisited, so snippets would stay
        // "not stored" forever; close the gap here without a full re-extraction.
        let stored_paths = queries.get_file_content_paths().await?;
        let mut backfilled = 0u32;

        let mut current_paths = HashSet::new();
        let mut changed = Vec::new();

        for (i, path) in files.iter().enumerate() {
            let rel = path
                .strip_prefix(&self.project_root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            current_paths.insert(rel.clone());

            if let Some(ref mut cb) = on_progress {
                cb(IndexProgress {
                    phase: IndexPhase::Scanning,
                    current: i as u32 + 1,
                    total: scan_total,
                    file_path: Some(rel.clone()),
                });
            }

            let size = file_size(path);
            let modified = file_mtime_ms(path).unwrap_or(0);
            match indexed_map.get(&rel) {
                None => changed.push(rel),
                Some(rec) if rec.modified_at == modified && rec.size == size => {
                    if !stored_paths.contains(&rel) && !rec.content_hash.is_empty() {
                        // Unchanged file with no stored source. Re-read and store it
                        // only if the text still matches the indexed hash; otherwise
                        // the graph itself is stale and a re-index is the right fix.
                        if let Ok(content) = ax_utils::read_text_file(path) {
                            let h = hash(content.as_bytes()).to_hex().to_string();
                            if h == rec.content_hash {
                                queries.upsert_file_content(&rel, &h, &content).await?;
                                backfilled += 1;
                            } else {
                                changed.push(rel);
                            }
                        }
                    }
                }
                Some(rec) => {
                    // mtime or size differs — compare content hashes so that
                    // touch-only changes (branch switches, checkouts) don't
                    // trigger a re-extraction of identical content.
                    let same_content = !rec.content_hash.is_empty()
                        && std::fs::read(path)
                            .map(|bytes| hash(&bytes).to_hex().to_string() == rec.content_hash)
                            .unwrap_or(false);
                    if same_content {
                        // Refresh the stamp so the fast path applies next sync.
                        let mut refreshed = rec.clone();
                        refreshed.modified_at = modified;
                        refreshed.size = size;
                        refreshed.indexed_at = now_ms();
                        queries.upsert_file(&refreshed).await?;
                        // Touch-only change: content is identical, so this file is
                        // never re-extracted. Backfill its source here or it stays
                        // missing from the store indefinitely.
                        if !stored_paths.contains(&rel) {
                            if let Ok(content) = ax_utils::read_text_file(path) {
                                queries
                                    .upsert_file_content(&rel, &rec.content_hash, &content)
                                    .await?;
                                backfilled += 1;
                            }
                        }
                    } else {
                        changed.push(rel);
                    }
                }
            }
        }

        let deleted: Vec<String> = indexed_map
            .keys()
            .filter(|p| !current_paths.contains(*p))
            .cloned()
            .collect();

        let removed = deleted.len() as u32;
        if !deleted.is_empty() {
            let del_total = deleted.len() as u32;
            for (i, path) in deleted.iter().enumerate() {
                if let Some(ref mut cb) = on_progress {
                    cb(IndexProgress {
                        phase: IndexPhase::Extracting,
                        current: i as u32 + 1,
                        total: del_total,
                        file_path: Some(path.clone()),
                    });
                }
                queries.clear_file(path).await?;
            }
        }

        let indexed_count = if changed.is_empty() {
            0
        } else {
            self.index_files(queries, &changed, opts, on_progress)
                .await?
                .files_indexed
        };

        let mut affected_files = changed;
        affected_files.extend(deleted.clone());

        // After indexing, so this sees the file rows this sync just wrote.
        let pruned = queries.prune_orphan_file_contents().await?;

        Ok(SyncResult {
            files_indexed: indexed_count,
            files_removed: removed,
            source_backfilled: backfilled,
            source_pruned: pruned,
            affected_files,
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }
    /// CG: `indexFiles` — re-index only the given project-relative paths.
    pub async fn index_files(
        &self,
        queries: &QueryBuilder,
        file_paths: &[String],
        opts: &IndexOptions,
        mut on_progress: Option<&mut Box<dyn FnMut(IndexProgress) + Send>>,
    ) -> Result<IndexResult, ax_utils::errors::AxError> {
        let start = Instant::now();
        let mut tasks = Vec::new();
        let mut content_hashes: HashMap<String, String> = HashMap::new();
        let ext_map = extension_map();
        let plugin_exts = ax_plugins::load_plugins(&self.project_root).extensions();
        for file_path in file_paths {
            let rel = file_path.replace('\\', "/");
            let full_path = self.project_root.join(&rel);
            if !full_path.is_file() {
                continue;
            }
            let content = ax_utils::read_text_file(&full_path).map_err(ax_utils::errors::AxError::File)?;
            if is_generated(&content) {
                continue;
            }
            let ext = full_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let lang = opts
                .custom_extensions
                .get(&format!(".{}", ext))
                .copied()
                .or_else(|| language_for_extension(ext))
                .unwrap_or(Language::Unknown);
            let content_hash = hash(content.as_bytes()).to_hex().to_string();
            // Store the text now, while we still own it — query-time snippets are
            // served from here so a graph read never touches the working tree.
            // Only for files a parser claims: callers of index_files did not run
            // the scan filter, and storing build output would put megabytes of
            // object-file text in ax.db that no snippet could ever use.
            if self.is_extractable(&full_path, opts, &ext_map, &plugin_exts) {
                queries.upsert_file_content(&rel, &content_hash, &content).await?;
            }
            content_hashes.insert(rel.clone(), content_hash);
            tasks.push(ParseTask { file_path: rel, content, language: lang });
        }

        let (results, batch_total) = self.parse_with_plugins(tasks, &mut on_progress);
        for (i, (file_path, parse_result)) in results.into_iter().enumerate() {
            if let Some(ref mut cb) = on_progress {
                cb(IndexProgress {
                    phase: IndexPhase::Extracting,
                    current: i as u32 + 1,
                    total: batch_total,
                    file_path: Some(file_path.clone()),
                });
            }
            // Keeps file_contents: the text was stored above and the hash we are
            // about to write must stay paired with it.
            queries.clear_file_for_reindex(&file_path).await?;
            match parse_result {
                Ok(extraction) => {
                    queries.upsert_nodes(&extraction.nodes).await?;
                    queries.upsert_edges(&extraction.edges).await?;
                    let mut refs: Vec<ax_types::UnresolvedReference> = Vec::new();
                    for ref_ in &extraction.unresolved_references {
                        let mut r = ref_.clone();
                        if r.file_path.is_none() {
                            r.file_path = Some(file_path.clone());
                        }
                        if r.language.is_none() {
                            r.language = extraction.nodes.first().map(|n| n.language);
                        }
                        refs.push(r);
                    }
                    queries.insert_unresolved_refs(&refs).await?;
                    let full_path = self.project_root.join(&file_path);
                    let meta = std::fs::metadata(&full_path).ok();
                    let size = meta.as_ref().map(|m| m.len() as i64).unwrap_or(0);
                    let modified = meta
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(now_ms());
                    let content_hash = content_hashes.remove(&file_path).unwrap_or_default();
                    queries.upsert_file(&FileRecord {
                        path: file_path,
                        content_hash,
                        language: extraction.nodes.first().map(|n| n.language).unwrap_or(Language::Unknown),
                        size,
                        modified_at: modified,
                        indexed_at: now_ms(),
                        node_count: extraction.nodes.len() as i64,
                        errors: if extraction.errors.is_empty() { None } else { Some(extraction.errors.clone()) },
                    }).await?;
                }
                Err(msg) => {
                    // A failed parse writes an empty content_hash, which no stored
                    // text could ever match. Drop the row so reads say "not stored"
                    // instead of looking permanently stale and retrying forever.
                    queries.delete_file_content(&file_path).await?;
                    queries.upsert_file(&FileRecord {
                        path: file_path.clone(),
                        content_hash: String::new(),
                        language: Language::Unknown,
                        size: 0,
                        modified_at: now_ms(),
                        indexed_at: now_ms(),
                        node_count: 0,
                        errors: Some(vec![ExtractionError {
                            message: msg,
                            file_path: Some(file_path),
                            line: None,
                            column: None,
                            severity: ExtractionSeverity::Error,
                            code: None,
                        }]),
                    }).await?;
                }
            }
        }
        Ok(IndexResult { files_indexed: batch_total, duration_ms: start.elapsed().as_millis() as u64 })
    }
}

pub struct IndexResult {
    pub files_indexed: u32,
    pub duration_ms: u64,
}

pub struct SyncResult {
    pub files_indexed: u32,
    pub files_removed: u32,
    /// Files whose source was added to the store without re-extraction — the
    /// pre-v17 backfill. Non-zero only until the store has caught up.
    pub source_backfilled: u32,
    /// Stored source rows dropped because no indexed file claims them. Cleans up
    /// after binaries that stored text for files they never parsed.
    pub source_pruned: u32,
    /// Project-relative paths that were added, modified, or deleted this sync.
    pub affected_files: Vec<String>,
    pub duration_ms: u64,
}

impl SyncResult {
    pub fn had_changes(&self) -> bool {
        self.files_indexed > 0 || self.files_removed > 0
    }
}

fn file_mtime_ms(path: &Path) -> Option<i64> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as i64)
}

fn file_size(path: &Path) -> i64 {
    std::fs::metadata(path).map(|m| m.len() as i64).unwrap_or(0)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn build_exclude_matcher(project_root: &Path, patterns: &[String]) -> Option<Gitignore> {
    if patterns.is_empty() {
        return None;
    }
    let mut builder = ignore::gitignore::GitignoreBuilder::new(project_root);
    for pat in patterns {
        if let Err(e) = builder.add_line(None, pat) {
            tracing::warn!("invalid ax.json exclude pattern {pat:?}: {e}");
        }
    }
    builder.build().ok()
}

fn should_skip_path(rel: &str, exclude: Option<&Gitignore>) -> bool {
    let rel_norm = rel.replace('\\', "/");
    for segment in rel_norm.split('/') {
        if BUILTIN_SKIP_DIRS.contains(&segment) {
            return true;
        }
    }
    if let Some(ex) = exclude {
        if ex.matched(rel_norm.as_str(), false).is_ignore() {
            return true;
        }
    }
    false
}