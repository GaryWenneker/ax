//! Extraction orchestrator — scan, parse, persist.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use ax_db::queries::QueryBuilder;
use ax_types::{ExtractionError, ExtractionSeverity, FileRecord, IndexPhase, IndexProgress, Language};
use blake3::hash;
use ignore::WalkBuilder;

use crate::generated_detection::is_generated;
use crate::grammars::{extension_map, language_for_extension};
use crate::parse_pool::{ParsePool, ParseTask};

#[derive(Clone)]
pub struct IndexOptions {
    pub force: bool,
    pub quiet: bool,
    pub custom_extensions: HashMap<String, Language>,
}

impl Default for IndexOptions {
    fn default() -> Self {
        Self { force: false, quiet: false, custom_extensions: HashMap::new() }
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

    pub async fn scan_files(&self, opts: &IndexOptions) -> Result<Vec<PathBuf>, ax_utils::errors::AxError> {
        let mut files = Vec::new();
        let walker = WalkBuilder::new(&self.project_root)
            .hidden(true).git_ignore(true).git_global(true).git_exclude(true).build();
        let ext_map = extension_map();
        for entry in walker {
            let entry = entry.map_err(|e| ax_utils::errors::AxError::File(ax_utils::errors::FileError::new(e.to_string())))?;
            if !entry.file_type().is_some_and(|t| t.is_file()) { continue; }
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let dotted = format!(".{}", ext);
            let lang = opts.custom_extensions.get(&dotted).copied().or_else(|| language_for_extension(&dotted));
            if lang.is_some() || ext_map.contains_key(&dotted) { files.push(path.to_path_buf()); }
        }
        Ok(files)
    }

    pub async fn index_all(
        &self,
        queries: &QueryBuilder,
        opts: &IndexOptions,
        mut on_progress: Option<Box<dyn FnMut(IndexProgress) + Send>>,
    ) -> Result<IndexResult, ax_utils::errors::AxError> {
        let start = Instant::now();
        let files = self.scan_files(opts).await?;
        let total = files.len() as u32;
        if let Some(ref mut cb) = on_progress {
            cb(IndexProgress { phase: IndexPhase::Scanning, current: total, total, file_path: None });
        }
        let mut tasks = Vec::new();
        for path in &files {
            let rel = path.strip_prefix(&self.project_root).unwrap_or(path).to_string_lossy().replace('\\', "/");
            let content = ax_utils::read_text_file(path)?;
            if is_generated(&content) { continue; }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let lang = opts.custom_extensions.get(&format!(".{}", ext)).copied().or_else(|| language_for_extension(ext)).unwrap_or(Language::Unknown);
            tasks.push(ParseTask { file_path: rel, content, language: lang });
        }
        let results = self.parse_pool.parse_batch(tasks);
        for (i, (file_path, parse_result)) in results.into_iter().enumerate() {
            if let Some(ref mut cb) = on_progress {
                cb(IndexProgress { phase: IndexPhase::Extracting, current: i as u32 + 1, total, file_path: Some(file_path.clone()) });
            }
            queries.clear_file(&file_path).await?;
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
                    let content_hash = hash(full_path.to_string_lossy().as_bytes()).to_hex().to_string();
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
                    queries.upsert_file(&FileRecord {
                        path: file_path.clone(), content_hash: String::new(), language: Language::Unknown,
                        size: 0, modified_at: now_ms(), indexed_at: now_ms(), node_count: 0,
                        errors: Some(vec![ExtractionError { message: msg, file_path: Some(file_path), line: None, column: None, severity: ExtractionSeverity::Error, code: None }]),
                    }).await?;
                }
            }
        }
        Ok(IndexResult { files_indexed: total, duration_ms: start.elapsed().as_millis() as u64 })
    }
    /// CG: `indexFiles` — re-index only the given project-relative paths.
    pub async fn index_files(
        &self,
        queries: &QueryBuilder,
        file_paths: &[String],
        opts: &IndexOptions,
        mut on_progress: Option<Box<dyn FnMut(IndexProgress) + Send>>,
    ) -> Result<IndexResult, ax_utils::errors::AxError> {
        let start = Instant::now();
        let mut tasks = Vec::new();
        for file_path in file_paths {
            let rel = file_path.replace('\\', "/");
            let full_path = self.project_root.join(&rel);
            if !full_path.is_file() {
                continue;
            }
            let content = ax_utils::read_text_file(&full_path).map_err(|e| ax_utils::errors::AxError::File(e))?;
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
            tasks.push(ParseTask { file_path: rel, content, language: lang });
        }

        let results = self.parse_pool.parse_batch(tasks);
        let batch_total = results.len() as u32;
        for (i, (file_path, parse_result)) in results.into_iter().enumerate() {
            if let Some(ref mut cb) = on_progress {
                cb(IndexProgress {
                    phase: IndexPhase::Extracting,
                    current: i as u32 + 1,
                    total: batch_total,
                    file_path: Some(file_path.clone()),
                });
            }
            queries.clear_file(&file_path).await?;
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
                    let content_hash = hash(full_path.to_string_lossy().as_bytes()).to_hex().to_string();
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

pub struct IndexResult { pub files_indexed: u32, pub duration_ms: u64 }

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}