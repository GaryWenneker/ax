//! Pi integration entry point.
//!
//! This is an observer and advisor. It does not run an agent loop, call a
//! model, or execute tools. Failures are logged and swallowed unless `strict`
//! is set. `prepareRequest` context replacement in Pi persists into the loop
//! transcript, so this crate returns context sections for a tool or resource
//! boundary and does not ask the host to rewrite `session.agent.state.messages`.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use ax_usage::ModelPricing;
use serde_json::Value;

use crate::context::{cache_key_string, AxContextEngine, ContextCache};
use crate::economics::{
    advise_observation, advise_with, context_token_cost, estimate_text, identity_hash,
    may_change_state, model_cost, replayable, RepeatDetector,
};
use crate::events::{self, arguments_for_hash, output_text_for_estimate};
use crate::report::{
    apply_display_currency, economics_from, economics_of, format_economics, format_optimization,
    optimization_from, optimization_of, EconomicsReport, OptimizationReport, ReportRows,
};
use crate::store::{self, ModelRow, OptimizationRow, PersistedSnapshot, SessionRow, TurnRow};
use crate::types::{
    AxAgentEvent, AxSessionState, BudgetSnapshot, ContextCacheKey, ContextRequest, ContextResult,
    ContextSection, CostSnapshot, OptimizationMode, SectionType, ToolCallAdvice,
    ToolCallObservation, ToolEconomicsRecord, UsageSnapshot, DEFAULT_CONTEXT_MAX_TOKENS,
    DEFAULT_OVERSIZED_TOKENS,
};

const LOG_TAG: &str = "AX_PI_INTEGRATION_ERROR";

#[derive(Debug)]
pub struct PiError {
    pub message: String,
}

impl std::fmt::Display for PiError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

#[derive(Debug, Clone)]
pub struct PiOptions {
    pub enabled: bool,
    pub mode: OptimizationMode,
    pub context_max_tokens: u32,
    pub oversized_token_threshold: u32,
    pub project_id: String,
    pub repository_root: Option<String>,
    pub session_id: Option<String>,
    pub monthly_budget: Option<f64>,
    pub currency: String,
    pub usd_per_eur: Option<f64>,
    pub strict: bool,
    pub pricing: Option<ModelPricing>,
    pub db_path: Option<PathBuf>,
    pub days_elapsed: u32,
    pub days_remaining: u32,
}

impl Default for PiOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: OptimizationMode::Observe,
            context_max_tokens: env_u32("AX_CONTEXT_MAX_TOKENS", DEFAULT_CONTEXT_MAX_TOKENS),
            oversized_token_threshold: DEFAULT_OVERSIZED_TOKENS,
            project_id: "local".into(),
            repository_root: None,
            session_id: None,
            monthly_budget: None,
            currency: "EUR".into(),
            usd_per_eur: None,
            strict: false,
            pricing: None,
            db_path: None,
            days_elapsed: 1,
            days_remaining: 1,
        }
    }
}

impl PiOptions {
    pub fn from_config_value(value: &Value) -> Self {
        let mut options = Self::default();
        let pi = value.get("pi").unwrap_or(value);
        if let Some(enabled) = pi.get("integration").and_then(Value::as_bool) {
            options.enabled = enabled;
        }
        if let Some(mode) = pi.pointer("/optimization/mode").and_then(Value::as_str) {
            options.mode = OptimizationMode::parse(mode);
        }
        if let Some(monthly) = value.pointer("/budget/monthly").and_then(Value::as_f64) {
            options.monthly_budget = Some(monthly);
        }
        if let Some(currency) = value.pointer("/budget/currency").and_then(Value::as_str) {
            options.currency = currency.to_string();
        }
        if let Some(rate) = value.pointer("/budget/usdPerEur").and_then(Value::as_f64) {
            options.usd_per_eur = Some(rate);
        }
        options
    }
}

pub struct PiIntegration {
    options: PiOptions,
    state: Mutex<State>,
    broken: AtomicBool,
}

struct State {
    sessions: HashMap<String, AxSessionState>,
    turns: Vec<TurnRow>,
    tools: Vec<ToolEconomicsRecord>,
    models: Vec<ModelRow>,
    optimizations: Vec<OptimizationRow>,
    repeats: RepeatDetector,
    cache: ContextCache,
    active_turn: HashMap<String, String>,
    turn_seq: HashMap<String, u32>,
    knowledge: Vec<ContextSection>,
    /// Shared so a tool event can lend them to the advice without a copy.
    entities: Arc<[String]>,
    /// Content hash of `entities`, part of the context cache key.
    entities_version: String,
    indexed_paths: Arc<[String]>,
    repository_state: String,
    output_cache: OutputCache,
    pending_tools: PendingTools,
    /// First and last event time per session, in milliseconds.
    session_times: HashMap<String, (u64, u64)>,
    /// Sequence for generated tool call ids when the host sends none.
    tool_seq: u64,
    started: bool,
    model_seq: u64,
    /// Unique per integration instance. Generated turn, model and tool ids
    /// carry it, so a later run on the same database never reuses an id.
    run_id: String,
}

/// Tool calls that started and have not ended yet, holding their arguments
/// until the end event. A start with a call id pairs with the end carrying the
/// same id; starts without one pair first-in, first-out per session and tool.
#[derive(Default)]
struct PendingTools {
    by_id: HashMap<String, (String, Value)>,
    unnamed: HashMap<(String, String), VecDeque<Value>>,
    count: usize,
}

