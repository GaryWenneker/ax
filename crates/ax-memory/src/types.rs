//! Memory vault types.

use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

/// What kind of knowledge a memory captures.
pub const MEMORY_KINDS: &[&str] = &["decision", "bug_fix", "architecture", "convention", "note", "git"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRow {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    /// Project-relative file paths this memory is about.
    pub files: Vec<String>,
    /// 0.0–1.0; decays over time unless the memory is touched again.
    pub confidence: f64,
    /// manual | mcp | git
    pub source: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMatch {
    #[serde(flatten)]
    pub memory: MemoryRow,
    /// Combined FTS rank and confidence-decay score. Higher is better.
    pub score: f64,
}

#[derive(Debug, Clone, Default)]
pub struct RememberInput {
    pub title: String,
    pub body: String,
    pub kind: Option<String>,
    pub tags: Vec<String>,
    pub files: Vec<String>,
    pub source: Option<String>,
}
