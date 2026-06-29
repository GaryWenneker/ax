//! Core type definitions for the ax semantic knowledge graph.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// =============================================================================
// Union Types
// =============================================================================

/// Types of nodes in the knowledge graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    File,
    Module,
    Class,
    Struct,
    Interface,
    Trait,
    Protocol,
    Function,
    Method,
    Property,
    Field,
    Variable,
    Constant,
    Enum,
    EnumMember,
    TypeAlias,
    Namespace,
    Parameter,
    Import,
    Export,
    Route,
    Component,
}

impl NodeKind {
    pub const ALL: [NodeKind; 22] = [
        NodeKind::File,
        NodeKind::Module,
        NodeKind::Class,
        NodeKind::Struct,
        NodeKind::Interface,
        NodeKind::Trait,
        NodeKind::Protocol,
        NodeKind::Function,
        NodeKind::Method,
        NodeKind::Property,
        NodeKind::Field,
        NodeKind::Variable,
        NodeKind::Constant,
        NodeKind::Enum,
        NodeKind::EnumMember,
        NodeKind::TypeAlias,
        NodeKind::Namespace,
        NodeKind::Parameter,
        NodeKind::Import,
        NodeKind::Export,
        NodeKind::Route,
        NodeKind::Component,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            NodeKind::File => "file",
            NodeKind::Module => "module",
            NodeKind::Class => "class",
            NodeKind::Struct => "struct",
            NodeKind::Interface => "interface",
            NodeKind::Trait => "trait",
            NodeKind::Protocol => "protocol",
            NodeKind::Function => "function",
            NodeKind::Method => "method",
            NodeKind::Property => "property",
            NodeKind::Field => "field",
            NodeKind::Variable => "variable",
            NodeKind::Constant => "constant",
            NodeKind::Enum => "enum",
            NodeKind::EnumMember => "enum_member",
            NodeKind::TypeAlias => "type_alias",
            NodeKind::Namespace => "namespace",
            NodeKind::Parameter => "parameter",
            NodeKind::Import => "import",
            NodeKind::Export => "export",
            NodeKind::Route => "route",
            NodeKind::Component => "component",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "file" => Some(NodeKind::File),
            "module" => Some(NodeKind::Module),
            "class" => Some(NodeKind::Class),
            "struct" => Some(NodeKind::Struct),
            "interface" => Some(NodeKind::Interface),
            "trait" => Some(NodeKind::Trait),
            "protocol" => Some(NodeKind::Protocol),
            "function" => Some(NodeKind::Function),
            "method" => Some(NodeKind::Method),
            "property" => Some(NodeKind::Property),
            "field" => Some(NodeKind::Field),
            "variable" => Some(NodeKind::Variable),
            "constant" => Some(NodeKind::Constant),
            "enum" => Some(NodeKind::Enum),
            "enum_member" => Some(NodeKind::EnumMember),
            "type_alias" => Some(NodeKind::TypeAlias),
            "namespace" => Some(NodeKind::Namespace),
            "parameter" => Some(NodeKind::Parameter),
            "import" => Some(NodeKind::Import),
            "export" => Some(NodeKind::Export),
            "route" => Some(NodeKind::Route),
            "component" => Some(NodeKind::Component),
            _ => None,
        }
    }
}

/// Types of edges (relationships) between nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    Contains,
    Calls,
    Imports,
    Exports,
    Extends,
    Implements,
    References,
    TypeOf,
    Returns,
    Instantiates,
    Overrides,
    Decorates,
}

impl EdgeKind {
    pub const ALL: [EdgeKind; 12] = [
        EdgeKind::Contains,
        EdgeKind::Calls,
        EdgeKind::Imports,
        EdgeKind::Exports,
        EdgeKind::Extends,
        EdgeKind::Implements,
        EdgeKind::References,
        EdgeKind::TypeOf,
        EdgeKind::Returns,
        EdgeKind::Instantiates,
        EdgeKind::Overrides,
        EdgeKind::Decorates,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            EdgeKind::Contains => "contains",
            EdgeKind::Calls => "calls",
            EdgeKind::Imports => "imports",
            EdgeKind::Exports => "exports",
            EdgeKind::Extends => "extends",
            EdgeKind::Implements => "implements",
            EdgeKind::References => "references",
            EdgeKind::TypeOf => "type_of",
            EdgeKind::Returns => "returns",
            EdgeKind::Instantiates => "instantiates",
            EdgeKind::Overrides => "overrides",
            EdgeKind::Decorates => "decorates",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "contains" => Some(EdgeKind::Contains),
            "calls" => Some(EdgeKind::Calls),
            "imports" => Some(EdgeKind::Imports),
            "exports" => Some(EdgeKind::Exports),
            "extends" => Some(EdgeKind::Extends),
            "implements" => Some(EdgeKind::Implements),
            "references" => Some(EdgeKind::References),
            "type_of" => Some(EdgeKind::TypeOf),
            "returns" => Some(EdgeKind::Returns),
            "instantiates" => Some(EdgeKind::Instantiates),
            "overrides" => Some(EdgeKind::Overrides),
            "decorates" => Some(EdgeKind::Decorates),
            _ => None,
        }
    }
}

