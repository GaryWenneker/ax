//! Pi integration entry point.
//!
//! This is an observer and advisor. It does not run an agent loop, call a
//! model, or execute tools. Failures are logged and swallowed unless `strict`
//! is set. `prepareRequest` context replacement in Pi persists into the loop
//! transcript, so this crate returns context sections for a tool or resource
//! boundary and does not ask the host to rewrite `session.agent.state.messages`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use ax_usage::ModelPricing;
use serde_json::Value;

use crate::context::{cache_key_string, AxContextEngine, ContextCache};
use crate::economics::{
    advise_observation, context_token_cost, estimate_text, identity_hash, model_cost,
    RepeatDetector,
};
use crate::events::{self, arguments_for_hash, output_text_for_estimate};
use crate::report::{
    apply_display_currency, economics_from, format_economics, format_optimization,
    optimization_from, EconomicsReport, OptimizationReport,
};
use crate::store::{
    self, CacheRow, ModelRow, OptimizationRow, PersistedSnapshot, SessionRow, TurnRow,
};
use crate::types::{
    AxAgentEvent, AxSessionState, BudgetSnapshot, ContextCacheKey, ContextRequest, ContextResult,
    ContextSection, CostSnapshot, OptimizationMode, ToolCallAdvice, ToolCallObservation,
    ToolEconomicsRecord, UsageSnapshot, DEFAULT_CONTEXT_MAX_TOKENS, DEFAULT_OVERSIZED_TOKENS,
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
    entities: Vec<String>,
    indexed_paths: Vec<String>,
    repository_state: String,
    output_cache: HashMap<String, String>,
    pending_args: HashMap<String, Value>,
    started: bool,
    model_seq: u64,
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
                    graph_version: state.entities.join(","),
                    policy_version: "policy".into(),
                    skill_version: "skill".into(),
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
                if self.options.mode == OptimizationMode::Optimize {
                    if let Some(cached) = state.output_cache.get(&identity) {
                        advice.cached_output = Some(cached.clone());
                    }
                }
                Ok(advice)
            },
        )
    }

    pub fn record_tool_result(&self, observation: &ToolCallObservation) -> Result<(), PiError> {
        self.guard(|| {
            let mut state = self.state.lock().map_err(|err| err.to_string())?;
            record_tool(&mut state, &self.options, observation);
            Ok(())
        })
    }

    pub fn get_cost(&self, session_id: &str) -> Result<CostSnapshot, PiError> {
        self.guard_or(
            CostSnapshot {
                currency: self.options.currency.clone(),
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
                        currency: self.options.currency.clone(),
                        model_usd: None,
                        tool_context_usd: None,
                        confidence: "estimated".into(),
                    }))
            },
        )
    }

    pub fn economics(&self, session_id: Option<&str>) -> Result<EconomicsReport, PiError> {
        self.guard_or(
            empty_report(&self.options.currency),
            |report| report,
            || {
                let state = self.state.lock().map_err(|err| err.to_string())?;
                Ok(economics_from(
                    &snapshot_of(&state),
                    session_id,
                    &self.options.currency,
                    self.budget_of(&state),
                ))
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
                Ok(optimization_from(&snapshot_of(&state)))
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
            self.state.lock().map_err(|err| err.to_string())?.entities = entities;
            Ok(())
        })
    }

    pub fn set_indexed_paths(&self, paths: Vec<String>) -> Result<(), PiError> {
        self.guard(|| {
            self.state
                .lock()
                .map_err(|err| err.to_string())?
                .indexed_paths = paths;
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

    fn budget_of(&self, state: &State) -> Option<BudgetSnapshot> {
        let monthly = self.options.monthly_budget?;
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
        snapshot.currency = self.options.currency.clone();
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
            entities: Vec::new(),
            indexed_paths: Vec::new(),
            repository_state: "unknown".into(),
            output_cache: HashMap::new(),
            pending_args: HashMap::new(),
            started: false,
            model_seq: 0,
        }),
    }
}

