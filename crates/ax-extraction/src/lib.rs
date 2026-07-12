//! Tree-sitter extraction engine for ax.

pub mod extraction_version;
pub mod function_ref;
pub mod generated_detection;
pub mod grammars;
pub mod languages;
pub mod orchestrator;
pub mod parse_pool;
pub mod test_mapper;
pub mod ts_grammars;

pub use extraction_version::EXTRACTION_VERSION;
pub use function_ref::{CaptureMode, FnRefCandidate};
pub use grammars::{extension_map, language_for_extension, is_language_supported};
pub use ts_grammars::{has_ts_grammar, ts_language_for};
pub use orchestrator::{ExtractionOrchestrator, IndexOptions};
pub use parse_pool::ParsePool;

use ax_types::{ExtractionResult, Language};
use tree_sitter::Tree;

/// Language-specific AST extractor.
pub trait LanguageExtractor: Send + Sync {
    fn language(&self) -> Language;
    fn extensions(&self) -> &[&str];
    fn extract(&self, source: &[u8], tree: &Tree, path: &str) -> ExtractionResult;
}