/// Supported programming languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    Typescript,
    Javascript,
    Tsx,
    Jsx,
    Python,
    Go,
    Rust,
    Java,
    C,
    Cpp,
    Csharp,
    Razor,
    Php,
    Ruby,
    Swift,
    Kotlin,
    Dart,
    Svelte,
    Vue,
    Astro,
    Liquid,
    Pascal,
    Scala,
    Lua,
    Luau,
    Objc,
    R,
    Yaml,
    Twig,
    Xml,
    Properties,
    Unknown,
}

impl Language {
    pub const ALL: [Language; 32] = [
        Language::Typescript,
        Language::Javascript,
        Language::Tsx,
        Language::Jsx,
        Language::Python,
        Language::Go,
        Language::Rust,
        Language::Java,
        Language::C,
        Language::Cpp,
        Language::Csharp,
        Language::Razor,
        Language::Php,
        Language::Ruby,
        Language::Swift,
        Language::Kotlin,
        Language::Dart,
        Language::Svelte,
        Language::Vue,
        Language::Astro,
        Language::Liquid,
        Language::Pascal,
        Language::Scala,
        Language::Lua,
        Language::Luau,
        Language::Objc,
        Language::R,
        Language::Yaml,
        Language::Twig,
        Language::Xml,
        Language::Properties,
        Language::Unknown,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Language::Typescript => "typescript",
            Language::Javascript => "javascript",
            Language::Tsx => "tsx",
            Language::Jsx => "jsx",
            Language::Python => "python",
            Language::Go => "go",
            Language::Rust => "rust",
            Language::Java => "java",
            Language::C => "c",
            Language::Cpp => "cpp",
            Language::Csharp => "csharp",
            Language::Razor => "razor",
            Language::Php => "php",
            Language::Ruby => "ruby",
            Language::Swift => "swift",
            Language::Kotlin => "kotlin",
            Language::Dart => "dart",
            Language::Svelte => "svelte",
            Language::Vue => "vue",
            Language::Astro => "astro",
            Language::Liquid => "liquid",
            Language::Pascal => "pascal",
            Language::Scala => "scala",
            Language::Lua => "lua",
            Language::Luau => "luau",
            Language::Objc => "objc",
            Language::R => "r",
            Language::Yaml => "yaml",
            Language::Twig => "twig",
            Language::Xml => "xml",
            Language::Properties => "properties",
            Language::Unknown => "unknown",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "typescript" => Some(Language::Typescript),
            "javascript" => Some(Language::Javascript),
            "tsx" => Some(Language::Tsx),
            "jsx" => Some(Language::Jsx),
            "python" => Some(Language::Python),
            "go" => Some(Language::Go),
            "rust" => Some(Language::Rust),
            "java" => Some(Language::Java),
            "c" => Some(Language::C),
            "cpp" => Some(Language::Cpp),
            "csharp" => Some(Language::Csharp),
            "razor" => Some(Language::Razor),
            "php" => Some(Language::Php),
            "ruby" => Some(Language::Ruby),
            "swift" => Some(Language::Swift),
            "kotlin" => Some(Language::Kotlin),
            "dart" => Some(Language::Dart),
            "svelte" => Some(Language::Svelte),
            "vue" => Some(Language::Vue),
            "astro" => Some(Language::Astro),
            "liquid" => Some(Language::Liquid),
            "pascal" => Some(Language::Pascal),
            "scala" => Some(Language::Scala),
            "lua" => Some(Language::Lua),
            "luau" => Some(Language::Luau),
            "objc" => Some(Language::Objc),
            "r" => Some(Language::R),
            "yaml" => Some(Language::Yaml),
            "twig" => Some(Language::Twig),
            "xml" => Some(Language::Xml),
            "properties" => Some(Language::Properties),
            "unknown" => Some(Language::Unknown),
            _ => None,
        }
    }
}

/// Visibility modifier for symbols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    Private,
    Protected,
    Internal,
}