fn apply_event(state: &mut State, options: &PiOptions, event: AxAgentEvent, raw: &Value) {
    match event {
        AxAgentEvent::SessionStarted(base) => {
            state
                .sessions
                .entry(base.session_id.clone())
                .or_insert_with(|| session_state(options, &base.session_id));
        }
        AxAgentEvent::SessionEnded(base) => {
            if let Some(session) = state.sessions.get_mut(&base.session_id) {
                session.cumulative_cost.currency = options.currency.clone();
            }
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
                .unwrap_or_else(|| format!("{session_id}-turn-{seq}"));
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
        }
        AxAgentEvent::ModelResponse(model) => {
            state.model_seq += 1;
            let cost = model_cost(
                model.provider.as_deref(),
                model.model.as_deref(),
                model.usage.input_tokens,
                model.usage.output_tokens,
                model.usage.cached_input_tokens,
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
                    turn.input_tokens += model.usage.input_tokens.unwrap_or(0);
                    turn.output_tokens += model.usage.output_tokens.unwrap_or(0);
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
                id: format!("{}-model-{}", model.base.session_id, state.model_seq),
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
            let call_id = start
                .base
                .operation_id
                .clone()
                .unwrap_or_else(|| format!("{}-{}", start.tool_name, state.tools.len()));
            state.pending_args.insert(call_id, arguments_for_hash(raw));
        }
        AxAgentEvent::ToolEnded(end) => {
            let call_id = end
                .base
                .operation_id
                .clone()
                .unwrap_or_else(|| format!("{}-{}", end.tool_name, state.tools.len()));
            let arguments = state
                .pending_args
                .remove(&call_id)
                .unwrap_or_else(|| arguments_for_hash(raw));
            let turn_id = end
                .base
                .turn_id
                .clone()
                .or_else(|| state.active_turn.get(&end.base.session_id).cloned())
                .unwrap_or_else(|| "turn".into());
            let tokens = end
                .output_token_estimate
                .or_else(|| output_text_for_estimate(raw).as_deref().map(estimate_text));
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
                    known_graph_entities: state.entities.clone(),
                    indexed_paths: state.indexed_paths.clone(),
                    output_text_for_estimate: output_text_for_estimate(raw),
                    output_token_estimate: tokens,
                    output_bytes: end.output_bytes,
                    duration_ms: end.duration_ms,
                    success: end.success,
                },
            );
        }
        AxAgentEvent::ModelRequest(_)
        | AxAgentEvent::ToolOutput(_)
        | AxAgentEvent::Compaction(_)
        | AxAgentEvent::SessionWrite(_) => {}
    }
}

fn record_tool(state: &mut State, options: &PiOptions, observation: &ToolCallObservation) {
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
    let (repeated, _) =
        state
            .repeats
            .observe(&identity, &observation.call_id, output_tokens, context_cost);
    let advice = advise_observation(
        observation,
        already,
        options.mode,
        options.oversized_token_threshold,
        true,
    );
    if options.mode == OptimizationMode::Optimize {
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
    } else if options.mode == OptimizationMode::Observe {
        let hidden = advise_observation(
            observation,
            already,
            OptimizationMode::Advise,
            options.oversized_token_threshold,
            true,
        );
        if let Some(alternative) = hidden.alternatives.into_iter().next() {
            state.optimizations.push(OptimizationRow {
                id: format!("opt-{}", observation.call_id),
                tool_call_id: observation.call_id.clone(),
                accepted: false,
                alternative,
            });
        }
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
        cumulative_cost: CostSnapshot {
            currency: options.currency.clone(),
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
            .map(|session| SessionRow {
                id: session.session_id.clone(),
                project_id: session.project_id.clone(),
                created_at: 0,
                updated_at: 0,
                repository_root: session.repository_root.clone(),
                active_branch: session.active_branch.clone(),
                cache_namespace: session.cache_namespace.clone(),
            })
            .collect(),
        turns: state.turns.clone(),
        tools: state.tools.clone(),
        models: state.models.clone(),
        optimizations: state.optimizations.clone(),
        cache_entries: state
            .cache
            .entries()
            .into_iter()
            .map(|entry| CacheRow {
                key: entry.key.clone(),
                project_id: entry.project_id.clone(),
                created_at: entry.created_at,
                estimated_tokens: entry.estimated_tokens,
                source_hashes: entry.source_hashes.clone(),
                git_revision: entry.git_revision.clone(),
                policy_version: entry.policy_version.clone(),
                skill_version: entry.skill_version.clone(),
                graph_version: entry.graph_version.clone(),
            })
            .collect(),
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
    let observation = ToolCallObservation {
        session_id: session_id.to_string(),
        turn_id: "read-guard".into(),
        call_id: format!("read-guard-{}", now_ms()),
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
