//! Context-export fidelity. Full evidence and the agent-ready manifest are built
//! only from the records the caller supplies. A closed turn inventory is required
//! before an export may say it is complete.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

/// Prompt character cap used by `record_hook_turn`. A stored prompt at this cap
/// is reported as truncated because the writer drops the rest.
pub const PROMPT_CAPTURE_CAP: usize = 8_000;
/// Outcome character cap used by `finish_hook_turn`.
pub const OUTCOME_CAPTURE_CAP: usize = 20_000;
const SCHEMA_VERSION: &str = "1.0";
const EXPORT_TYPE: &str = "ax-context-continuity";

/// A section that was read from a store, or that Ax has no store for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Section<T> {
    Present(T),
    NotCaptured,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentityInput {
    pub agent_provider: Section<String>,
    pub agent_chat_id: Section<String>,
    pub ax_session_id: String,
    pub project_id: Section<String>,
    pub linked_session_ids: Section<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnInput {
    pub ax_turn_id: String,
    pub agent_turn_id: Option<String>,
    pub sequence: i64,
    pub prompt: String,
    pub outcome: Option<String>,
    /// `None` means the file list could not be read, which is not an empty list.
    pub changed_files: Option<Vec<String>>,
    pub commits: Option<Vec<String>>,
    pub decision_ids: Vec<String>,
    pub tool_call_refs: Option<Vec<String>>,
    pub memory_refs: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionInput {
    pub id: String,
    pub text: String,
    pub turn_id: String,
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FailureInput {
    pub id: String,
    pub text: String,
    pub resolution: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefInput {
    pub id: String,
    pub version: Option<String>,
    pub resolvable: bool,
}

#[derive(Clone, Debug)]
pub struct ExportInput {
    pub inventory_closed: bool,
    pub identity: IdentityInput,
    pub objective: Section<String>,
    pub turns: Vec<TurnInput>,
    pub decisions: Section<Vec<DecisionInput>>,
    pub constraints: Section<Vec<String>>,
    pub open_tasks: Section<Vec<String>>,
    pub known_failures: Section<Vec<FailureInput>>,
    pub rejected_approaches: Section<Vec<DecisionInput>>,
    pub memories: Section<Vec<RefInput>>,
    pub graph_references: Section<Vec<RefInput>>,
    pub rules: Section<Vec<RefInput>>,
    pub skills: Section<Vec<RefInput>>,
    pub repository_commit: Section<Option<String>>,
    pub model: Section<Option<String>>,
    /// Dot-free paths such as `turns/{id}/prompt`. The raw value is omitted.
    pub redacted_fields: Vec<String>,
    /// A reference to a stored spec. Absence is not a completeness blocker.
    pub spec_pointer: Option<crate::spec_registry::SpecPointer>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoverageReport {
    pub turns: String,
    pub decisions: String,
    pub rules_and_skills: String,
    pub unresolved_references: usize,
    pub source_integrity: String,
    pub context_relevance: String,
    pub cross_chat_continuity: String,
    pub completeness: String,
    pub blockers: Vec<String>,
}

pub struct ExportBundle {
    pub full: Value,
    pub ready: Value,
    pub coverage: CoverageReport,
    pub errors: Vec<String>,
}

pub fn validate_context_export(input: &ExportInput) -> ExportBundle {
    let blockers = completeness_blockers(input);
    let completeness = if blockers.is_empty() {
        "complete"
    } else {
        "partial"
    };
    let full = build_full(input, completeness, &blockers);
    let ready = build_ready(input, &full);
    let errors = verify(input, &full, &ready);
    let coverage = coverage_report(input, &full, &blockers, errors.is_empty());
    ExportBundle {
        full,
        ready,
        coverage,
        errors,
    }
}

#[allow(dead_code)] // rendered coverage kept for the continuity report
pub fn coverage_markdown(report: &CoverageReport) -> String {
    let blockers = if report.blockers.is_empty() {
        "None".to_string()
    } else {
        report.blockers.join("; ")
    };
    format!(
        "Turns: {turns}\nDecisions: {decisions}\nRules & Skills: {rules}\nUnresolved references: {unresolved}\nSource integrity: {integrity}\nContext relevance: {relevance}\nCross-chat continuity: {continuity}\nCompleteness: {completeness}\nBlockers: {blockers}\n",
        turns = report.turns,
        decisions = report.decisions,
        rules = report.rules_and_skills,
        unresolved = report.unresolved_references,
        integrity = report.source_integrity,
        relevance = report.context_relevance,
        continuity = report.cross_chat_continuity,
        completeness = report.completeness,
    )
}

/// Read the turns Ax actually stored for one session.
/// Decisions, rules, and the provider chat id are `not_captured` because this
/// table does not store them. A failed query is an error, not an empty session.
pub async fn load_export_input(pool: &SqlitePool, session_id: &str) -> Result<ExportInput, String> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err("session id is required".into());
    }
    let project_id: Option<String> =
        sqlx::query_scalar("SELECT project_id FROM agent_sessions WHERE id = ?")
            .bind(session_id)
            .fetch_optional(pool)
            .await
            .map_err(|error| error.to_string())?;
    let rows: Vec<(String, i64, String, Option<String>, String, String)> = sqlx::query_as(
        "SELECT id, sequence, prompt, outcome, files, commits
         FROM agent_turns WHERE session_id = ? ORDER BY sequence, id",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;

    let turns = rows
        .into_iter()
        .map(
            |(id, sequence, prompt, outcome, files, commits)| TurnInput {
                ax_turn_id: id,
                agent_turn_id: None,
                sequence,
                prompt,
                outcome,
                changed_files: parse_string_list(&files),
                commits: parse_string_list(&commits),
                decision_ids: Vec::new(),
                tool_call_refs: None,
                memory_refs: None,
            },
        )
        .collect();

    Ok(ExportInput {
        inventory_closed: true,
        identity: IdentityInput {
            agent_provider: Section::NotCaptured,
            agent_chat_id: Section::NotCaptured,
            ax_session_id: session_id.to_string(),
            project_id: non_empty(project_id),
            linked_session_ids: Section::NotCaptured,
        },
        objective: Section::NotCaptured,
        turns,
        decisions: Section::NotCaptured,
        constraints: Section::NotCaptured,
        open_tasks: Section::NotCaptured,
        known_failures: Section::NotCaptured,
        rejected_approaches: Section::NotCaptured,
        memories: Section::NotCaptured,
        graph_references: Section::NotCaptured,
        rules: Section::NotCaptured,
        skills: Section::NotCaptured,
        repository_commit: Section::NotCaptured,
        model: Section::NotCaptured,
        redacted_fields: Vec::new(),
        spec_pointer: crate::spec_registry::spec_pointer_for(pool, session_id)
            .await
            .ok()
            .flatten(),
    })
}

fn non_empty(value: Option<String>) -> Section<String> {
    match value {
        Some(text) if !text.trim().is_empty() => Section::Present(text),
        _ => Section::NotCaptured,
    }
}

fn parse_string_list(raw: &str) -> Option<Vec<String>> {
    let value: Value = serde_json::from_str(raw).ok()?;
    let items = value.as_array()?;
    items
        .iter()
        .map(|item| item.as_str().map(str::to_string))
        .collect()
}

fn completeness_blockers(input: &ExportInput) -> Vec<String> {
    let mut blockers = Vec::new();
    if !input.inventory_closed {
        blockers.push("turn inventory is not closed".into());
    }
    if input.identity.ax_session_id.trim().is_empty() {
        blockers.push("ax session id is empty".into());
    }
    require_text(
        &input.identity.agent_provider,
        "agent provider",
        &mut blockers,
    );
    require_text(
        &input.identity.agent_chat_id,
        "agent chat id",
        &mut blockers,
    );
    require_text(&input.identity.project_id, "project id", &mut blockers);
    require_section(
        &input.identity.linked_session_ids,
        "linked sessions",
        &mut blockers,
    );
    require_text(&input.objective, "objective", &mut blockers);
    require_section(&input.decisions, "decisions", &mut blockers);
    require_section(&input.constraints, "constraints", &mut blockers);
    require_section(&input.open_tasks, "open tasks", &mut blockers);
    require_section(&input.known_failures, "known failures", &mut blockers);
    require_section(
        &input.rejected_approaches,
        "rejected approaches",
        &mut blockers,
    );
    require_section(&input.memories, "memories", &mut blockers);
    require_section(&input.graph_references, "graph references", &mut blockers);
    require_section(&input.rules, "rules", &mut blockers);
    require_section(&input.skills, "skills", &mut blockers);
    require_section(&input.repository_commit, "repository commit", &mut blockers);
    require_section(&input.model, "model", &mut blockers);

    let mut seen = std::collections::BTreeSet::new();
    let mut sequences: Vec<i64> = Vec::new();
    for turn in &input.turns {
        if turn.ax_turn_id.trim().is_empty() {
            blockers.push("a turn id is empty".into());
        } else if !seen.insert(turn.ax_turn_id.clone()) {
            blockers.push(format!("duplicate turn id {}", turn.ax_turn_id));
        }
        sequences.push(turn.sequence);
        if prompt_truncated(turn) {
            blockers.push(format!("turn {} prompt is truncated", turn.ax_turn_id));
        }
        if outcome_truncated(turn) {
            blockers.push(format!("turn {} outcome is truncated", turn.ax_turn_id));
        }
        if turn.changed_files.is_none() {
            blockers.push(format!("turn {} files are unavailable", turn.ax_turn_id));
        }
        if turn.commits.is_none() {
            blockers.push(format!("turn {} commits are unavailable", turn.ax_turn_id));
        }
        if turn.tool_call_refs.is_none() {
            blockers.push(format!(
                "turn {} tool results are not captured",
                turn.ax_turn_id
            ));
        }
    }
    if !sequences.is_empty() {
        sequences.sort_unstable();
        if sequences[0] != 1 || sequences.windows(2).any(|pair| pair[1] != pair[0] + 1) {
            blockers.push("turn sequence has a gap, so a missing turn cannot be ruled out".into());
        }
    }
    for reference in unresolved_refs(input) {
        blockers.push(format!("unresolved reference {reference}"));
    }
    if !input.redacted_fields.is_empty() {
        blockers.push("one or more fields are redacted".into());
    }
    blockers
}

fn require_text(section: &Section<String>, label: &str, blockers: &mut Vec<String>) {
    match section {
        Section::NotCaptured => blockers.push(format!("{label} is not captured")),
        Section::Present(text) if text.trim().is_empty() => {
            blockers.push(format!("{label} is empty"))
        }
        Section::Present(_) => {}
    }
}

fn require_section<T>(section: &Section<T>, label: &str, blockers: &mut Vec<String>) {
    if matches!(section, Section::NotCaptured) {
        blockers.push(format!("{label} is not captured"));
    }
}

fn unresolved_refs(input: &ExportInput) -> Vec<String> {
    let mut ids = Vec::new();
    for section in [
        &input.memories,
        &input.graph_references,
        &input.rules,
        &input.skills,
    ] {
        if let Section::Present(items) = section {
            ids.extend(
                items
                    .iter()
                    .filter(|item| !item.resolvable)
                    .map(|item| item.id.clone()),
            );
        }
    }
    ids
}

fn prompt_truncated(turn: &TurnInput) -> bool {
    turn.prompt.chars().count() >= PROMPT_CAPTURE_CAP
}

fn outcome_truncated(turn: &TurnInput) -> bool {
    turn.outcome
        .as_ref()
        .map(|text| text.chars().count() >= OUTCOME_CAPTURE_CAP)
        .unwrap_or(false)
}

fn build_full(input: &ExportInput, completeness: &str, blockers: &[String]) -> Value {
    let turns: Vec<Value> = input
        .turns
        .iter()
        .map(|turn| full_turn(input, turn))
        .collect();
    let turn_hash = sha256_hex(turn_fingerprints(input).join("\n").as_bytes());
    json!({
        "schemaVersion": SCHEMA_VERSION,
        "exportType": EXPORT_TYPE,
        "representation": "full-evidence",
        "identity": identity_value(&input.identity, input.turns.last().map(|turn| turn.ax_turn_id.as_str())),
        "session": {
            "objective": section_text(&input.objective),
            "status": "active",
            "totalTurns": input.turns.len(),
            "capturedTurns": input.turns.len(),
            "inventoryClosed": input.inventory_closed,
            "firstTurn": input.turns.iter().map(|turn| turn.sequence).min(),
            "lastTurn": input.turns.iter().map(|turn| turn.sequence).max(),
        },
        "workingContext": {
            "currentGoal": section_text(&input.objective),
            "decisions": decision_section(&input.decisions),
            "constraints": string_section(&input.constraints),
            "openTasks": string_section(&input.open_tasks),
            "knownFailures": failure_section(&input.known_failures),
            "rejectedApproaches": decision_section(&input.rejected_approaches),
        },
        "turns": turns,
        "knowledge": {
            "memories": ref_section(&input.memories),
            "graphReferences": ref_section(&input.graph_references),
            "rules": ref_section(&input.rules),
            "skills": ref_section(&input.skills),
        },
        "execution": {
            "repositoryCommit": option_section(&input.repository_commit),
            "model": option_section(&input.model),
            "configuration": {},
            "artifacts": [],
        },
        "provenance": {
            "snapshotId": format!("snap-{}", &turn_hash[..12]),
            "sourceHashes": { "turns": turn_hash },
            "missingFields": missing_fields(input),
            "redactedFields": input.redacted_fields,
            "truncatedFields": truncated_fields(input),
            "completeness": completeness,
            "completenessBlockers": blockers,
        },
        "spec": spec_value(input),
    })
}

fn spec_value(input: &ExportInput) -> Value {
    match &input.spec_pointer {
        Some(spec) => json!({
            "availability": "referenced",
            "id": spec.id,
            "version": spec.version,
            "hash": spec.content_hash,
            "pointer": format!("ax spec show {}@{}", spec.id, spec.version),
        }),
        None => json!({ "availability": "not_captured" }),
    }
}

fn build_ready(input: &ExportInput, full: &Value) -> Value {
    let last_id = input.turns.last().map(|turn| turn.ax_turn_id.as_str());
    let turns: Vec<Value> = input
        .turns
        .iter()
        .map(|turn| {
            let include_prompt = last_id == Some(turn.ax_turn_id.as_str())
                && !prompt_truncated(turn)
                && !is_redacted(input, &format!("turns/{}/prompt", turn.ax_turn_id))
                && turn.prompt.chars().count() < 2_000;
            json!({
                "axTurnId": turn.ax_turn_id,
                "sequence": turn.sequence,
                "sha256": turn_hash(turn),
                "prompt": if include_prompt {
                    json!({"state": "present", "value": turn.prompt})
                } else {
                    json!({"state": "referenced", "sha256": turn_hash(turn)})
                },
                "decisionIds": turn.decision_ids,
            })
        })
        .collect();
    let mut ready = full.clone();
    ready["representation"] = json!("agent-ready");
    ready["turns"] = json!(turns);
    ready["knowledge"]["memories"] = ref_ids_only(&input.memories);
    ready["knowledge"]["graphReferences"] = ref_ids_only(&input.graph_references);
    ready
}

fn full_turn(input: &ExportInput, turn: &TurnInput) -> Value {
    json!({
        "axTurnId": turn.ax_turn_id,
        "agentTurnId": match &turn.agent_turn_id {
            Some(id) => json!({"state": "present", "value": id}),
            None => json!({"state": "not_captured"}),
        },
        "sequence": turn.sequence,
        "sha256": turn_hash(turn),
        "prompt": text_state(input, &format!("turns/{}/prompt", turn.ax_turn_id), &turn.prompt, prompt_truncated(turn)),
        "outcome": match &turn.outcome {
            Some(text) => text_state(input, &format!("turns/{}/outcome", turn.ax_turn_id), text, outcome_truncated(turn)),
            None => json!({"state": "not_captured"}),
        },
        "changedFiles": list_state(turn.changed_files.as_ref()),
        "commits": list_state(turn.commits.as_ref()),
        "decisions": turn.decision_ids,
        "toolCallRefs": list_state(turn.tool_call_refs.as_ref()),
        "memoryRefs": list_state(turn.memory_refs.as_ref()),
    })
}

fn text_state(input: &ExportInput, path: &str, value: &str, truncated: bool) -> Value {
    if is_redacted(input, path) {
        json!({"state": "redacted"})
    } else if truncated {
        json!({"state": "truncated", "value": value})
    } else {
        json!({"state": "present", "value": value})
    }
}

fn list_state(items: Option<&Vec<String>>) -> Value {
    match items {
        Some(items) => json!({"state": "present", "items": items}),
        None => json!({"state": "unavailable"}),
    }
}

fn is_redacted(input: &ExportInput, path: &str) -> bool {
    input.redacted_fields.iter().any(|field| field == path)
}

fn section_text(section: &Section<String>) -> Value {
    match section {
        Section::Present(text) => json!({"state": "present", "value": text}),
        Section::NotCaptured => json!({"state": "not_captured"}),
    }
}

fn string_section(section: &Section<Vec<String>>) -> Value {
    match section {
        Section::Present(items) => json!({"state": "present", "items": items}),
        Section::NotCaptured => json!({"state": "not_captured", "items": []}),
    }
}

fn decision_section(section: &Section<Vec<DecisionInput>>) -> Value {
    match section {
        Section::Present(items) => json!({
            "state": "present",
            "items": items.iter().map(decision_value).collect::<Vec<_>>(),
        }),
        Section::NotCaptured => json!({"state": "not_captured", "items": []}),
    }
}

fn decision_value(decision: &DecisionInput) -> Value {
    json!({
        "id": decision.id,
        "text": decision.text,
        "turnId": decision.turn_id,
        "status": decision.status,
    })
}

fn failure_section(section: &Section<Vec<FailureInput>>) -> Value {
    match section {
        Section::Present(items) => json!({
            "state": "present",
            "items": items.iter().map(|failure| json!({
                "id": failure.id,
                "text": failure.text,
                "resolution": failure.resolution,
            })).collect::<Vec<_>>(),
        }),
        Section::NotCaptured => json!({"state": "not_captured", "items": []}),
    }
}

fn ref_section(section: &Section<Vec<RefInput>>) -> Value {
    match section {
        Section::Present(items) => json!({
            "state": "present",
            "items": items.iter().map(ref_value).collect::<Vec<_>>(),
        }),
        Section::NotCaptured => json!({"state": "not_captured", "items": []}),
    }
}

fn ref_ids_only(section: &Section<Vec<RefInput>>) -> Value {
    match section {
        Section::Present(items) => json!({
            "state": "referenced",
            "items": items.iter().map(|item| json!({
                "id": item.id,
                "state": if item.resolvable { "referenced" } else { "unavailable" },
                "sha256": sha256_hex(item.id.as_bytes()),
            })).collect::<Vec<_>>(),
        }),
        Section::NotCaptured => json!({"state": "not_captured", "items": []}),
    }
}

fn ref_value(item: &RefInput) -> Value {
    json!({
        "id": item.id,
        "version": item.version,
        "state": if item.resolvable { "present" } else { "unavailable" },
    })
}

fn option_section(section: &Section<Option<String>>) -> Value {
    match section {
        Section::NotCaptured => json!({"state": "not_captured"}),
        Section::Present(None) => json!({"state": "present", "value": null}),
        Section::Present(Some(text)) => json!({"state": "present", "value": text}),
    }
}

fn identity_value(identity: &IdentityInput, source_turn: Option<&str>) -> Value {
    json!({
        "agentProvider": section_text(&identity.agent_provider),
        "agentChatId": section_text(&identity.agent_chat_id),
        "axSessionId": identity.ax_session_id,
        "sourceTurnId": match source_turn {
            Some(id) => json!({"state": "present", "value": id}),
            None => json!({"state": "not_captured"}),
        },
        "projectId": section_text(&identity.project_id),
        "linkedSessionIds": string_section(&identity.linked_session_ids),
    })
}

fn missing_fields(input: &ExportInput) -> Vec<String> {
    let mut fields = Vec::new();
    if matches!(input.identity.agent_provider, Section::NotCaptured) {
        fields.push("identity.agentProvider".into());
    }
    if matches!(input.identity.agent_chat_id, Section::NotCaptured) {
        fields.push("identity.agentChatId".into());
    }
    if matches!(input.identity.project_id, Section::NotCaptured) {
        fields.push("identity.projectId".into());
    }
    if matches!(input.identity.linked_session_ids, Section::NotCaptured) {
        fields.push("identity.linkedSessionIds".into());
    }
    if matches!(input.objective, Section::NotCaptured) {
        fields.push("session.objective".into());
    }
    if matches!(input.decisions, Section::NotCaptured) {
        fields.push("workingContext.decisions".into());
    }
    if matches!(input.constraints, Section::NotCaptured) {
        fields.push("workingContext.constraints".into());
    }
    if matches!(input.open_tasks, Section::NotCaptured) {
        fields.push("workingContext.openTasks".into());
    }
    if matches!(input.known_failures, Section::NotCaptured) {
        fields.push("workingContext.knownFailures".into());
    }
    if matches!(input.rejected_approaches, Section::NotCaptured) {
        fields.push("workingContext.rejectedApproaches".into());
    }
    if matches!(input.memories, Section::NotCaptured) {
        fields.push("knowledge.memories".into());
    }
    if matches!(input.graph_references, Section::NotCaptured) {
        fields.push("knowledge.graphReferences".into());
    }
    if matches!(input.rules, Section::NotCaptured) {
        fields.push("knowledge.rules".into());
    }
    if matches!(input.skills, Section::NotCaptured) {
        fields.push("knowledge.skills".into());
    }
    if matches!(input.repository_commit, Section::NotCaptured) {
        fields.push("execution.repositoryCommit".into());
    }
    if matches!(input.model, Section::NotCaptured) {
        fields.push("execution.model".into());
    }
    fields
}

fn truncated_fields(input: &ExportInput) -> Vec<String> {
    let mut fields = Vec::new();
    for turn in &input.turns {
        if prompt_truncated(turn) {
            fields.push(format!("turns/{}/prompt", turn.ax_turn_id));
        }
        if outcome_truncated(turn) {
            fields.push(format!("turns/{}/outcome", turn.ax_turn_id));
        }
    }
    fields
}

fn turn_fingerprints(input: &ExportInput) -> Vec<String> {
    let mut turns = input.turns.clone();
    turns.sort_by(|left, right| {
        left.sequence
            .cmp(&right.sequence)
            .then(left.ax_turn_id.cmp(&right.ax_turn_id))
    });
    turns.iter().map(turn_hash).collect()
}

fn turn_hash(turn: &TurnInput) -> String {
    let files = match &turn.changed_files {
        Some(items) => items.join("\n"),
        None => "\u{0}unavailable".into(),
    };
    let commits = match &turn.commits {
        Some(items) => items.join("\n"),
        None => "\u{0}unavailable".into(),
    };
    let canonical = format!(
        "{id}\n{sequence}\n{prompt}\n{outcome}\n{files}\n{commits}",
        id = turn.ax_turn_id,
        sequence = turn.sequence,
        prompt = turn.prompt,
        outcome = turn.outcome.as_deref().unwrap_or(""),
        files = files,
        commits = commits,
    );
    sha256_hex(canonical.as_bytes())
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn verify(input: &ExportInput, full: &Value, ready: &Value) -> Vec<String> {
    let mut errors = Vec::new();
    expect_str(full, "schemaVersion", SCHEMA_VERSION, &mut errors);
    expect_str(full, "exportType", EXPORT_TYPE, &mut errors);
    expect_str(full, "representation", "full-evidence", &mut errors);
    expect_str(ready, "representation", "agent-ready", &mut errors);
    let blockers = completeness_blockers(input);
    let expected = if blockers.is_empty() {
        "complete"
    } else {
        "partial"
    };
    let actual = full["provenance"]["completeness"].as_str().unwrap_or("");
    if actual != expected {
        errors.push(format!("completeness is {actual}, expected {expected}"));
    }
    if full["provenance"]["completenessBlockers"] != json!(blockers) {
        errors.push("completeness blockers do not match the source".into());
    }
    let exported = full["turns"].as_array().cloned().unwrap_or_default();
    if exported.len() != input.turns.len() {
        errors.push(format!(
            "exported {} turns, source has {}",
            exported.len(),
            input.turns.len()
        ));
    }
    for turn in &input.turns {
        let Some(exported_turn) = exported
            .iter()
            .find(|item| item["axTurnId"] == turn.ax_turn_id)
        else {
            errors.push(format!("missing turn {}", turn.ax_turn_id));
            continue;
        };
        if exported_turn["sequence"] != json!(turn.sequence) {
            errors.push(format!("turn {} sequence was altered", turn.ax_turn_id));
        }
        if exported_turn["sha256"] != json!(turn_hash(turn)) {
            errors.push(format!("turn {} hash was altered", turn.ax_turn_id));
        }
        check_text_field(
            input,
            exported_turn,
            "prompt",
            &turn.prompt,
            &format!("turns/{}/prompt", turn.ax_turn_id),
            prompt_truncated(turn),
            &mut errors,
        );
        if let Some(outcome) = &turn.outcome {
            check_text_field(
                input,
                exported_turn,
                "outcome",
                outcome,
                &format!("turns/{}/outcome", turn.ax_turn_id),
                outcome_truncated(turn),
                &mut errors,
            );
        }
    }
    check_decisions(
        &input.decisions,
        &full["workingContext"]["decisions"],
        "decisions",
        &mut errors,
    );
    check_decisions(
        &input.rejected_approaches,
        &full["workingContext"]["rejectedApproaches"],
        "rejected approaches",
        &mut errors,
    );
    let serialized = full.to_string() + &ready.to_string();
    for turn in &input.turns {
        let path = format!("turns/{}/prompt", turn.ax_turn_id);
        if is_redacted(input, &path) && serialized.contains(&turn.prompt) && !turn.prompt.is_empty()
        {
            errors.push(format!("{path} still contains the redacted value"));
        }
    }
    let ready_turns = ready["turns"].as_array().cloned().unwrap_or_default();
    for turn in &input.turns {
        let Some(ready_turn) = ready_turns
            .iter()
            .find(|item| item["axTurnId"] == turn.ax_turn_id)
        else {
            errors.push(format!(
                "agent-ready context dropped turn {}",
                turn.ax_turn_id
            ));
            continue;
        };
        if let Some(value) = ready_turn["prompt"]["value"].as_str() {
            if value != turn.prompt {
                errors.push(format!(
                    "agent-ready prompt for {} was rewritten",
                    turn.ax_turn_id
                ));
            }
        }
        let is_last = input.turns.last().map(|item| item.ax_turn_id.as_str())
            == Some(turn.ax_turn_id.as_str());
        if !is_last && ready_turn["prompt"]["state"] != "referenced" {
            errors.push(format!(
                "turn {} body was copied into the agent-ready context",
                turn.ax_turn_id
            ));
        }
    }
    if let Section::Present(decisions) = &input.decisions {
        let ready_text = ready["workingContext"]["decisions"].to_string();
        for decision in decisions {
            if !ready_text.contains(&decision.text) {
                errors.push(format!(
                    "agent-ready context dropped decision {}",
                    decision.id
                ));
            }
        }
    }
    errors
}

fn check_text_field(
    input: &ExportInput,
    turn: &Value,
    name: &str,
    raw: &str,
    path: &str,
    truncated: bool,
    errors: &mut Vec<String>,
) {
    let field = &turn[name];
    if is_redacted(input, path) {
        if field["state"] != "redacted" || field.get("value").is_some() {
            errors.push(format!("{path} must be redacted without a value"));
        }
        return;
    }
    let expected_state = if truncated { "truncated" } else { "present" };
    if field["state"] != expected_state || field["value"] != json!(raw) {
        errors.push(format!("{path} does not match the stored value"));
    }
}

fn check_decisions(
    section: &Section<Vec<DecisionInput>>,
    exported: &Value,
    label: &str,
    errors: &mut Vec<String>,
) {
    match section {
        Section::NotCaptured => {
            if exported["state"] != "not_captured" {
                errors.push(format!("{label} were marked captured without a source"));
            }
        }
        Section::Present(items) => {
            if exported["state"] != "present" {
                errors.push(format!("{label} were dropped"));
                return;
            }
            let exported_items = exported["items"].as_array().cloned().unwrap_or_default();
            if exported_items.len() != items.len() {
                errors.push(format!("{label} count does not match the source"));
            }
            for decision in items {
                let found = exported_items.iter().any(|item| {
                    item["id"] == decision.id
                        && item["text"] == decision.text
                        && item["turnId"] == decision.turn_id
                });
                if !found {
                    errors.push(format!(
                        "{label} item {} does not match the source",
                        decision.id
                    ));
                }
            }
        }
    }
}

fn expect_str(value: &Value, key: &str, expected: &str, errors: &mut Vec<String>) {
    if value[key].as_str() != Some(expected) {
        errors.push(format!("{key} is not {expected}"));
    }
}

fn coverage_report(
    input: &ExportInput,
    full: &Value,
    blockers: &[String],
    verified: bool,
) -> CoverageReport {
    let exported = full["turns"].as_array().map(Vec::len).unwrap_or(0);
    CoverageReport {
        turns: if input.inventory_closed {
            format!("{exported} / {}", input.turns.len())
        } else {
            "unverified".into()
        },
        decisions: match &input.decisions {
            Section::Present(items) => format!("{} / {}", items.len(), items.len()),
            Section::NotCaptured => "not captured".into(),
        },
        rules_and_skills: rules_and_skills(input),
        unresolved_references: unresolved_refs(input).len(),
        source_integrity: if verified { "Verified" } else { "Failed" }.into(),
        context_relevance: "Not yet tested".into(),
        cross_chat_continuity: "Pending".into(),
        completeness: full["provenance"]["completeness"]
            .as_str()
            .unwrap_or("partial")
            .to_string(),
        blockers: blockers.to_vec(),
    }
}

fn rules_and_skills(input: &ExportInput) -> String {
    match (&input.rules, &input.skills) {
        (Section::Present(rules), Section::Present(skills)) => {
            let count = rules.len() + skills.len();
            format!("{count} / {count}")
        }
        _ => "not captured".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> Section<String> {
        Section::Present(value.to_string())
    }

    fn refs(ids: &[&str]) -> Section<Vec<RefInput>> {
        Section::Present(
            ids.iter()
                .map(|id| RefInput {
                    id: (*id).to_string(),
                    version: Some("1".into()),
                    resolvable: true,
                })
                .collect(),
        )
    }

    fn turn(sequence: i64, prompt: &str) -> TurnInput {
        TurnInput {
            ax_turn_id: format!("turn-{sequence}"),
            agent_turn_id: None,
            sequence,
            prompt: prompt.to_string(),
            outcome: Some("completed".into()),
            changed_files: Some(vec![format!("file-{sequence}.rs")]),
            commits: Some(vec![]),
            decision_ids: Vec::new(),
            tool_call_refs: Some(vec![]),
            memory_refs: Some(vec![]),
        }
    }

    fn closed_input(turns: Vec<TurnInput>) -> ExportInput {
        ExportInput {
            inventory_closed: true,
            identity: IdentityInput {
                agent_provider: text("cursor"),
                agent_chat_id: text("chat-1"),
                ax_session_id: "axs-1".into(),
                project_id: text("ax"),
                linked_session_ids: Section::Present(vec!["chat-1".into(), "chat-2".into()]),
            },
            objective: text("Continue the recorded task"),
            turns,
            decisions: Section::Present(vec![DecisionInput {
                id: "d-early".into(),
                text: "keep the first ranking".into(),
                turn_id: "turn-1".into(),
                status: "active".into(),
            }]),
            constraints: Section::Present(vec!["do not flip neural weights".into()]),
            open_tasks: Section::Present(vec!["verify the export".into()]),
            known_failures: Section::Present(vec![FailureInput {
                id: "f-1".into(),
                text: "the first index build failed".into(),
                resolution: Some("rebuilt the index".into()),
            }]),
            rejected_approaches: Section::Present(vec![DecisionInput {
                id: "d-rejected".into(),
                text: "rejected neural ranker v1".into(),
                turn_id: "turn-1".into(),
                status: "rejected".into(),
            }]),
            memories: refs(&["mem-1"]),
            graph_references: refs(&["sym-1"]),
            rules: refs(&["english-only"]),
            skills: refs(&["old-coder"]),
            repository_commit: Section::Present(Some("abc123".into())),
            model: Section::Present(Some("test-model".into())),
            redacted_fields: Vec::new(),
            spec_pointer: None,
        }
    }

    #[test]
    fn a_closed_record_with_every_section_is_complete() {
        let mut turns = vec![turn(1, "first"), turn(2, "second"), turn(3, "third")];
        turns[0].decision_ids = vec!["d-early".into(), "d-rejected".into()];
        let bundle = validate_context_export(&closed_input(turns));
        assert!(bundle.errors.is_empty(), "{:?}", bundle.errors);
        assert_eq!(bundle.full["provenance"]["completeness"], "complete");
        assert_eq!(bundle.coverage.turns, "3 / 3");
        assert_eq!(bundle.coverage.decisions, "1 / 1");
        assert_eq!(bundle.coverage.rules_and_skills, "2 / 2");
        assert_eq!(bundle.coverage.source_integrity, "Verified");
        assert_eq!(bundle.coverage.cross_chat_continuity, "Pending");
        let report = coverage_markdown(&bundle.coverage);
        assert!(report.contains("Turns: 3 / 3"));
        assert!(report.contains("Completeness: complete"));
        assert!(report.contains("Cross-chat continuity: Pending"));
        assert_eq!(bundle.full["spec"]["availability"], "not_captured");
        assert_eq!(bundle.ready["spec"]["availability"], "not_captured");
    }

    #[test]
    fn a_spec_pointer_is_a_reference_and_not_a_copied_body() {
        let mut input = closed_input(vec![turn(1, "only")]);
        input.spec_pointer = Some(crate::spec_registry::SpecPointer {
            id: "billing".into(),
            version: 2,
            content_hash: "abc".into(),
        });
        let bundle = validate_context_export(&input);
        assert!(bundle.errors.is_empty(), "{:?}", bundle.errors);
        assert_eq!(bundle.full["provenance"]["completeness"], "complete");
        assert_eq!(bundle.full["spec"]["pointer"], "ax spec show billing@2");
        assert_eq!(bundle.ready["spec"], bundle.full["spec"]);
        assert!(bundle.full["spec"].get("body").is_none());
        assert!(!bundle.full.to_string().contains("Keep the first price."));
    }

    #[test]
    fn an_open_inventory_cannot_be_complete() {
        let mut input = closed_input(vec![turn(1, "only")]);
        input.inventory_closed = false;
        let bundle = validate_context_export(&input);
        assert!(bundle.errors.is_empty(), "{:?}", bundle.errors);
        assert_eq!(bundle.full["provenance"]["completeness"], "partial");
        assert_eq!(bundle.coverage.turns, "unverified");
        assert!(bundle
            .coverage
            .blockers
            .iter()
            .any(|item| item.contains("not closed")));
    }

    #[test]
    fn a_sequence_gap_blocks_complete() {
        let bundle =
            validate_context_export(&closed_input(vec![turn(1, "first"), turn(3, "third")]));
        assert!(bundle.errors.is_empty(), "{:?}", bundle.errors);
        assert_eq!(bundle.full["provenance"]["completeness"], "partial");
        assert!(bundle
            .coverage
            .blockers
            .iter()
            .any(|item| item.contains("gap")));
    }

    #[test]
    fn a_claimed_complete_export_fails_when_the_inventory_is_open() {
        let mut input = closed_input(vec![turn(1, "only")]);
        input.inventory_closed = false;
        let bundle = validate_context_export(&input);
        let mut forged = bundle.full.clone();
        forged["provenance"]["completeness"] = json!("complete");
        let errors = verify(&input, &forged, &bundle.ready);
        assert!(errors.iter().any(|error| error.contains("completeness")));
    }

    #[test]
    fn two_hundred_turns_keep_the_early_decision_and_reference_the_bodies() {
        let turns: Vec<TurnInput> = (1..=200)
            .map(|sequence| turn(sequence, &format!("prompt {sequence}")))
            .collect();
        let mut input = closed_input(turns);
        input.turns[0].decision_ids = vec!["d-early".into(), "d-rejected".into()];
        let bundle = validate_context_export(&input);
        assert!(bundle.errors.is_empty(), "{:?}", bundle.errors);
        assert_eq!(bundle.coverage.turns, "200 / 200");
        let ready = bundle.ready.to_string();
        assert!(ready.contains("keep the first ranking"));
        assert!(ready.contains("rejected neural ranker v1"));
        assert!(!ready.contains("prompt 1"));
        assert!(ready.contains("prompt 200"));
        assert_eq!(bundle.ready["turns"][0]["prompt"]["state"], "referenced");
        assert_eq!(bundle.full["provenance"]["completeness"], "complete");
    }

    #[test]
    fn contradictory_decisions_are_both_kept() {
        let mut input = closed_input(vec![turn(1, "choose")]);
        input.decisions = Section::Present(vec![
            DecisionInput {
                id: "d-a".into(),
                text: "use ranker A".into(),
                turn_id: "turn-1".into(),
                status: "active".into(),
            },
            DecisionInput {
                id: "d-b".into(),
                text: "use ranker B".into(),
                turn_id: "turn-1".into(),
                status: "active".into(),
            },
        ]);
        let bundle = validate_context_export(&input);
        assert!(bundle.errors.is_empty(), "{:?}", bundle.errors);
        let text = bundle.full["workingContext"]["decisions"].to_string();
        assert!(text.contains("use ranker A"));
        assert!(text.contains("use ranker B"));
    }

    #[test]
    fn a_rewritten_prompt_fails_verification() {
        let input = closed_input(vec![turn(1, "original prompt")]);
        let bundle = validate_context_export(&input);
        let mut forged = bundle.full.clone();
        forged["turns"][0]["prompt"]["value"] = json!("a shorter summary");
        let errors = verify(&input, &forged, &bundle.ready);
        assert!(errors.iter().any(|error| error.contains("does not match")));
    }

    #[test]
    fn redaction_removes_the_secret_and_stays_partial() {
        let mut input = closed_input(vec![turn(1, "token sk-secret-value")]);
        input.redacted_fields = vec!["turns/turn-1/prompt".into()];
        let bundle = validate_context_export(&input);
        assert!(bundle.errors.is_empty(), "{:?}", bundle.errors);
        let serialized = bundle.full.to_string() + &bundle.ready.to_string();
        assert!(!serialized.contains("sk-secret-value"));
        assert_eq!(bundle.full["turns"][0]["prompt"]["state"], "redacted");
        assert_eq!(bundle.full["provenance"]["completeness"], "partial");
    }

    #[test]
    fn an_unresolved_memory_is_unavailable() {
        let mut input = closed_input(vec![turn(1, "remember")]);
        input.memories = Section::Present(vec![RefInput {
            id: "mem-missing".into(),
            version: None,
            resolvable: false,
        }]);
        let bundle = validate_context_export(&input);
        assert!(bundle.errors.is_empty(), "{:?}", bundle.errors);
        assert_eq!(
            bundle.full["knowledge"]["memories"]["items"][0]["state"],
            "unavailable"
        );
        assert_eq!(bundle.coverage.unresolved_references, 1);
        assert_eq!(bundle.full["provenance"]["completeness"], "partial");
    }

    #[test]
    fn missing_tool_results_are_not_invented() {
        let mut turns = vec![turn(1, "call")];
        turns[0].tool_call_refs = None;
        let bundle = validate_context_export(&closed_input(turns));
        assert_eq!(
            bundle.full["turns"][0]["toolCallRefs"]["state"],
            "unavailable"
        );
        assert_eq!(bundle.full["provenance"]["completeness"], "partial");
        assert!(bundle.errors.is_empty(), "{:?}", bundle.errors);
    }

    #[test]
    fn a_captured_prompt_at_the_writer_cap_is_truncated() {
        let bundle = validate_context_export(&closed_input(vec![turn(
            1,
            &"y".repeat(PROMPT_CAPTURE_CAP),
        )]));
        assert_eq!(bundle.full["turns"][0]["prompt"]["state"], "truncated");
        assert_eq!(bundle.full["provenance"]["completeness"], "partial");
        assert!(bundle.full["provenance"]["truncatedFields"]
            .to_string()
            .contains("turns/turn-1/prompt"));
    }

    #[tokio::test]
    async fn the_loader_reports_stored_turns_and_does_not_invent_decisions() {
        let root = std::env::temp_dir().join(format!("ax-context-export-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".ax")).unwrap();
        let database = ax_db::Database::open(&root.join(".ax/ax.db"))
            .await
            .unwrap();
        ax_db::hook_turns::record_hook_turn(
            database.pool(),
            &ax_db::hook_turns::HookTurn {
                project_id: "ax".into(),
                session_id: "conv-1".into(),
                turn_id: "gen-1".into(),
                prompt: "hello".into(),
            },
        )
        .await
        .unwrap();
        ax_db::hook_turns::record_hook_turn(
            database.pool(),
            &ax_db::hook_turns::HookTurn {
                project_id: "ax".into(),
                session_id: "conv-1".into(),
                turn_id: "gen-2".into(),
                prompt: "y".repeat(PROMPT_CAPTURE_CAP + 1),
            },
        )
        .await
        .unwrap();
        let input = load_export_input(database.pool(), "conv-1").await.unwrap();
        database.close().await;
        let _ = std::fs::remove_dir_all(&root);

        assert!(input.inventory_closed);
        assert_eq!(input.turns.len(), 2);
        assert_eq!(input.turns[0].prompt, "hello");
        assert_eq!(input.turns[1].prompt.chars().count(), PROMPT_CAPTURE_CAP);
        assert!(matches!(input.decisions, Section::NotCaptured));
        let bundle = validate_context_export(&input);
        assert!(bundle.errors.is_empty(), "{:?}", bundle.errors);
        assert_eq!(bundle.coverage.turns, "2 / 2");
        assert_eq!(bundle.coverage.decisions, "not captured");
        assert_eq!(bundle.full["provenance"]["completeness"], "partial");
        assert_eq!(bundle.full["turns"][1]["prompt"]["state"], "truncated");
    }

    #[test]
    fn the_published_fidelity_file_matches_a_fresh_run() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/specs/context-continuity/CONTEXT-EXPORT-FIDELITY.json");
        let document: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(document["equivalenceClaimed"], serde_json::json!(false));
        assert_eq!(document["crossChatExperiments"], serde_json::json!(0));
        let mut turns = vec![turn(1, "first"), turn(2, "second"), turn(3, "third")];
        turns[0].decision_ids = vec!["d-early".into(), "d-rejected".into()];
        let bundle = validate_context_export(&closed_input(turns));
        let fixture = document["fixtures"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["name"] == "closed-three-turns")
            .unwrap();
        assert_eq!(
            fixture["completeness"],
            bundle.full["provenance"]["completeness"]
        );
        assert_eq!(fixture["turns"], serde_json::json!(bundle.coverage.turns));
        assert_eq!(
            fixture["decisions"],
            serde_json::json!(bundle.coverage.decisions)
        );
        assert_eq!(
            fixture["rulesAndSkills"],
            serde_json::json!(bundle.coverage.rules_and_skills)
        );
        assert_eq!(
            fixture["unresolvedReferences"],
            serde_json::json!(bundle.coverage.unresolved_references)
        );
        assert_eq!(
            fixture["sourceIntegrity"],
            serde_json::json!(bundle.coverage.source_integrity)
        );
        assert_eq!(
            fixture["contextRelevance"],
            serde_json::json!(bundle.coverage.context_relevance)
        );
        assert_eq!(
            fixture["crossChatContinuity"],
            serde_json::json!(bundle.coverage.cross_chat_continuity)
        );
        assert_eq!(fixture["spec"], bundle.full["spec"]["availability"]);
        assert_eq!(bundle.ready["spec"]["availability"], bundle.full["spec"]["availability"]);
    }
}