/// Edge provenance — how an edge was created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Provenance {
    #[serde(rename = "tree-sitter")]
    TreeSitter,
    Scip,
    Heuristic,
}

/// Internal-only reference kind for fn-as-value capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceKind {
    Contains,
    Calls,
    Imports,
    Exports,
    Extends,
    Implements,
    References,
    TypeOf,
    Returns,
    Instantiates,
    Overrides,
    Decorates,
    FunctionRef,
}

impl ReferenceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReferenceKind::Contains => "contains",
            ReferenceKind::Calls => "calls",
            ReferenceKind::Imports => "imports",
            ReferenceKind::Exports => "exports",
            ReferenceKind::Extends => "extends",
            ReferenceKind::Implements => "implements",
            ReferenceKind::References => "references",
            ReferenceKind::TypeOf => "type_of",
            ReferenceKind::Returns => "returns",
            ReferenceKind::Instantiates => "instantiates",
            ReferenceKind::Overrides => "overrides",
            ReferenceKind::Decorates => "decorates",
            ReferenceKind::FunctionRef => "function_ref",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "contains" => Some(ReferenceKind::Contains),
            "calls" => Some(ReferenceKind::Calls),
            "imports" => Some(ReferenceKind::Imports),
            "exports" => Some(ReferenceKind::Exports),
            "extends" => Some(ReferenceKind::Extends),
            "implements" => Some(ReferenceKind::Implements),
            "references" => Some(ReferenceKind::References),
            "type_of" => Some(ReferenceKind::TypeOf),
            "returns" => Some(ReferenceKind::Returns),
            "instantiates" => Some(ReferenceKind::Instantiates),
            "overrides" => Some(ReferenceKind::Overrides),
            "decorates" => Some(ReferenceKind::Decorates),
            "function_ref" => Some(ReferenceKind::FunctionRef),
            _ => None,
        }
    }
}

/// Retrieval confidence for context-style queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    High,
    Low,
}

/// Traversal direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TraversalDirection {
    #[default]
    Outgoing,
    Incoming,
    Both,
}


/// Output format for context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ContextFormat {
    #[default]
    Markdown,
    Json,
}

// =============================================================================
// Core Graph Types
// =============================================================================

/// A node in the knowledge graph representing a code symbol.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    pub name: String,
    pub qualified_name: String,
    pub file_path: String,
    pub language: Language,
    pub start_line: i32,
    pub end_line: i32,
    pub start_column: i32,
    pub end_column: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docstring: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<Visibility>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_exported: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_async: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_static: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_abstract: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decorators: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_parameters: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    pub updated_at: i64,
}

/// An edge representing a relationship between two nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub source: String,
    pub target: String,
    pub kind: EdgeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
}

/// Metadata about a tracked file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRecord {
    pub path: String,
    pub content_hash: String,
    pub language: Language,
    pub size: i64,
    pub modified_at: i64,
    pub indexed_at: i64,
    #[serde(default)]
    pub node_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<ExtractionError>>,
}

// =============================================================================
// Extraction Types
// =============================================================================

/// Result from parsing a source file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionResult {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub unresolved_references: Vec<UnresolvedReference>,
    pub errors: Vec<ExtractionError>,
    pub duration_ms: u64,
}

/// Error during code extraction.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionError {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<i32>,
    pub severity: ExtractionSeverity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtractionSeverity {
    Error,
    Warning,
}

/// A reference that could not be resolved during extraction.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedReference {
    pub from_node_id: String,
    pub reference_name: String,
    pub reference_kind: ReferenceKind,
    pub line: i32,
    pub column: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<Language>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates: Option<Vec<String>>,
}

// =============================================================================
// Query Types
// =============================================================================

/// A subgraph containing a subset of the knowledge graph.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subgraph {
    pub nodes: HashMap<String, Node>,
    pub edges: Vec<Edge>,
    pub roots: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Confidence>,
}

/// Options for graph traversal.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraversalOptions {
    pub max_depth: Option<u32>,
    pub edge_kinds: Option<Vec<EdgeKind>>,
    pub node_kinds: Option<Vec<NodeKind>>,
    pub direction: Option<TraversalDirection>,
    pub limit: Option<u32>,
    pub include_start: Option<bool>,
}

impl Default for TraversalOptions {
    fn default() -> Self {
        Self {
            max_depth: None,
            edge_kinds: None,
            node_kinds: None,
            direction: Some(TraversalDirection::Outgoing),
            limit: Some(1000),
            include_start: Some(true),
        }
    }
}

