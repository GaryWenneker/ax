//! Memory vault types.

use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

fn default_tier() -> String {
    "t2".into()
}

/// Kinds that preflight never injects; only an explicit recall finds them.
/// `recall_only_kinds_sql!` is the same list as an SQL tuple.
pub const RECALL_ONLY_KINDS: &[&str] = &["turn", "doc", "graph_diff", "activity", "file_change"];

/// `RECALL_ONLY_KINDS` as an SQL tuple literal, for `concat!` in queries.
macro_rules! recall_only_kinds_sql {
    () => {
        "('turn', 'doc', 'graph_diff', 'activity', 'file_change')"
    };
}
pub(crate) use recall_only_kinds_sql;

/// What kind of knowledge a memory captures.
pub const MEMORY_KINDS: &[&str] = &[
    "decision",
    "bug_fix",
    "architecture",
    "convention",
    "note",
    "lesson",
    "git",
    "turn",
    "doc",
    "policy",
];

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
    /// t0 working, t1 recent, t2 long-term, t3 archive.
    #[serde(default = "default_tier")]
    pub tier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMatch {
    #[serde(flatten)]
    pub memory: MemoryRow,
    /// Combined FTS rank and confidence-decay score. Higher is better.
    pub score: f64,
    /// Why this memory was recalled (signals, terms, anchors).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<crate::fusion::WhyRecalled>,
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

/// Shared recall filter keys for MCP, CLI, and the Command Center API.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryFilter {
    pub session: Option<String>,
    pub turn: Option<String>,
    pub keyword: Option<String>,
    pub trigger: Option<String>,
    pub file: Option<String>,
    pub symbol: Option<String>,
    pub kind: Option<String>,
    pub since: Option<i64>,
    pub until: Option<i64>,
}

#[cfg(test)]
mod recall_only_tests {
    #[test]
    fn the_sql_tuple_matches_the_kind_list() {
        let quoted: Vec<String> = super::RECALL_ONLY_KINDS
            .iter()
            .map(|k| format!("'{k}'"))
            .collect();
        assert_eq!(recall_only_kinds_sql!(), format!("({})", quoted.join(", ")));
    }
}