/// Unfinished tool calls kept at most. Starts beyond this are not held; their
/// end events fall back to the arguments they carry themselves.
const MAX_PENDING_TOOLS: usize = 1024;

/// Tool output text kept for exact-repeat replay, in bytes.
const MAX_OUTPUT_CACHE_BYTES: usize = 16 * 1024 * 1024;

/// Read-only tool output kept for exact-repeat replay, oldest dropped first
/// once it holds more than [`MAX_OUTPUT_CACHE_BYTES`].
#[derive(Default)]
struct OutputCache {
    entries: HashMap<String, String>,
    order: VecDeque<String>,
    bytes: usize,
}

impl OutputCache {
    fn get(&self, identity: &str) -> Option<&String> {
        self.entries.get(identity)
    }

    fn insert(&mut self, identity: String, text: String) {
        if text.len() > MAX_OUTPUT_CACHE_BYTES {
            return;
        }
        if let Some(old) = self.entries.remove(&identity) {
            self.bytes -= old.len();
            self.order.retain(|key| key != &identity);
        }
        while self.bytes + text.len() > MAX_OUTPUT_CACHE_BYTES {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(old) = self.entries.remove(&oldest) {
                self.bytes -= old.len();
            }
        }
        self.bytes += text.len();
        self.order.push_back(identity.clone());
        self.entries.insert(identity, text);
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.bytes = 0;
    }

    #[cfg(test)]
    fn values(&self) -> impl Iterator<Item = &String> {
        self.entries.values()
    }
}

impl PendingTools {
    fn start(&mut self, session: &str, tool: &str, call_id: Option<String>, arguments: Value) {
        if self.count >= MAX_PENDING_TOOLS {
            return;
        }
        match call_id {
            Some(id) => {
                if self
                    .by_id
                    .insert(id, (session.to_string(), arguments))
                    .is_none()
                {
                    self.count += 1;
                }
            }
            None => {
                self.unnamed
                    .entry((session.to_string(), tool.to_string()))
                    .or_default()
                    .push_back(arguments);
                self.count += 1;
            }
        }
    }

    fn finish(&mut self, session: &str, tool: &str, call_id: Option<&str>) -> Option<Value> {
        let found = match call_id {
            Some(id) => self.by_id.remove(id).map(|(_, arguments)| arguments),
            None => {
                let key = (session.to_string(), tool.to_string());
                let queue = self.unnamed.get_mut(&key)?;
                let found = queue.pop_front();
                if queue.is_empty() {
                    self.unnamed.remove(&key);
                }
                found
            }
        };
        if found.is_some() {
            self.count -= 1;
        }
        found
    }

    fn drop_session(&mut self, session: &str) {
        self.by_id.retain(|_, (owner, _)| owner != session);
        self.unnamed.retain(|(owner, _), _| owner != session);
        self.count = self.by_id.len() + self.unnamed.values().map(VecDeque::len).sum::<usize>();
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.count
    }
}

impl PiIntegration {
    pub fn start(&self) -> Result<(), PiError> {
        self.guard(|| {
            self.state.lock().map_err(|err| err.to_string())?.started = true;
            Ok(())
        })
    }

    pub async fn stop(&self) -> Result<(), PiError> {
        let snapshot = match self.state.lock() {
            Ok(state) => snapshot_of(&state),
            Err(err) => return self.note(err.to_string()),
        };
        if let Some(path) = &self.options.db_path {
            if let Err(err) = store::flush(path, &snapshot).await {
                return self.note(err);
            }
        }
        Ok(())
    }

    pub fn ingest(&self, event: &Value) -> Result<(), PiError> {
        if !self.options.enabled {
            return Ok(());
        }
        self.guard(|| {
            let fallback = self
                .options
                .session_id
                .clone()
                .unwrap_or_else(|| "pi".into());
            let translated = events::translate(event, &fallback, now_ms()).map_err(|err| err.0)?;
            let mut state = self.state.lock().map_err(|err| err.to_string())?;
            for item in translated {
                apply_event(&mut state, &self.options, item, event);
            }
            Ok(())
        })
    }

    pub fn get_session_context(&self, request: &ContextRequest) -> Result<ContextResult, PiError> {
        self.guard_or(
            ContextResult {
                context_version: String::new(),
                sections: Vec::new(),
                estimated_tokens: 0,
                cache_hit: false,
            },
            |fallback| fallback,
            || {
                let mut state = self.state.lock().map_err(|err| err.to_string())?;
                let key = ContextCacheKey {
                    project_id: self.options.project_id.clone(),
                    git_revision: Some(state.repository_state.clone()),
                    working_tree_hash: None,
                    intent_hash: crate::context::intent_hash(
                        request.user_intent.as_deref().unwrap_or(""),
                    ),
                    graph_version: format!(
                        "{}|{}",
                        state.entities_version,
                        knowledge_version(&state.knowledge, |kind| !matches!(
                            kind,
                            SectionType::Rule | SectionType::Constraint | SectionType::Skill
                        ))
                    ),
                    policy_version: knowledge_version(&state.knowledge, |kind| {
                        matches!(kind, SectionType::Rule | SectionType::Constraint)
                    }),
                    skill_version: knowledge_version(&state.knowledge, |kind| {
                        matches!(kind, SectionType::Skill)
                    }),
                };
                if let Some(hit) = state.cache.get(&key) {
                    return Ok(ContextResult {
                        context_version: hit.key.clone(),
                        sections: hit.sections.clone(),
                        estimated_tokens: hit.estimated_tokens,
                        cache_hit: true,
                    });
                }
                let engine = AxContextEngine::new(self.options.context_max_tokens);
                let mut result = engine.select(request, state.knowledge.clone());
                result.context_version = cache_key_string(&key);
                let paths: Vec<String> = result
                    .sections
                    .iter()
                    .flat_map(|section| section.source_paths.clone())
                    .collect();
                state.cache.insert(
                    key.clone(),
                    crate::types::CachedContext {
                        key: result.context_version.clone(),
                        created_at: now_ms(),
                        sections: result.sections.clone(),
                        estimated_tokens: result.estimated_tokens,
                        source_hashes: paths,
                        project_id: key.project_id.clone(),
                        git_revision: key.git_revision.clone(),
                        policy_version: key.policy_version.clone(),
                        skill_version: key.skill_version.clone(),
                        graph_version: key.graph_version.clone(),
                    },
                );
                if let Some(session) = state.sessions.get_mut(&request.session_id) {
                    session.context_version = Some(result.context_version.clone());
                }
                Ok(result)
            },
        )
    }

