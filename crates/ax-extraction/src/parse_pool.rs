//! Parse worker pool with per-thread parsers.

use std::collections::HashMap;
use std::sync::Arc;

use ax_types::{ExtractionResult, Language};
use rayon::prelude::*;
use tree_sitter::Parser;

use crate::languages::all_extractors;
use crate::ts_grammars;
use crate::LanguageExtractor;

const DEFAULT_POOL_CAP: usize = 8;
const MAX_POOL_SIZE: usize = 16;

pub struct ParseTask {
    pub file_path: String,
    pub content: String,
    pub language: Language,
}

pub struct ParsePool {
    /// Thread pool and extractor registry are built once and reused across
    /// batches — rebuilding them per 48-file chunk dominated small syncs.
    pool: rayon::ThreadPool,
    extractors: Arc<HashMap<Language, Box<dyn LanguageExtractor>>>,
}

impl ParsePool {
    pub fn new() -> Self {
        let env = std::env::var("AX_PARSE_WORKERS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        let pool_size = if env > 0 {
            env.min(MAX_POOL_SIZE)
        } else {
            cpus.min(DEFAULT_POOL_CAP)
        };
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(pool_size)
            .build()
            .unwrap_or_else(|e| {
                tracing::warn!("failed to build sized rayon pool ({e}); using default");
                rayon::ThreadPoolBuilder::new()
                    .build()
                    .expect("rayon pool with default settings")
            });
        Self {
            pool,
            extractors: Arc::new(all_extractors()),
        }
    }

    pub fn parse_batch(&self, tasks: Vec<ParseTask>) -> Vec<(String, Result<ExtractionResult, String>)> {
        let extractors = Arc::clone(&self.extractors);
        self.pool.install(|| {
            tasks
                .into_par_iter()
                .map(|task| {
                    let result = parse_file(&task, &extractors);
                    (task.file_path.clone(), result)
                })
                .collect()
        })
    }
}

fn parse_file(task: &ParseTask, extractors: &HashMap<Language, Box<dyn LanguageExtractor>>) -> Result<ExtractionResult, String> {
    let Some(extractor) = extractor_for_task(task.language, extractors) else {
        return Ok(ExtractionResult::default());
    };

    let ts_lang = ts_grammars::ts_language_for(task.language)
        .ok_or_else(|| format!("no tree-sitter grammar for {:?}", task.language))?;
    let mut parser = Parser::new();
    parser.set_language(&ts_lang).map_err(|e| e.to_string())?;

    let source = task.content.as_bytes();
    let tree = parser.parse(source, None).ok_or_else(|| "parse failed".to_string())?;
    Ok(extractor.extract(source, &tree, &task.file_path))
}

fn extractor_for_task(
    lang: Language,
    extractors: &HashMap<Language, Box<dyn LanguageExtractor>>,
) -> Option<&dyn LanguageExtractor> {
    if let Some(e) = extractors.get(&lang) {
        return Some(e.as_ref());
    }
    // .tsx is Language::Tsx in grammars.rs but TypescriptExtractor registers as Language::Typescript.
    if lang == Language::Kotlin {
        return extractors.get(&Language::Kotlin).map(|b| b.as_ref());
    }
    if lang == Language::Tsx {
        return extractors.get(&Language::Typescript).map(|b| b.as_ref());
    }
    if lang == Language::Jsx {
        return extractors.get(&Language::Javascript).map(|b| b.as_ref());
    }
    None
}
