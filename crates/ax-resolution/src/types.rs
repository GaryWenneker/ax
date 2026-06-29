//! Resolution types.

use ax_types::{Language, Node, ReferenceKind};

#[derive(Debug, Clone)]
pub struct UnresolvedRef {
    pub from_node_id: String,
    pub reference_name: String,
    pub reference_kind: ReferenceKind,
    pub line: i32,
    pub column: i32,
    pub file_path: String,
    pub language: Language,
    pub candidates: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedBy {
    ExactMatch,
    Import,
    QualifiedName,
    Framework,
    Fuzzy,
    InstanceMethod,
    FilePath,
    FunctionRef,
}

#[derive(Debug, Clone)]
pub struct ResolvedRef {
    pub original: UnresolvedRef,
    pub target_node_id: String,
    pub confidence: f64,
    pub resolved_by: ResolvedBy,
}

#[derive(Debug, Clone, Default)]
pub struct ResolutionStats {
    pub total: u32,
    pub resolved: u32,
    pub unresolved: u32,
    pub by_method: std::collections::HashMap<String, u32>,
}

#[derive(Debug, Clone)]
pub struct ResolutionResult {
    pub resolved: Vec<ResolvedRef>,
    pub unresolved: Vec<UnresolvedRef>,
    pub stats: ResolutionStats,
}

#[async_trait::async_trait]
pub trait ResolutionContext: Send + Sync {
    async fn get_nodes_in_file(&self, file_path: &str) -> Vec<Node>;
    async fn get_nodes_by_name(&self, name: &str) -> Vec<Node>;
    async fn get_nodes_by_qualified_name(&self, qualified_name: &str) -> Vec<Node>;
    async fn file_exists(&self, file_path: &str) -> bool;
    async fn read_file(&self, file_path: &str) -> Option<String>;
    fn get_project_root(&self) -> &str;
    async fn get_all_files(&self) -> Vec<String>;
}