    pub fn advise_tool_call(
        &self,
        observation: &ToolCallObservation,
    ) -> Result<ToolCallAdvice, PiError> {
        self.guard_or(
            empty_advice(self.options.mode),
            |advice| advice,
            || {
                let state = self.state.lock().map_err(|err| err.to_string())?;
                let identity = identity_hash(
                    &observation.tool_name,
                    &observation.arguments,
                    &observation.repository_state,
                );
                let mut advice = advise_observation(
                    observation,
                    state.repeats.contains(&identity),
                    self.options.mode,
                    self.options.oversized_token_threshold,
                    true,
                );
                let cached = state.output_cache.get(&identity);
                if withhold_replay(&mut advice, self.options.mode, cached.is_some()) {
                    advice.cached_output = cached.cloned();
                }
                Ok(advice)
            },
        )
    }

    pub fn record_tool_result(&self, observation: &ToolCallObservation) -> Result<(), PiError> {
        self.guard(|| {
            let mut state = self.state.lock().map_err(|err| err.to_string())?;
            record_tool(
                &mut state,
                &self.options,
                observation,
                &observation.known_graph_entities,
                &observation.indexed_paths,
            );
            Ok(())
        })
    }

    pub fn get_cost(&self, session_id: &str) -> Result<CostSnapshot, PiError> {
        self.guard_or(
            CostSnapshot {
                currency: "USD".into(),
                model_usd: None,
                tool_context_usd: None,
                confidence: "estimated".into(),
            },
            |cost| cost,
            || {
                let state = self.state.lock().map_err(|err| err.to_string())?;
                Ok(state
                    .sessions
                    .get(session_id)
                    .map(|session| session.cumulative_cost.clone())
                    .unwrap_or(CostSnapshot {
                        currency: "USD".into(),
                        model_usd: None,
                        tool_context_usd: None,
                        confidence: "estimated".into(),
                    }))
            },
        )
    }

    pub fn economics(&self, session_id: Option<&str>) -> Result<EconomicsReport, PiError> {
        let currency = self.options.currency.as_str();
        let usd_per_eur = self.options.usd_per_eur;
        let mut empty = empty_report("USD");
        apply_display_currency(&mut empty, currency, usd_per_eur);
        self.guard_or(
            empty,
            |report| report,
            || {
                let state = self.state.lock().map_err(|err| err.to_string())?;
                // Figures stay in USD until the one display conversion below.
                let rows = ReportRows {
                    turns: &state.turns,
                    tools: &state.tools,
                    models: &state.models,
                    optimizations: &state.optimizations,
                };
                let mut report = economics_of(&rows, session_id, "USD", self.budget_of(&state));
                apply_display_currency(&mut report, currency, usd_per_eur);
                Ok(report)
            },
        )
    }

    pub fn optimization_report(&self) -> Result<OptimizationReport, PiError> {
        self.guard_or(
            OptimizationReport {
                items: Vec::new(),
                total_estimated_avoidable_tokens: 0,
            },
            |report| report,
            || {
                let state = self.state.lock().map_err(|err| err.to_string())?;
                Ok(optimization_of(&state.optimizations))
            },
        )
    }

    pub fn format_economics_text(&self, session_id: Option<&str>) -> Result<String, PiError> {
        Ok(format_economics(&self.economics(session_id)?))
    }

    pub fn format_optimization_text(&self) -> Result<String, PiError> {
        Ok(format_optimization(&self.optimization_report()?))
    }

    pub fn set_knowledge(&self, sections: Vec<ContextSection>) -> Result<(), PiError> {
        self.guard(|| {
            self.state.lock().map_err(|err| err.to_string())?.knowledge = sections;
            Ok(())
        })
    }

    pub fn set_graph_entities(&self, entities: Vec<String>) -> Result<(), PiError> {
        self.guard(|| {
            let version = list_version(&entities);
            let mut state = self.state.lock().map_err(|err| err.to_string())?;
            state.entities = Arc::from(entities);
            state.entities_version = version;
            Ok(())
        })
    }

    pub fn set_indexed_paths(&self, paths: Vec<String>) -> Result<(), PiError> {
        self.guard(|| {
            self.state
                .lock()
                .map_err(|err| err.to_string())?
                .indexed_paths = Arc::from(paths);
            Ok(())
        })
    }

    pub fn set_repository_state(&self, state_hash: impl Into<String>) -> Result<(), PiError> {
        self.guard(|| {
            self.state
                .lock()
                .map_err(|err| err.to_string())?
                .repository_state = state_hash.into();
            Ok(())
        })
    }

