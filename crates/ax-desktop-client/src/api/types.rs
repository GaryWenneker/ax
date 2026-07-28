//! Shared DTOs mirroring `web-ui/src/api.ts` / `types.ts` / `shipApi.ts`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LangStat {
    pub language: String,
    pub count: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Stats {
    pub node_count: i64,
    pub edge_count: i64,
    pub file_count: i64,
    #[serde(default)]
    pub languages: Vec<LangStat>,
    #[serde(default)]
    pub last_indexed_at: i64,
    pub unresolved_ref_count: Option<i64>,
    #[serde(default)]
    pub db_size_bytes: i64,
    #[serde(default)]
    pub policy_rules_count: i64,
    #[serde(default)]
    pub policy_skills_count: i64,
    #[serde(default)]
    pub readonly: bool,
    #[serde(default)]
    pub project_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NodeRow {
    pub id: String,
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub qualified_name: String,
    pub file_path: String,
    pub language: String,
    pub start_line: i64,
    #[serde(default)]
    pub end_line: i64,
    pub signature: Option<String>,
    #[serde(default)]
    pub is_exported: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NodePage {
    pub nodes: Vec<NodeRow>,
    pub total: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EdgeNode {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub file_path: String,
    pub start_line: i64,
    pub edge_kind: String,
    pub edge_confidence: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NodeDetailProps {
    pub id: String,
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub qualified_name: String,
    pub file_path: String,
    pub language: String,
    pub start_line: i64,
    pub end_line: i64,
    pub signature: Option<String>,
    pub docstring: Option<String>,
    pub visibility: Option<String>,
    #[serde(default)]
    pub is_exported: i64,
    #[serde(default)]
    pub is_async: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NodeDetail {
    pub node: NodeDetailProps,
    #[serde(default)]
    pub callers: Vec<EdgeNode>,
    #[serde(default)]
    pub callees: Vec<EdgeNode>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileRow {
    pub path: String,
    pub language: String,
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub node_count: i64,
    #[serde(default)]
    pub indexed_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileRoot {
    pub name: String,
    pub path: String,
    pub count: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FilePage {
    pub files: Vec<FileRow>,
    pub total: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FileRootsPage {
    #[serde(default)]
    pub roots: Vec<FileRoot>,
    /// Indexed files at the project root (not folders).
    #[serde(default)]
    pub files: Vec<FileRow>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub qualified_name: String,
    pub file_path: String,
    pub start_line: i64,
    pub language: String,
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchPage {
    pub results: Vec<SearchResult>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UnresolvedRow {
    pub id: i64,
    pub from_node_id: String,
    pub reference_name: String,
    pub reference_kind: String,
    pub line: i64,
    #[serde(default)]
    pub col: i64,
    pub file_path: String,
    pub language: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UnresolvedPage {
    pub refs: Vec<UnresolvedRow>,
    pub total: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UnresolvedKindStat {
    pub kind: String,
    pub count: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct UnresolvedSummary {
    pub total: i64,
    #[serde(default)]
    pub by_kind: Vec<UnresolvedKindStat>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SourceLine {
    pub no: i64,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SourceSlice {
    pub path: String,
    pub from: i64,
    pub to: i64,
    pub total_lines: i64,
    pub lines: Vec<SourceLine>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub community_id: i64,
    pub community_label: Option<String>,
    pub degree: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub kind: String,
    pub confidence: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GraphPayload {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub total_nodes: i64,
    pub truncated: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GraphStreamMeta {
    #[serde(default)]
    pub total_nodes: i64,
    #[serde(default)]
    pub truncated: bool,
    #[serde(default)]
    pub node_count: i64,
    #[serde(default)]
    pub edge_count: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum GraphStreamEvent {
    #[serde(rename = "meta")]
    Meta {
        #[serde(flatten)]
        meta: GraphStreamMeta,
    },
    #[serde(rename = "nodes")]
    Nodes { nodes: Vec<GraphNode> },
    #[serde(rename = "edges")]
    Edges { edges: Vec<GraphEdge> },
    #[serde(rename = "done")]
    Done,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct MemoryRow {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub source: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct MemoryPage {
    #[serde(default)]
    pub memories: Vec<MemoryRow>,
    #[serde(default)]
    pub total: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PricingInfo {
    #[serde(default)]
    pub reference_model: String,
    #[serde(default)]
    pub input_per_mtok: f64,
    #[serde(default)]
    pub output_per_mtok: f64,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub config_path: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SavingsAssumptions {
    #[serde(default)]
    pub exact_tokenizer: bool,
    #[serde(default)]
    pub chars_per_token: f64,
    #[serde(default)]
    pub tokens_per_line: f64,
    #[serde(default)]
    pub avg_file_tokens: f64,
    #[serde(default)]
    pub counterfactual_mode: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ToolSavingsRow {
    pub tool: String,
    #[serde(default)]
    pub calls: i64,
    #[serde(default)]
    pub graph_calls: i64,
    #[serde(default)]
    pub failed_calls: i64,
    #[serde(default)]
    pub tokens_saved_est: i64,
    #[serde(default)]
    pub counterfactual_files: i64,
    #[serde(default)]
    pub counterfactual_tokens_est: i64,
    #[serde(default)]
    pub graph_response_tokens_est: i64,
    #[serde(default)]
    pub avg_duration_ms: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DailySavings {
    pub date: String,
    #[serde(default)]
    pub tokens_saved_est: i64,
    #[serde(default)]
    pub calls: i64,
    #[serde(default)]
    pub graph_calls: i64,
    #[serde(default)]
    pub failed_calls: i64,
    #[serde(default)]
    pub counterfactual_files: i64,
    #[serde(default)]
    pub counterfactual_tokens_est: i64,
    #[serde(default)]
    pub graph_response_tokens_est: i64,
    #[serde(default)]
    pub cost_saved_usd_est: f64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProjectSavingsRow {
    pub project: String,
    #[serde(default)]
    pub calls: i64,
    #[serde(default)]
    pub graph_calls: i64,
    #[serde(default)]
    pub tokens_saved_est: i64,
    #[serde(default)]
    pub counterfactual_files: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ModelSavingsRow {
    pub model: String,
    #[serde(default)]
    pub sessions: i64,
    #[serde(default)]
    pub session_input_tokens: i64,
    #[serde(default)]
    pub tokens_saved_est: i64,
    #[serde(default)]
    pub ax_calls: i64,
    #[serde(default)]
    pub read_calls: i64,
    #[serde(default)]
    pub grep_calls: i64,
    #[serde(default)]
    pub session_cost_usd_est: f64,
    #[serde(default)]
    pub cost_saved_usd_est: f64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RecentCallRow {
    pub id: i64,
    pub tool: String,
    pub project: Option<String>,
    #[serde(default)]
    pub tokens_saved_est: i64,
    #[serde(default)]
    pub counterfactual_tokens_est: i64,
    #[serde(default)]
    pub response_tokens_est: i64,
    #[serde(default)]
    pub counterfactual_files: i64,
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub savings_eligible: bool,
    pub duration_ms: Option<i64>,
    #[serde(default)]
    pub created_at: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SavingsSummary {
    #[serde(default)]
    pub period: String,
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: String,
    #[serde(default)]
    pub mcp_calls: i64,
    #[serde(default)]
    pub graph_calls: i64,
    #[serde(default)]
    pub failed_calls: i64,
    #[serde(default)]
    pub tokens_saved_est: i64,
    #[serde(default)]
    pub net_tokens_saved_est: i64,
    #[serde(default)]
    pub counterfactual_files: i64,
    #[serde(default)]
    pub counterfactual_tokens_est: i64,
    #[serde(default)]
    pub graph_response_tokens_est: i64,
    #[serde(default)]
    pub cost_saved_usd_est: f64,
    #[serde(default)]
    pub graph_response_cost_usd_est: f64,
    #[serde(default)]
    pub counterfactual_cost_usd_est: f64,
    #[serde(default)]
    pub policy_calls: i64,
    #[serde(default)]
    pub success_rate_pct: f64,
    #[serde(default)]
    pub avg_duration_ms: i64,
    #[serde(default)]
    pub projects_active: i64,
    #[serde(default)]
    pub pricing: PricingInfo,
    #[serde(default)]
    pub assumptions: SavingsAssumptions,
    #[serde(default)]
    pub by_tool: Vec<ToolSavingsRow>,
    #[serde(default)]
    pub by_project: Vec<ProjectSavingsRow>,
    #[serde(default)]
    pub by_model: Vec<ModelSavingsRow>,
    #[serde(default)]
    pub daily: Vec<DailySavings>,
    #[serde(default)]
    pub recent_calls: Vec<RecentCallRow>,
    #[serde(default)]
    pub db_path: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PricingSourceStatus {
    pub source: String,
    pub last_success_date: Option<String>,
    pub status: Option<String>,
    pub error: Option<String>,
    pub models_count: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PricingStatus {
    #[serde(default)]
    pub today: String,
    #[serde(default)]
    pub synced_today: bool,
    #[serde(default)]
    pub sources: Vec<PricingSourceStatus>,
    #[serde(default)]
    pub price_rows: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PricingCatalogRow {
    pub date: String,
    pub source: String,
    pub model_id: String,
    pub display_name: Option<String>,
    pub provider: Option<String>,
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PricingCatalogResponse {
    #[serde(default)]
    pub status: PricingStatus,
    #[serde(default)]
    pub models: Vec<PricingCatalogRow>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PricingHistoryPoint {
    pub date: String,
    pub source: String,
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PricingSyncReport {
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub forced: bool,
    #[serde(default)]
    pub skipped: bool,
    #[serde(default)]
    pub openrouter_count: i64,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PolicyRuleRow {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub always_apply: bool,
    #[serde(default)]
    pub body: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PolicyRulesPage {
    #[serde(default)]
    pub rules: Vec<PolicyRuleRow>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PolicySkillRow {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub body: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PolicySkillsPage {
    #[serde(default)]
    pub skills: Vec<PolicySkillRow>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct GateStep {
    pub step: String,
    pub status: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ShipReport {
    pub quality_gate: Option<ShipQualityGate>,
    #[serde(default)]
    pub changed_files: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ShipQualityGate {
    pub passed: bool,
    #[serde(default)]
    pub steps: Vec<GateStep>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LastRunLog {
    pub ok: bool,
    #[serde(default)]
    pub lines: Vec<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ShipStatus {
    pub branch: Option<String>,
    pub report: Option<ShipReport>,
    pub last_run: Option<LastRunLog>,
    #[serde(default)]
    pub evaluating: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShipConfig {
    #[serde(default)]
    pub ship: ShipSection,
    #[serde(default)]
    pub ui: UiSection,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShipSection {
    #[serde(default = "default_branch")]
    pub target_branch: String,
    #[serde(default = "default_port")]
    pub web_port: u16,
    #[serde(default)]
    pub git_roots: Vec<String>,
}

fn default_branch() -> String {
    "main".into()
}
fn default_port() -> u16 {
    7070
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UiSection {
    #[serde(default = "default_true")]
    pub show_savings: bool,
    #[serde(default = "default_true")]
    pub show_agent_terminal: bool,
    #[serde(default)]
    pub verbose_mcp: bool,
    #[serde(default)]
    pub timezone: String,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ShipConfigResponse {
    #[serde(default)]
    pub config: ShipConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct McpTracePath {
    #[serde(default, rename = "path")]
    pub path: String,
    #[serde(default, rename = "projectRoot")]
    pub project_root: String,
    #[serde(default, rename = "projectLabel")]
    pub project_label: String,
    #[serde(default, rename = "logDay")]
    pub log_day: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct McpTraceChunk {
    #[serde(default)]
    pub ok: bool,
    pub day: Option<String>,
    #[serde(default)]
    pub lines: Vec<String>,
    #[serde(default, rename = "hasOlder")]
    pub has_older: bool,
}

#[derive(Debug, Clone)]
pub struct TraceEntry {
    pub id: String,
    pub raw: String,
    pub time: String,
    pub kind: String,
    pub tool: Option<String>,
    pub message: String,
    pub day: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct VersionInfo {
    #[serde(default)]
    pub version: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ReconcileResult {
    #[serde(default)]
    pub ok: bool,
    pub resolved: Option<i64>,
    pub remaining: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LspServerInfo {
    pub id: String,
    #[serde(default)]
    pub available: bool,
    pub command: Option<String>,
    /// Present when a binary/shim is on PATH (even if not runnable).
    pub path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LspStatus {
    #[serde(default)]
    pub servers: Vec<LspServerInfo>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LspEnrichReport {
    #[serde(default)]
    pub examined: i64,
    #[serde(default)]
    pub resolved: i64,
    #[serde(default, rename = "skippedNoServer")]
    pub skipped_no_server: i64,
    #[serde(default, rename = "skippedNoDefinition")]
    pub skipped_no_definition: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LspEnrichResponse {
    #[serde(default)]
    pub ok: bool,
    pub report: Option<LspEnrichReport>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SavingsImportResult {
    #[serde(default)]
    pub claude_sessions: i64,
    #[serde(default)]
    pub cursor_sessions: i64,
    #[serde(default)]
    pub skipped: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AgentSessionInfo {
    #[serde(default)]
    pub sessions: Vec<serde_json::Value>,
}