/// Options for searching the graph.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SearchOptions {
    pub kinds: Option<Vec<NodeKind>>,
    pub languages: Option<Vec<Language>>,
    pub include_patterns: Option<Vec<String>>,
    pub exclude_patterns: Option<Vec<String>>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub case_sensitive: Option<bool>,
}

/// A search result with relevance scoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub node: Node,
    pub score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlights: Option<Vec<String>>,
}

// =============================================================================
// Context Types
// =============================================================================

/// A node reference with its connecting edge.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeRef {
    pub node: Node,
    pub edge: Edge,
}

/// Context information for code understanding.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    pub focal: Node,
    pub ancestors: Vec<Node>,
    pub children: Vec<Node>,
    pub incoming_refs: Vec<NodeRef>,
    pub outgoing_refs: Vec<NodeRef>,
    pub types: Vec<Node>,
    pub imports: Vec<Node>,
}

/// A block of code with context.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeBlock {
    pub content: String,
    pub file_path: String,
    pub start_line: i32,
    pub end_line: i32,
    pub language: Language,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<Node>,
}

// =============================================================================
// Database Types
// =============================================================================

/// Database schema version info.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaVersion {
    pub version: i32,
    pub applied_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Statistics about the knowledge graph.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphStats {
    pub node_count: i64,
    pub edge_count: i64,
    pub file_count: i64,
    pub nodes_by_kind: HashMap<String, i64>,
    pub edges_by_kind: HashMap<String, i64>,
    pub files_by_language: HashMap<String, i64>,
    pub db_size_bytes: i64,
    pub last_updated: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unresolved_ref_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_total: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_resolved: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_unresolved: Option<u32>,
}

// =============================================================================
// Task Context Types
// =============================================================================

/// Input for building task context.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TaskInput {
    Text(String),
    Structured {
        title: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
}

/// Options for building task context.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildContextOptions {
    pub max_nodes: Option<u32>,
    pub max_code_blocks: Option<u32>,
    pub max_code_block_size: Option<u32>,
    pub include_code: Option<bool>,
    pub format: Option<ContextFormat>,
    pub search_limit: Option<u32>,
    pub traversal_depth: Option<u32>,
    pub min_score: Option<f64>,
}

impl Default for BuildContextOptions {
    fn default() -> Self {
        Self {
            max_nodes: Some(50),
            max_code_blocks: Some(10),
            max_code_block_size: Some(2000),
            include_code: Some(true),
            format: Some(ContextFormat::Markdown),
            search_limit: Some(5),
            traversal_depth: Some(2),
            min_score: Some(0.3),
        }
    }
}

/// Statistics about a task context.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskContextStats {
    pub node_count: u32,
    pub edge_count: u32,
    pub file_count: u32,
    pub code_block_count: u32,
    pub total_code_size: u32,
}

/// Full context for a task, ready for AI injection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskContext {
    pub query: String,
    pub subgraph: Subgraph,
    pub entry_points: Vec<Node>,
    pub code_blocks: Vec<CodeBlock>,
    pub related_files: Vec<String>,
    pub summary: String,
    pub stats: TaskContextStats,
}

/// Options for `ax_explore` and CLI `explore`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExploreOptions {
    pub limit: Option<u32>,
    pub depth: Option<u32>,
    pub include_code: Option<bool>,
    pub max_lines_per_snippet: Option<u32>,
    pub max_source_chars: Option<u32>,
}

/// One explore hit with optional numbered source and call spine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExploreEntry {
    pub node: Node,
    pub score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub callers: Vec<Node>,
    pub callees: Vec<Node>,
}

/// Rich explore payload for MCP and CLI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExploreResult {
    pub query: String,
    pub summary: String,
    pub blast_radius: String,
    pub entries: Vec<ExploreEntry>,
}

/// Options for finding relevant context.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FindRelevantContextOptions {
    pub search_limit: Option<u32>,
    pub traversal_depth: Option<u32>,
    pub max_nodes: Option<u32>,
    pub min_score: Option<f64>,
    pub edge_kinds: Option<Vec<EdgeKind>>,
    pub node_kinds: Option<Vec<NodeKind>>,
}

/// Index progress phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IndexPhase {
    Scanning,
    Extracting,
    Resolving,
}

/// Progress callback data during indexing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexProgress {
    pub phase: IndexPhase,
    pub current: u32,
    pub total: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
}

/// Pending file in the watcher queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingFile {
    pub path: String,
    pub first_seen_ms: i64,
    pub last_seen_ms: i64,
    pub indexing: bool,
}