    pub fn invalidate_context_paths(&self, paths: &[String]) -> Result<(), PiError> {
        self.guard(|| {
            self.state
                .lock()
                .map_err(|err| err.to_string())?
                .cache
                .invalidate_paths(paths);
            Ok(())
        })
    }

    /// Next Ax operation fails internally. Non-strict mode still returns success.
    pub fn fail_next(&self) {
        self.broken.store(true, Ordering::SeqCst);
    }

    /// The budget in USD, like the spend it is compared with. `None` when the
    /// monthly budget's currency has no rate; no exchange rate is assumed.
    fn budget_of(&self, state: &State) -> Option<BudgetSnapshot> {
        let monthly = crate::economics::amount_in_usd(
            self.options.monthly_budget?,
            &self.options.currency,
            self.options.usd_per_eur,
        )?;
        let spent = state
            .sessions
            .values()
            .filter_map(|session| session.cumulative_cost.model_usd)
            .sum();
        let mut snapshot = crate::economics::budget_snapshot(
            spent,
            self.options.days_elapsed,
            self.options.days_remaining,
            monthly,
        );
        snapshot.currency = "USD".into();
        Some(snapshot)
    }

    fn guard(&self, body: impl FnOnce() -> Result<(), String>) -> Result<(), PiError> {
        if self.consume_broken() {
            return self.note("injected failure".into());
        }
        match body() {
            Ok(()) => Ok(()),
            Err(err) => self.note(err),
        }
    }

