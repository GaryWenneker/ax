//! Normalized agent events and the session facts Ax is allowed to keep.
//!
//! Prompts, completions, credentials, and raw tool bodies are not fields on
//! these types. Pi keeps that transcript.

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_CONTEXT_MAX_TOKENS: u32 = 12_000;
pub const DEFAULT_OVERSIZED_TOKENS: u32 = 2_000;
/// Advisory size of an `ax_explore` answer when the graph snippet is not measured.
pub const GRAPH_EXPLORE_TOKEN_ESTIMATE: u32 = 640;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptimizationMode {
    Observe,
    Advise,
    Optimize,
}

impl OptimizationMode {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "advise" => Self::Advise,
            "optimize" => Self::Optimize,
            _ => Self::Observe,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Observe => "observe",
            Self::Advise => "advise",
            Self::Optimize => "optimize",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

impl Confidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AxEventBase {
    pub timestamp: u64,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub operation_id: Option<String>,
    pub lane_name: Option<String>,
    pub source: String,
    pub schema_version: u32,
}

impl AxEventBase {
    pub fn new(timestamp: u64, session_id: impl Into<String>) -> Self {
        Self {
            timestamp,
            session_id: session_id.into(),
            turn_id: None,
            operation_id: None,
            lane_name: None,
            source: "pi".into(),
            schema_version: SCHEMA_VERSION,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AxAgentEvent {
    SessionStarted(AxEventBase),
    SessionEnded(AxEventBase),
    TurnStarted(AxEventBase),
    TurnEnded(AxEventBase),
    ModelRequest(ModelEvent),
    ModelResponse(ModelEvent),
    ToolStarted(ToolStartEvent),
    ToolOutput(ToolOutputEvent),
    ToolEnded(ToolEndEvent),
    Compaction(AxEventBase),
    SessionWrite(AxEventBase),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelEvent {
    #[serde(flatten)]
    pub base: AxEventBase,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub usage: AxModelUsage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolStartEvent {
    #[serde(flatten)]
    pub base: AxEventBase,
    pub tool_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolOutputEvent {
    #[serde(flatten)]
    pub base: AxEventBase,
    pub tool_name: String,
    pub output_token_estimate: Option<u32>,
    pub output_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolEndEvent {
    #[serde(flatten)]
    pub base: AxEventBase,
    pub tool_name: String,
    pub duration_ms: u64,
    pub success: bool,
    pub output_token_estimate: Option<u32>,
    pub output_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AxModelUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UsageSnapshot {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub total_tokens: u64,
}

impl UsageSnapshot {
    pub fn add_model(&mut self, usage: &AxModelUsage) {
        self.input_tokens += usage.input_tokens.unwrap_or(0);
        self.output_tokens += usage.output_tokens.unwrap_or(0);
        self.cached_input_tokens += usage.cached_input_tokens.unwrap_or(0);
        self.total_tokens += usage.total_tokens.unwrap_or_else(|| {
            usage.input_tokens.unwrap_or(0)
                + usage.output_tokens.unwrap_or(0)
                + usage.cached_input_tokens.unwrap_or(0)
        });
    }
}

/// Dollars from the existing catalog pricer. Tool execution itself is not a model cost.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostSnapshot {
    pub currency: String,
    /// Model input/output/cache quote from [`ax_usage::calculate`]. `None` when the rate is unknown.
    pub model_usd: Option<f64>,
    /// Tokens a tool delivered into context, priced at the model input rate. Not added to `model_usd`.
    pub tool_context_usd: Option<f64>,
    pub confidence: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AxSessionState {
    pub session_id: String,
    pub project_id: String,
    pub repository_root: Option<String>,
    pub active_branch: Option<String>,
    pub last_observed_turn_id: Option<String>,
    pub context_version: Option<String>,
    pub cache_namespace: String,
    pub cumulative_usage: UsageSnapshot,
    pub cumulative_cost: CostSnapshot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SectionType {
    Graph,
    Memory,
    Rule,
    Skill,
    Source,
    ToolResult,
    Summary,
    Constraint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextSection {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: SectionType,
    pub priority: u32,
    pub token_estimate: u32,
    pub content: String,
    /// Always-apply rules. Ax does not drop or truncate these to meet the budget.
    #[serde(default)]
    pub mandatory: bool,
    /// Paths this section was derived from. Used for selective cache invalidation.
    #[serde(default)]
    pub source_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextRequest {
    pub session_id: String,
    pub turn_id: String,
    pub user_intent: Option<String>,
    pub repository_root: Option<String>,
    pub changed_files: Vec<String>,
    pub previous_tool_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextResult {
    pub context_version: String,
    pub sections: Vec<ContextSection>,
    pub estimated_tokens: u32,
    pub cache_hit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextCacheKey {
    pub project_id: String,
    pub git_revision: Option<String>,
    pub working_tree_hash: Option<String>,
    pub intent_hash: String,
    pub graph_version: String,
    pub policy_version: String,
    pub skill_version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CachedContext {
    pub key: String,
    pub created_at: u64,
    pub sections: Vec<ContextSection>,
    pub estimated_tokens: u32,
    pub source_hashes: Vec<String>,
    pub project_id: String,
    pub git_revision: Option<String>,
    pub policy_version: String,
    pub skill_version: String,
    pub graph_version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallObservation {
    pub session_id: String,
    pub turn_id: String,
    pub call_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub repository_state: String,
    pub known_graph_entities: Vec<String>,
    pub indexed_paths: Vec<String>,
    pub output_text_for_estimate: Option<String>,
    pub output_token_estimate: Option<u32>,
    pub output_bytes: Option<u64>,
    pub duration_ms: u64,
    pub success: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolAlternative {
    pub tool_name: String,
    pub estimated_current_tokens: u32,
    pub estimated_alternative_tokens: u32,
    pub estimated_reduction_percent: f64,
    pub confidence: Confidence,
    pub reason: String,
    /// Which deterministic rule fired. Phase 5 auto-approval uses this.
    pub rule: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallAdvice {
    pub mode: OptimizationMode,
    pub alternatives: Vec<ToolAlternative>,
    /// Set only in `optimize` mode for a high-confidence allowed rule.
    pub apply: Option<ToolAlternative>,
    /// Process-local cached tool body. Present only in `optimize` mode for an exact repeat.
    /// Not written to the database.
    pub cached_output: Option<String>,
    pub degraded: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolEconomicsRecord {
    pub id: String,
    pub session_id: String,
    pub turn_id: String,
    pub tool_name: String,
    pub input_hash: String,
    pub input_bytes: Option<u64>,
    pub output_bytes: Option<u64>,
    pub output_tokens: u32,
    pub token_confidence: String,
    pub duration_ms: u64,
    /// Tool processes do not spend model tokens. This stays zero.
    pub execution_cost_usd: f64,
    /// Estimated cost of delivering `output_tokens` into a later model context.
    pub context_cost_usd: Option<f64>,
    pub cache_hit: bool,
    pub repeated: bool,
    pub alternative: Option<ToolAlternative>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepeatRecord {
    pub repeat_count: u32,
    pub first_call_id: String,
    pub latest_call_id: String,
    pub tokens_repeated: u64,
    pub estimated_repeated_cost: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BudgetSnapshot {
    pub spent: f64,
    pub remaining: f64,
    pub projected: f64,
    pub currency: String,
    pub days_elapsed: u32,
    pub days_remaining: u32,
}