    fn guard_or<T>(
        &self,
        fallback: T,
        into_fallback: impl Fn(T) -> T,
        body: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, PiError> {
        if self.consume_broken() {
            tracing::warn!("{LOG_TAG} injected failure");
            return if self.options.strict {
                Err(PiError {
                    message: "injected failure".into(),
                })
            } else {
                Ok(into_fallback(fallback))
            };
        }
        match body() {
            Ok(value) => Ok(value),
            Err(err) => {
                tracing::warn!("{LOG_TAG} {err}");
                if self.options.strict {
                    Err(PiError { message: err })
                } else {
                    Ok(into_fallback(fallback))
                }
            }
        }
    }

    fn consume_broken(&self) -> bool {
        self.broken.swap(false, Ordering::SeqCst)
    }

    fn note(&self, message: String) -> Result<(), PiError> {
        tracing::warn!("{LOG_TAG} {message}");
        if self.options.strict {
            Err(PiError { message })
        } else {
            Ok(())
        }
    }
}

/// Content hash of a list, with each item length-prefixed so `["a,b"]` and
/// `["a", "b"]` differ.
fn list_version(items: &[String]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&(items.len() as u64).to_le_bytes());
    for item in items {
        hasher.update(&(item.len() as u64).to_le_bytes());
        hasher.update(item.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

/// Content hash of the knowledge sections whose kind `include` accepts, so a
/// changed rule, skill, memory or source section gives a new cache key.
fn knowledge_version(
    sections: &[ContextSection],
    include: impl Fn(&SectionType) -> bool,
) -> String {
    let mut hasher = blake3::Hasher::new();
    for section in sections.iter().filter(|section| include(&section.kind)) {
        for part in [section.id.as_str(), section.content.as_str()] {
            hasher.update(&(part.len() as u64).to_le_bytes());
            hasher.update(part.as_bytes());
        }
        hasher.update(&section.priority.to_le_bytes());
        hasher.update(&[u8::from(section.mandatory)]);
        for path in &section.source_paths {
            hasher.update(&(path.len() as u64).to_le_bytes());
            hasher.update(path.as_bytes());
        }
    }
    hasher.finalize().to_hex().to_string()
}

fn empty_advice(mode: OptimizationMode) -> ToolCallAdvice {
    ToolCallAdvice {
        mode,
        alternatives: Vec::new(),
        apply: None,
        cached_output: None,
        degraded: true,
    }
}

fn empty_report(currency: &str) -> EconomicsReport {
    EconomicsReport {
        session_id: None,
        turns: 0,
        models: Vec::new(),
        input_tokens: 0,
        output_tokens: 0,
        cached_input_tokens: 0,
        tool_output_tokens: std::collections::BTreeMap::new(),
        repeated_output_tokens: 0,
        estimated_model_cost: None,
        potential_optimization_cost: None,
        largest_tool: None,
        currency: currency.into(),
        budget: None,
    }
}

pub fn create_pi_integration(options: PiOptions) -> PiIntegration {
    PiIntegration {
        options,
        broken: AtomicBool::new(false),
        state: Mutex::new(State {
            sessions: HashMap::new(),
            turns: Vec::new(),
            tools: Vec::new(),
            models: Vec::new(),
            optimizations: Vec::new(),
            repeats: RepeatDetector::default(),
            cache: ContextCache::default(),
            active_turn: HashMap::new(),
            turn_seq: HashMap::new(),
            knowledge: Vec::new(),
            entities: Arc::from(Vec::new()),
            entities_version: list_version(&[]),
            indexed_paths: Arc::from(Vec::new()),
            repository_state: "unknown".into(),
            output_cache: OutputCache::default(),
            pending_tools: PendingTools::default(),
            session_times: HashMap::new(),
            tool_seq: 0,
            started: false,
            model_seq: 0,
            run_id: uuid::Uuid::new_v4().simple().to_string(),
        }),
    }
}

fn apply_event(state: &mut State, options: &PiOptions, event: AxAgentEvent, raw: &Value) {
    let base = event.base();
    let times = state
        .session_times
        .entry(base.session_id.clone())
        .or_insert((base.timestamp, base.timestamp));
    times.0 = times.0.min(base.timestamp);
    times.1 = times.1.max(base.timestamp);
    match event {
        AxAgentEvent::SessionStarted(base) => {
            state
                .sessions
                .entry(base.session_id.clone())
                .or_insert_with(|| session_state(options, &base.session_id));
        }
        AxAgentEvent::SessionEnded(base) => {
            if let Some(session) = state.sessions.get_mut(&base.session_id) {
                session.cumulative_cost.currency = "USD".into();
            }
            state.pending_tools.drop_session(&base.session_id);
        }
        AxAgentEvent::TurnStarted(base) => {
            let session_id = base.session_id.clone();
            state
                .sessions
                .entry(session_id.clone())
                .or_insert_with(|| session_state(options, &session_id));
            let seq = state.turn_seq.entry(session_id.clone()).or_insert(0);
            *seq += 1;
            let turn_id = base
                .turn_id
                .clone()
                .unwrap_or_else(|| format!("{session_id}-{}-turn-{seq}", state.run_id));
            state
                .active_turn
                .insert(session_id.clone(), turn_id.clone());
            if let Some(session) = state.sessions.get_mut(&session_id) {
                session.last_observed_turn_id = Some(turn_id.clone());
            }
            state.turns.push(TurnRow {
                id: turn_id,
                session_id,
                sequence: *seq,
                started_at: Some(base.timestamp),
                ended_at: None,
                input_tokens: 0,
                output_tokens: 0,
                estimated_cost: None,
            });
        }
        AxAgentEvent::TurnEnded(base) => {
            let turn_id = base
                .turn_id
                .clone()
                .or_else(|| state.active_turn.get(&base.session_id).cloned());
            if let Some(turn_id) = turn_id {
                if let Some(turn) = state.turns.iter_mut().find(|turn| turn.id == turn_id) {
                    turn.ended_at = Some(base.timestamp);
                }
            }
            // A tool that has not ended by the end of its turn never will.
            state.pending_tools.drop_session(&base.session_id);
        }
        AxAgentEvent::ModelResponse(model) => {
            state.model_seq += 1;
            let cost = model_cost(
                model.provider.as_deref(),
                model.model.as_deref(),
                &model.usage,
                &model.base.session_id,
                model.base.turn_id.as_deref().or_else(|| {
                    state
                        .active_turn
                        .get(&model.base.session_id)
                        .map(String::as_str)
                }),
                options.pricing,
            );
            let turn_id = model
                .base
                .turn_id
                .clone()
                .or_else(|| state.active_turn.get(&model.base.session_id).cloned());
            if let Some(turn_id) = &turn_id {
                if let Some(turn) = state.turns.iter_mut().find(|turn| &turn.id == turn_id) {
                    turn.input_tokens = turn
                        .input_tokens
                        .saturating_add(model.usage.input_tokens.unwrap_or(0));
                    turn.output_tokens = turn
                        .output_tokens
                        .saturating_add(model.usage.output_tokens.unwrap_or(0));
                    if let Some(cost) = cost {
                        turn.estimated_cost = Some(turn.estimated_cost.unwrap_or(0.0) + cost);
                    }
                }
            }
            state
                .sessions
                .entry(model.base.session_id.clone())
                .or_insert_with(|| session_state(options, &model.base.session_id));
            if let Some(session) = state.sessions.get_mut(&model.base.session_id) {
                session.cumulative_usage.add_model(&model.usage);
                if let Some(cost) = cost {
                    session.cumulative_cost.model_usd =
                        Some(session.cumulative_cost.model_usd.unwrap_or(0.0) + cost);
                    session.cumulative_cost.confidence = "estimated".into();
                }
            }
            state.models.push(ModelRow {
                id: format!(
                    "{}-{}-model-{}",
                    model.base.session_id, state.run_id, state.model_seq
                ),
                session_id: model.base.session_id,
                turn_id,
                provider: model.provider,
                model: model.model,
                input_tokens: model.usage.input_tokens,
                output_tokens: model.usage.output_tokens,
                cached_input_tokens: model.usage.cached_input_tokens,
                estimated_cost: cost,
                created_at: model.base.timestamp,
            });
        }
        AxAgentEvent::ToolStarted(start) => {
            state.pending_tools.start(
                &start.base.session_id,
                &start.tool_name,
                start.base.operation_id.clone(),
                arguments_for_hash(raw),
            );
        }
        AxAgentEvent::ToolEnded(end) => {
            let arguments = state
                .pending_tools
                .finish(
                    &end.base.session_id,
                    &end.tool_name,
                    end.base.operation_id.as_deref(),
                )
                .unwrap_or_else(|| arguments_for_hash(raw));
            let call_id = match end.base.operation_id.clone() {
                Some(id) => id,
                None => {
                    state.tool_seq += 1;
                    format!("{}-{}-{}", end.tool_name, state.run_id, state.tool_seq)
                }
            };
            let turn_id = end
                .base
                .turn_id
                .clone()
                .or_else(|| state.active_turn.get(&end.base.session_id).cloned())
                .unwrap_or_else(|| "turn".into());
            let output_text = output_text_for_estimate(raw);
            let tokens = end
                .output_token_estimate
                .or_else(|| output_text.as_deref().map(estimate_text));
            let entities = Arc::clone(&state.entities);
            let indexed_paths = Arc::clone(&state.indexed_paths);
            record_tool(
                state,
                options,
                &ToolCallObservation {
                    session_id: end.base.session_id,
                    turn_id,
                    call_id,
                    tool_name: end.tool_name,
                    arguments,
                    repository_state: state.repository_state.clone(),
                    known_graph_entities: Vec::new(),
                    indexed_paths: Vec::new(),
                    output_text_for_estimate: output_text,
                    output_token_estimate: tokens,
                    output_bytes: end.output_bytes,
                    duration_ms: end.duration_ms,
                    success: end.success,
                },
                &entities,
                &indexed_paths,
            );
        }
        AxAgentEvent::ModelRequest(_)
        | AxAgentEvent::ToolOutput(_)
        | AxAgentEvent::Compaction(_)
        | AxAgentEvent::SessionWrite(_) => {}
    }
}

/// In optimize mode, withdraws an exact-repeat approval when nothing safe is
/// cached for the call (the earlier call failed, or a later call may have
/// changed its result), so the call runs. Returns true when the advice still
/// replays a cached result.
fn withhold_replay(advice: &mut ToolCallAdvice, mode: OptimizationMode, cached: bool) -> bool {
    let replaying = advice
        .apply
        .as_ref()
        .is_some_and(|item| item.rule == "exact-repeat");
    if mode != OptimizationMode::Optimize || !replaying {
        return false;
    }
    if !cached {
        advice.apply = None;
    }
    cached
}

fn record_tool(
    state: &mut State,
    options: &PiOptions,
    observation: &ToolCallObservation,
    entities: &[String],
    indexed_paths: &[String],
) {
    let identity = identity_hash(
        &observation.tool_name,
        &observation.arguments,
        &observation.repository_state,
    );
    let output_tokens = observation.output_token_estimate.unwrap_or_else(|| {
        observation
            .output_text_for_estimate
            .as_deref()
            .map(estimate_text)
            .unwrap_or(0)
    });
    let context_cost = context_token_cost(output_tokens, options.pricing);
    let already = state.repeats.contains(&identity);
    let had_cached = state.output_cache.get(&identity).is_some();
    let (repeated, _) =
        state
            .repeats
            .observe(&identity, &observation.call_id, output_tokens, context_cost);
    // Observe mode shows no advice but still records what it would have said.
    let recorded_mode = match options.mode {
        OptimizationMode::Observe => OptimizationMode::Advise,
        mode => mode,
    };
    let mut advice = advise_with(
        observation,
        entities,
        indexed_paths,
        already,
        recorded_mode,
        options.oversized_token_threshold,
        true,
    );
    // Log the decision advise_tool_call made: a replay only counts as
    // accepted when a cached result existed before this call.
    withhold_replay(&mut advice, options.mode, had_cached);
    if may_change_state(&observation.tool_name) {
        // An edit, shell command or unknown tool may have changed what the
        // cached reads saw, so none of them can be replayed any more.
        state.output_cache.clear();
    } else if options.mode == OptimizationMode::Optimize && replayable(observation) {
        if let Some(text) = &observation.output_text_for_estimate {
            state.output_cache.insert(identity, text.clone());
        }
    }
    if let Some(alternative) = advice
        .alternatives
        .first()
        .cloned()
        .or(advice.apply.clone())
    {
        state.optimizations.push(OptimizationRow {
            id: format!("opt-{}", observation.call_id),
            tool_call_id: observation.call_id.clone(),
            accepted: advice.apply.is_some(),
            alternative,
        });
    }
    if let Some(cost) = context_cost {
        if let Some(session) = state.sessions.get_mut(&observation.session_id) {
            session.cumulative_cost.tool_context_usd =
                Some(session.cumulative_cost.tool_context_usd.unwrap_or(0.0) + cost);
        }
    }
    state.tools.push(ToolEconomicsRecord {
        id: observation.call_id.clone(),
        session_id: observation.session_id.clone(),
        turn_id: observation.turn_id.clone(),
        tool_name: observation.tool_name.clone(),
        input_hash: identity_hash(
            &observation.tool_name,
            &observation.arguments,
            &observation.repository_state,
        ),
        input_bytes: None,
        output_bytes: observation.output_bytes,
        output_tokens,
        token_confidence: "estimated".into(),
        duration_ms: observation.duration_ms,
        execution_cost_usd: 0.0,
        context_cost_usd: context_cost,
        cache_hit: false,
        repeated,
        alternative: None,
    });
}

fn session_state(options: &PiOptions, session_id: &str) -> AxSessionState {
    AxSessionState {
        session_id: session_id.into(),
        project_id: options.project_id.clone(),
        repository_root: options.repository_root.clone(),
        active_branch: None,
        last_observed_turn_id: None,
        context_version: None,
        cache_namespace: format!("{}:{session_id}", options.project_id),
        cumulative_usage: UsageSnapshot::default(),
        // `model_usd` and `tool_context_usd` are USD figures.
        cumulative_cost: CostSnapshot {
            currency: "USD".into(),
            model_usd: None,
            tool_context_usd: None,
            confidence: "estimated".into(),
        },
    }
}

fn snapshot_of(state: &State) -> PersistedSnapshot {
    PersistedSnapshot {
        sessions: state
            .sessions
            .values()
            .map(|session| {
                let (created_at, updated_at) = state
                    .session_times
                    .get(&session.session_id)
                    .copied()
                    .unwrap_or_else(|| {
                        let now = now_ms();
                        (now, now)
                    });
                SessionRow {
                    id: session.session_id.clone(),
                    project_id: session.project_id.clone(),
                    created_at,
                    updated_at,
                    repository_root: session.repository_root.clone(),
                    active_branch: session.active_branch.clone(),
                    cache_namespace: session.cache_namespace.clone(),
                }
            })
            .collect(),
        turns: state.turns.clone(),
        tools: state.tools.clone(),
        models: state.models.clone(),
        optimizations: state.optimizations.clone(),
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn env_u32(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

pub async fn read_economics(
    path: &Path,
    session: Option<&str>,
    currency: &str,
    usd_per_eur: Option<f64>,
) -> Result<(String, Value), String> {
    let snapshot = store::load(path).await?;
    let mut report = economics_from(&snapshot, session, currency, None);
    apply_display_currency(&mut report, currency, usd_per_eur);
    let text = format_economics(&report);
    let json = serde_json::to_value(&report).map_err(|err| err.to_string())?;
    Ok((text, json))
}

/// Record a read-guard recommendation. Missing databases and Ax errors do not
/// change the guard decision.
pub fn read_guard_ids(db: &Path, fallback_session: &str) -> (String, String) {
    let Some(parent) = db.parent() else {
        return (fallback_session.to_string(), "read-guard".into());
    };
    let Ok(text) = std::fs::read_to_string(parent.join("turns").join("current.json")) else {
        return (fallback_session.to_string(), "read-guard".into());
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return (fallback_session.to_string(), "read-guard".into());
    };
    let session = value
        .get("session_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let turn = value
        .get("turn_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if session.is_empty() || turn.is_empty() {
        return (fallback_session.to_string(), "read-guard".into());
    }
    (session.to_string(), turn.to_string())
}

/// A tool call id for a read-guard row: the time, the process id and a
/// per-process counter, so hooks that fire in the same millisecond, in one
/// process or several, get distinct ids.
fn read_guard_call_id() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("read-guard-{}-{}-{sequence}", now_ms(), std::process::id())
}

pub async fn note_read_guard(
    db: &Path,
    session_id: &str,
    tool_name: &str,
    arguments: Value,
    known_graph_entities: Vec<String>,
    indexed_paths: Vec<String>,
) -> Result<(), String> {
    if !db.is_file() {
        return Ok(());
    }
    let (session_id, turn_id) = read_guard_ids(db, session_id);
    let observation = ToolCallObservation {
        session_id,
        turn_id,
        call_id: read_guard_call_id(),
        tool_name: tool_name.to_string(),
        arguments,
        repository_state: String::new(),
        known_graph_entities,
        indexed_paths,
        output_text_for_estimate: None,
        output_token_estimate: None,
        output_bytes: None,
        duration_ms: 0,
        success: true,
    };
    let advice = advise_observation(
        &observation,
        false,
        OptimizationMode::Advise,
        DEFAULT_OVERSIZED_TOKENS,
        true,
    );
    let Some(alternative) = advice.alternatives.into_iter().next() else {
        return Ok(());
    };
    store::record_advice(db, &observation, &alternative).await
}

pub async fn read_optimization(path: &Path) -> Result<(String, Value), String> {
    let snapshot = store::load(path).await?;
    let report = optimization_from(&snapshot);
    let text = format_optimization(&report);
    let json = serde_json::to_value(&report).map_err(|err| err.to_string())?;
    Ok((text, json))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn integration() -> PiIntegration {
        create_pi_integration(PiOptions {
            session_id: Some("s".into()),
            ..PiOptions::default()
        })
    }

    fn pending_count(pi: &PiIntegration) -> usize {
        let state = pi.state.lock().unwrap();
        state.pending_tools.len()
    }

    #[test]
    fn tools_that_never_end_are_dropped_when_the_session_ends() {
        let pi = integration();
        pi.ingest(&json!({"type": "agent_start"})).unwrap();
        pi.ingest(&json!({"type": "tool_execution_start", "toolName": "rg", "toolCallId": "c1", "args": {"q": 1}}))
            .unwrap();
        pi.ingest(&json!({"type": "tool_execution_start", "toolName": "rg", "args": {"q": 2}}))
            .unwrap();
        assert!(pending_count(&pi) > 0);
        pi.ingest(&json!({"type": "agent_end"})).unwrap();
        assert_eq!(pending_count(&pi), 0);
    }

    #[test]
    fn unfinished_tools_are_held_up_to_a_limit() {
        let pi = integration();
        for n in 0..(MAX_PENDING_TOOLS + 50) {
            pi.ingest(&json!({"type": "tool_execution_start", "toolName": "rg", "args": {"n": n}}))
                .unwrap();
        }
        assert_eq!(pending_count(&pi), MAX_PENDING_TOOLS);
    }

    fn read_like(tool: &str, path: &str, call_id: &str) -> ToolCallObservation {
        ToolCallObservation {
            session_id: "s".into(),
            turn_id: "t".into(),
            call_id: call_id.into(),
            tool_name: tool.into(),
            arguments: json!({ "path": path }),
            repository_state: "head".into(),
            known_graph_entities: Vec::new(),
            indexed_paths: Vec::new(),
            output_text_for_estimate: Some(format!("{call_id}-body")),
            output_token_estimate: Some(3_000),
            output_bytes: None,
            duration_ms: 1,
            success: true,
        }
    }

    fn exact_repeat_accepted(pi: &PiIntegration, call_id: &str) -> Option<bool> {
        let state = pi.state.lock().unwrap();
        state
            .optimizations
            .iter()
            .find(|row| row.tool_call_id == call_id && row.alternative.rule == "exact-repeat")
            .map(|row| row.accepted)
    }

    #[test]
    fn a_repeat_that_could_not_be_replayed_is_not_logged_as_accepted() {
        let pi = create_pi_integration(PiOptions {
            mode: OptimizationMode::Optimize,
            ..PiOptions::default()
        });
        pi.record_tool_result(&read_like("read", "notes.txt", "r1"))
            .unwrap();
        pi.record_tool_result(&read_like("read", "notes.txt", "r2"))
            .unwrap();
        assert_eq!(exact_repeat_accepted(&pi, "r2"), Some(true));
        pi.record_tool_result(&read_like("edit", "notes.txt", "e1"))
            .unwrap();
        let again = read_like("read", "notes.txt", "r3");
        assert_eq!(pi.advise_tool_call(&again).unwrap().apply, None);
        pi.record_tool_result(&again).unwrap();
        assert_eq!(exact_repeat_accepted(&pi, "r3"), Some(false));
    }

    #[test]
    fn cached_tool_output_is_bounded_in_bytes() {
        let pi = create_pi_integration(PiOptions {
            mode: OptimizationMode::Optimize,
            ..PiOptions::default()
        });
        pi.set_repository_state("head").unwrap();
        let body = "x".repeat(1024 * 1024);
        for n in 0..(MAX_OUTPUT_CACHE_BYTES / body.len() + 8) {
            pi.record_tool_result(&ToolCallObservation {
                session_id: "s".into(),
                turn_id: "t".into(),
                call_id: format!("c{n}"),
                tool_name: "read".into(),
                arguments: json!({"path": format!("f{n}.txt")}),
                repository_state: "head".into(),
                known_graph_entities: Vec::new(),
                indexed_paths: Vec::new(),
                output_text_for_estimate: Some(body.clone()),
                output_token_estimate: Some(10),
                output_bytes: None,
                duration_ms: 1,
                success: true,
            })
            .unwrap();
        }
        let state = pi.state.lock().unwrap();
        let held: usize = state.output_cache.values().map(String::len).sum();
        assert!(held <= MAX_OUTPUT_CACHE_BYTES, "{held} bytes held");
        assert!(held > 0, "recent outputs stay cached");
    }

    #[test]
    fn different_entity_lists_never_share_a_context_cache_entry() {
        let pi = integration();
        let request = ContextRequest {
            session_id: "s".into(),
            turn_id: "t".into(),
            user_intent: Some("task".into()),
            repository_root: None,
            changed_files: Vec::new(),
            previous_tool_names: Vec::new(),
        };
        pi.set_graph_entities(vec!["Alpha,Beta".into()]).unwrap();
        assert!(!pi.get_session_context(&request).unwrap().cache_hit);
        pi.set_graph_entities(vec!["Alpha".into(), "Beta".into()])
            .unwrap();
        assert!(!pi.get_session_context(&request).unwrap().cache_hit);
        assert!(pi.get_session_context(&request).unwrap().cache_hit);
    }

    async fn run_session(db: &Path, times: &[u64]) {
        let pi = create_pi_integration(PiOptions {
            session_id: Some("s".into()),
            db_path: Some(db.to_path_buf()),
            ..PiOptions::default()
        });
        pi.start().unwrap();
        let (first, rest) = times.split_first().unwrap();
        pi.ingest(&json!({"type": "agent_start", "timestamp": first}))
            .unwrap();
        for time in rest {
            pi.ingest(&json!({"type": "turn_start", "timestamp": time}))
                .unwrap();
        }
        pi.stop().await.unwrap();
    }

    async fn session_times(db: &Path) -> (u64, u64) {
        let snapshot = store::load(db).await.unwrap();
        let row = snapshot.sessions.iter().find(|row| row.id == "s").unwrap();
        (row.created_at, row.updated_at)
    }

    #[tokio::test]
    async fn session_rows_carry_first_and_last_event_times() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ax.db");
        run_session(&db, &[1_000, 1_500, 2_000]).await;
        assert_eq!(session_times(&db).await, (1_000, 2_000));
        run_session(&db, &[5_000, 6_000]).await;
        assert_eq!(session_times(&db).await, (1_000, 6_000));
        // An older run flushed later never moves the times backwards.
        run_session(&db, &[3_000, 3_500]).await;
        assert_eq!(session_times(&db).await, (1_000, 6_000));
    }

    #[test]
    fn a_turn_end_drops_that_sessions_unfinished_tools_only() {
        let pi = integration();
        pi.ingest(&json!({"type": "tool_execution_start", "toolName": "rg", "toolCallId": "c1", "args": {}}))
            .unwrap();
        pi.ingest(&json!({"type": "tool_execution_start", "toolName": "rg", "toolCallId": "c2", "sessionId": "other", "args": {}}))
            .unwrap();
        pi.ingest(&json!({"type": "turn_end"})).unwrap();
        assert_eq!(pending_count(&pi), 1);
    }
}

#[cfg(test)]
mod read_guard_call_id_tests {
    use super::read_guard_call_id;

    #[test]
    fn read_guard_call_ids_are_distinct_within_one_millisecond() {
        let ids: std::collections::HashSet<String> =
            (0..1000).map(|_| read_guard_call_id()).collect();
        assert_eq!(ids.len(), 1000);
        assert!(ids.iter().all(|id| id.starts_with("read-guard-")));
    }
}
