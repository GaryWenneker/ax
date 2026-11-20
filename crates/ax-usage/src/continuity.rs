//! Cross-chat context continuity.
//!
//! A checkpoint is the Ax knowledge state at one moment. A later chat retrieves
//! from that state by tag overlap. The transcript is never part of the checkpoint
//! and is never delivered to a new chat. This module proves the harness. It does
//! not claim a live model matched a long conversation.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const TRANSCRIPT_MARK: &str = "BEGIN TRANSCRIPT";
const NOT_ISSUED: &str = "not issued";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "SessionIdentityRaw")]
pub struct SessionIdentity {
    external_chat_id: Option<String>,
    external_turn_id: Option<String>,
    ax_session_id: String,
    ax_turn_id: Option<String>,
    agent_provider: Option<String>,
    project_id: String,
    created_at: String,
    parent_session_id: Option<String>,
}

impl SessionIdentity {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        external_chat_id: Option<String>,
        external_turn_id: Option<String>,
        ax_session_id: impl Into<String>,
        ax_turn_id: Option<String>,
        agent_provider: Option<String>,
        project_id: impl Into<String>,
        created_at: impl Into<String>,
        parent_session_id: Option<String>,
    ) -> Result<Self, String> {
        let identity = Self {
            external_chat_id: blank_to_none(external_chat_id),
            external_turn_id: blank_to_none(external_turn_id),
            ax_session_id: ax_session_id.into(),
            ax_turn_id: blank_to_none(ax_turn_id),
            agent_provider: blank_to_none(agent_provider),
            project_id: project_id.into(),
            created_at: created_at.into(),
            parent_session_id: blank_to_none(parent_session_id),
        };
        if identity.ax_session_id.trim().is_empty() {
            return Err("ax session id is required".into());
        }
        if identity.project_id.trim().is_empty() {
            return Err("project id is required".into());
        }
        if identity.created_at.trim().is_empty() {
            return Err("session creation time is required".into());
        }
        if identity.parent_session_id.as_deref() == Some(identity.ax_session_id.as_str()) {
            return Err("parent session must be a different session".into());
        }
        Ok(identity)
    }
}

#[derive(Deserialize)]
struct SessionIdentityRaw {
    external_chat_id: Option<String>,
    external_turn_id: Option<String>,
    ax_session_id: String,
    ax_turn_id: Option<String>,
    agent_provider: Option<String>,
    project_id: String,
    created_at: String,
    parent_session_id: Option<String>,
}

impl TryFrom<SessionIdentityRaw> for SessionIdentity {
    type Error = String;

    fn try_from(raw: SessionIdentityRaw) -> Result<Self, Self::Error> {
        Self::try_new(
            raw.external_chat_id,
            raw.external_turn_id,
            raw.ax_session_id,
            raw.ax_turn_id,
            raw.agent_provider,
            raw.project_id,
            raw.created_at,
            raw.parent_session_id,
        )
    }
}

fn blank_to_none(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyKind {
    ExternalChat,
    ExternalTurn,
    AxSession,
    AxTurn,
    All,
    TestReference,
}

pub fn copy_identifier(
    identity: &SessionIdentity,
    kind: CopyKind,
    reference: Option<&TestReference>,
) -> Result<String, String> {
    match kind {
        CopyKind::ExternalChat => issued(&identity.external_chat_id, "external chat id"),
        CopyKind::ExternalTurn => issued(&identity.external_turn_id, "external turn id"),
        CopyKind::AxSession => Ok(identity.ax_session_id.clone()),
        CopyKind::AxTurn => issued(&identity.ax_turn_id, "ax turn id"),
        CopyKind::All => Ok(render_all(identity)),
        CopyKind::TestReference => {
            let reference = reference.ok_or("context test reference is missing")?;
            Ok(render_reference(reference))
        }
    }
}

fn issued(value: &Option<String>, label: &str) -> Result<String, String> {
    value
        .clone()
        .ok_or_else(|| format!("{label} was {NOT_ISSUED}"))
}

fn render_all(identity: &SessionIdentity) -> String {
    [
        line("external_chat_id", identity.external_chat_id.as_deref()),
        line("external_turn_id", identity.external_turn_id.as_deref()),
        line("ax_session_id", Some(identity.ax_session_id.as_str())),
        line("ax_turn_id", identity.ax_turn_id.as_deref()),
        line("agent_provider", identity.agent_provider.as_deref()),
        line("project_id", Some(identity.project_id.as_str())),
        line("created_at", Some(identity.created_at.as_str())),
        line("parent_session_id", identity.parent_session_id.as_deref()),
    ]
    .join("\n")
}

fn line(label: &str, value: Option<&str>) -> String {
    format!("{label}: {}", value.unwrap_or(NOT_ISSUED))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestReference {
    pub id: String,
    pub source_session_id: String,
    pub checkpoint_id: String,
    pub mode: TestMode,
    pub task: String,
    pub target_agent: String,
    pub spec_id: Option<String>,
    pub spec_version: Option<i64>,
    pub spec_hash: Option<String>,
}

pub fn continuity_prompt(reference: &TestReference) -> Result<String, String> {
    if reference.id.trim().is_empty() || reference.checkpoint_id.trim().is_empty() {
        return Err("a continuity prompt needs a reference id and a checkpoint id".into());
    }
    if reference.task.contains(TRANSCRIPT_MARK) {
        return Err("the task must not carry a transcript".into());
    }
    let spec = match (
        reference.spec_id.as_deref(),
        reference.spec_version,
        reference.spec_hash.as_deref(),
    ) {
        (Some(id), Some(version), Some(hash)) if !id.trim().is_empty() && !hash.trim().is_empty() => {
            format!("Spec: {id}@{version} {hash}\nPointer: ax spec show {id}@{version}\n")
        }
        _ => "Spec: none\n".into(),
    };
    Ok(format!(
        "Retrieve context through Ax for continuity test {id}.\n\
         Source session: {session}\n\
         Checkpoint: {checkpoint}\n\
         Mode: {mode}\n\
         Target agent: {agent}\n\
         Task: {task}\n\
         {spec}\
         Use normal Ax retrieval for this reference. Historical answers are not included.",
        id = reference.id,
        session = reference.source_session_id,
        checkpoint = reference.checkpoint_id,
        mode = reference.mode.as_str(),
        agent = reference.target_agent,
        spec = spec,
        task = reference.task,
    ))
}

fn render_reference(reference: &TestReference) -> String {
    format!(
        "reference_id: {id}\nsource_session_id: {session}\ncheckpoint_id: {checkpoint}\nmode: {mode}\ntarget_agent: {agent}\ntask: {task}",
        id = reference.id,
        session = reference.source_session_id,
        checkpoint = reference.checkpoint_id,
        mode = reference.mode.as_str(),
        agent = reference.target_agent,
        task = reference.task,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestMode {
    Historical,
    Current,
}

impl TestMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Historical => "historical",
            Self::Current => "current",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Objective,
    Implementation,
    Decision,
    Memory,
    Graph,
    Rule,
    Skill,
    Failure,
    Pending,
    Constraint,
    File,
    GlobalMemory,
    Handoff,
    Transcript,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeItem {
    pub id: String,
    pub kind: ItemKind,
    pub text: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub version: String,
    pub at: String,
    pub tags: Vec<String>,
    pub historical: bool,
    pub decision_key: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Arm {
    LongConversation,
    NewWithAx,
    NegativeControl,
}

impl Arm {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LongConversation => "A",
            Self::NewWithAx => "B",
            Self::NegativeControl => "C",
        }
    }

    fn session(self, source: &str) -> String {
        match self {
            Self::LongConversation => source.to_string(),
            Self::NewWithAx => format!("axs_new_{source}"),
            Self::NegativeControl => format!("axs_control_{source}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Query {
    pub arm: Arm,
    pub mode: TestMode,
    pub tags: Vec<String>,
    pub cutoff: String,
    pub explain: bool,
    pub now: String,
}

pub fn select_context(items: &[KnowledgeItem], query: &Query) -> Vec<KnowledgeItem> {
    let mut pool: Vec<KnowledgeItem> = items
        .iter()
        .filter(|item| item.kind != ItemKind::Transcript && item.kind != ItemKind::Handoff)
        .filter(|item| in_mode(item, query))
        .cloned()
        .collect();
    if query.arm == Arm::NegativeControl {
        pool.retain(|item| !item.historical && item.kind != ItemKind::GlobalMemory);
        pool.sort_by(|left, right| left.id.cmp(&right.id));
        return pool;
    }
    resolve_conflicts(&mut pool);
    pool.retain(|item| overlaps(&item.tags, &query.tags));
    pool.sort_by(|left, right| left.id.cmp(&right.id));
    pool
}

fn in_mode(item: &KnowledgeItem, query: &Query) -> bool {
    match query.mode {
        TestMode::Historical => item.at.as_str() <= query.cutoff.as_str(),
        TestMode::Current => true,
    }
}

fn resolve_conflicts(items: &mut Vec<KnowledgeItem>) {
    let mut winner: BTreeMap<String, String> = BTreeMap::new();
    for item in items.iter() {
        let Some(key) = item.decision_key.as_ref() else {
            continue;
        };
        let replace = winner
            .get(key)
            .map(|at| item.at.as_str() >= at.as_str())
            .unwrap_or(true);
        if replace {
            winner.insert(key.clone(), item.at.clone());
        }
    }
    items.retain(|item| match &item.decision_key {
        Some(key) => winner.get(key).map(String::as_str) == Some(item.at.as_str()),
        None => true,
    });
}

fn overlaps(item_tags: &[String], query_tags: &[String]) -> bool {
    item_tags
        .iter()
        .any(|tag| query_tags.iter().any(|wanted| wanted == tag))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attribution {
    pub item_id: String,
    pub source: String,
    pub original_session_id: String,
    pub original_turn_id: Option<String>,
    pub version: String,
    pub method: String,
    pub score: f64,
    pub delivered_at: String,
    pub received: bool,
    pub use_observed: bool,
    pub why: Option<String>,
}

pub fn attribute(item: &KnowledgeItem, query: &Query, cited: bool) -> Attribution {
    let hits = item
        .tags
        .iter()
        .filter(|tag| query.tags.iter().any(|wanted| wanted == *tag))
        .count();
    let score = if query.tags.is_empty() {
        0.0
    } else {
        hits as f64 / query.tags.len() as f64
    };
    let why = query.explain.then(|| {
        format!(
            "tag-overlap {hits} of {} on {}",
            query.tags.join(","),
            item.id
        )
    });
    Attribution {
        item_id: item.id.clone(),
        source: format!("{:?}:{}", item.kind, item.id),
        original_session_id: item.session_id.clone(),
        original_turn_id: item.turn_id.clone(),
        version: item.version.clone(),
        method: "tag-overlap".into(),
        score,
        delivered_at: query.now.clone(),
        received: true,
        use_observed: cited,
        why,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CheckpointBody {
    id: String,
    objective: String,
    implementation_state: String,
    decisions: Vec<String>,
    memory_ids: Vec<String>,
    graph: Vec<String>,
    rules: Vec<String>,
    skills: Vec<String>,
    failures: Vec<String>,
    pending: Vec<String>,
    constraints: Vec<String>,
    files: Vec<String>,
    commit: String,
    knowledge_version: String,
    knowledge_at: String,
    items: Vec<KnowledgeItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct CheckpointEnvelope {
    body: CheckpointBody,
    hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Checkpoint {
    envelope: CheckpointEnvelope,
}

impl Checkpoint {
    pub fn id(&self) -> &str {
        &self.envelope.body.id
    }

    pub fn hash(&self) -> &str {
        &self.envelope.hash
    }

    pub fn items(&self) -> &[KnowledgeItem] {
        &self.envelope.body.items
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(&self.envelope).expect("checkpoint envelope is serializable")
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let envelope: CheckpointEnvelope =
            serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        let expected = hash_body(&envelope.body)?;
        if envelope.hash != expected {
            return Err("checkpoint hash does not match its body".into());
        }
        Ok(Self { envelope })
    }
}

#[derive(Debug, Clone)]
pub struct SealMeta {
    pub id: String,
    pub commit: String,
    pub cutoff: String,
}

pub fn seal_checkpoint(items: &[KnowledgeItem], meta: &SealMeta) -> Result<Checkpoint, String> {
    let kept: Vec<KnowledgeItem> = items
        .iter()
        .filter(|item| item.at.as_str() <= meta.cutoff.as_str())
        .filter(|item| item.kind != ItemKind::Transcript && item.kind != ItemKind::Handoff)
        .filter(|item| !item.text.contains(TRANSCRIPT_MARK))
        .cloned()
        .collect();
    if !kept.iter().any(|item| item.kind == ItemKind::Objective) {
        return Err("checkpoint needs a project objective".into());
    }
    let body = CheckpointBody {
        id: meta.id.clone(),
        objective: join_kind(&kept, ItemKind::Objective),
        implementation_state: join_kind(&kept, ItemKind::Implementation),
        decisions: texts(&kept, ItemKind::Decision),
        memory_ids: ids(&kept, ItemKind::Memory),
        graph: texts(&kept, ItemKind::Graph),
        rules: ids(&kept, ItemKind::Rule),
        skills: ids(&kept, ItemKind::Skill),
        failures: texts(&kept, ItemKind::Failure),
        pending: texts(&kept, ItemKind::Pending),
        constraints: texts(&kept, ItemKind::Constraint),
        files: texts(&kept, ItemKind::File),
        commit: meta.commit.clone(),
        knowledge_version: kept
            .iter()
            .map(|item| item.version.as_str())
            .max()
            .unwrap_or("0")
            .to_string(),
        knowledge_at: meta.cutoff.clone(),
        items: kept,
    };
    let hash = hash_body(&body)?;
    Ok(Checkpoint {
        envelope: CheckpointEnvelope { body, hash },
    })
}

fn join_kind(items: &[KnowledgeItem], kind: ItemKind) -> String {
    texts(items, kind).join("\n")
}

fn texts(items: &[KnowledgeItem], kind: ItemKind) -> Vec<String> {
    items
        .iter()
        .filter(|item| item.kind == kind)
        .map(|item| item.text.clone())
        .collect()
}

fn ids(items: &[KnowledgeItem], kind: ItemKind) -> Vec<String> {
    items
        .iter()
        .filter(|item| item.kind == kind)
        .map(|item| item.id.clone())
        .collect()
}

fn hash_body(body: &CheckpointBody) -> Result<String, String> {
    let bytes = serde_json::to_vec(body).map_err(|error| error.to_string())?;
    let digest = Sha256::digest(bytes);
    Ok(format!("{digest:x}"))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArmReport {
    pub arm: Arm,
    pub mode: TestMode,
    pub session_id: String,
    pub sees_transcript: bool,
    pub delivered_ids: Vec<String>,
    pub delivered_text: String,
    pub available_text: String,
    pub cited_ids: Vec<String>,
    pub attributions: Vec<Attribution>,
    pub passed_tests: Vec<String>,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub retrieval_latency_ms: u32,
    pub execution_ms: u32,
    pub retries: u32,
    pub worktree: String,
    pub files: BTreeMap<String, String>,
    pub model: String,
    pub config: String,
    pub task: String,
    pub limit_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct TaskSpec {
    pub description: String,
    pub tags: Vec<String>,
    pub required_ids: Vec<String>,
    pub decision_ids: Vec<String>,
    pub rule_ids: Vec<String>,
    pub acceptance: Vec<String>,
    pub forbidden_answers: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct World {
    pub snapshot: String,
    pub model: String,
    pub source_session: String,
    pub transcript: String,
    pub items: Vec<KnowledgeItem>,
    pub task: TaskSpec,
    pub limits_tokens: u32,
    pub config: String,
}

pub fn run_arm(world: &World, query: &Query) -> ArmReport {
    let selected = select_context(&world.items, query);
    let cited = cite(&world.task.required_ids, &selected);
    let passed = if world
        .task
        .required_ids
        .iter()
        .all(|id| cited.iter().any(|cited_id| cited_id == id))
    {
        world.task.acceptance.clone()
    } else {
        Vec::new()
    };
    let delivered_text = selected
        .iter()
        .map(|item| item.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let sees_transcript = query.arm == Arm::LongConversation;
    let available_text = if sees_transcript {
        format!("{}\n{delivered_text}", world.transcript)
    } else {
        delivered_text.clone()
    };
    let input_tokens = tokens(&available_text);
    let retrieval_latency_ms = u32::try_from(selected.len()).unwrap_or(u32::MAX);
    let mut files = BTreeMap::new();
    files.insert("arm.txt".into(), query.arm.as_str().to_string());
    ArmReport {
        arm: query.arm,
        mode: query.mode,
        session_id: query.arm.session(&world.source_session),
        sees_transcript,
        delivered_ids: selected.iter().map(|item| item.id.clone()).collect(),
        attributions: selected
            .iter()
            .map(|item| attribute(item, query, cited.iter().any(|id| id == &item.id)))
            .collect(),
        delivered_text,
        available_text,
        cited_ids: cited,
        passed_tests: passed,
        input_tokens,
        output_tokens: 0,
        retrieval_latency_ms,
        execution_ms: retrieval_latency_ms.saturating_add(input_tokens),
        retries: 0,
        worktree: format!("{}/arm-{}", world.snapshot, query.arm.as_str()),
        files,
        model: world.model.clone(),
        config: world.config.clone(),
        task: world.task.description.clone(),
        limit_tokens: world.limits_tokens,
    }
}

fn cite(required: &[String], selected: &[KnowledgeItem]) -> Vec<String> {
    required
        .iter()
        .filter(|id| selected.iter().any(|item| item.id == **id))
        .cloned()
        .collect()
}

fn tokens(text: &str) -> u32 {
    u32::try_from(text.len() / 4).unwrap_or(u32::MAX)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trio {
    pub a: ArmReport,
    pub b: ArmReport,
    pub c: ArmReport,
    pub required_ids: Vec<String>,
    pub decision_ids: Vec<String>,
    pub rule_ids: Vec<String>,
    pub acceptance: Vec<String>,
    pub transcript: String,
    pub forbidden_answers: Vec<String>,
    pub historical_ids: Vec<String>,
    pub repeat_ids: Vec<String>,
}

pub fn run_trio(world: &World, mode: TestMode, cutoff: &str, explain: bool) -> Trio {
    let query = |arm| Query {
        arm,
        mode,
        tags: world.task.tags.clone(),
        cutoff: cutoff.to_string(),
        explain,
        now: cutoff.to_string(),
    };
    let b = run_arm(world, &query(Arm::NewWithAx));
    Trio {
        a: run_arm(world, &query(Arm::LongConversation)),
        c: run_arm(world, &query(Arm::NegativeControl)),
        repeat_ids: run_arm(world, &query(Arm::NewWithAx)).delivered_ids,
        required_ids: world.task.required_ids.clone(),
        decision_ids: world.task.decision_ids.clone(),
        rule_ids: world.task.rule_ids.clone(),
        acceptance: world.task.acceptance.clone(),
        transcript: world.transcript.clone(),
        forbidden_answers: world.task.forbidden_answers.clone(),
        historical_ids: world
            .items
            .iter()
            .filter(|item| item.historical)
            .map(|item| item.id.clone())
            .collect(),
        b,
    }
}

pub fn gate_failures(trio: &Trio) -> Vec<&'static str> {
    let mut failed = Vec::new();
    let sessions = [
        trio.a.session_id.as_str(),
        trio.b.session_id.as_str(),
        trio.c.session_id.as_str(),
    ];
    let independent = sessions.iter().collect::<BTreeSet<_>>().len() == 3
        && trio.a.sees_transcript
        && !trio.b.sees_transcript
        && !trio.c.sees_transcript
        && trio.a.mode == trio.b.mode
        && trio.b.mode == trio.c.mode
        && trio.a.worktree != trio.b.worktree
        && trio.b.worktree != trio.c.worktree
        && trio.a.model == trio.b.model
        && trio.b.model == trio.c.model
        && trio.a.config == trio.b.config
        && trio.b.config == trio.c.config
        && trio.a.task == trio.b.task
        && trio.b.task == trio.c.task
        && trio.a.input_tokens <= trio.a.limit_tokens
        && trio.b.input_tokens <= trio.b.limit_tokens
        && trio.c.input_tokens <= trio.c.limit_tokens;
    if !independent {
        failed.push("independent");
    }
    let leaked_text = |report: &ArmReport| {
        report.delivered_text.contains(TRANSCRIPT_MARK)
            || report.available_text.contains(TRANSCRIPT_MARK)
            || trio
                .forbidden_answers
                .iter()
                .any(|answer| report.delivered_text.contains(answer))
    };
    if leaked_text(&trio.b) || leaked_text(&trio.c) {
        failed.push("transcript");
    }
    let retrieved = trio
        .required_ids
        .iter()
        .all(|id| trio.b.delivered_ids.iter().any(|got| got == id));
    if !retrieved {
        failed.push("retrieved");
    }
    if trio.b.passed_tests != trio.acceptance {
        failed.push("task");
    }
    let policies = trio
        .decision_ids
        .iter()
        .chain(trio.rule_ids.iter())
        .all(|id| trio.b.cited_ids.iter().any(|cited| cited == id));
    if !policies {
        failed.push("policy");
    }
    let leak = trio
        .historical_ids
        .iter()
        .any(|id| trio.c.delivered_ids.iter().any(|got| got == id));
    if leak {
        failed.push("leakage");
    }
    let sourced = trio.b.attributions.iter().all(|item| {
        !item.source.is_empty()
            && !item.original_session_id.is_empty()
            && !item.method.is_empty()
            && item.score.is_finite()
            && !item.delivered_at.is_empty()
    });
    if !sourced {
        failed.push("sources");
    }
    if trio.repeat_ids != trio.b.delivered_ids {
        failed.push("reproducible");
    }
    failed
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scorecard {
    pub fact_recall: f64,
    pub fact_precision: f64,
    pub decision_consistency: f64,
    pub rule_compliance: f64,
    pub acceptance_pass_rate: f64,
    pub outcome_equivalence: f64,
    pub token_overhead: f64,
    pub retrieval_latency_ms: u32,
    pub negative_control_pass_rate: f64,
    pub confidence_low: Option<f64>,
    pub confidence_high: Option<f64>,
    pub historical_dependency: bool,
}

pub fn score(trio: &Trio, repeats: u32) -> Scorecard {
    let recall = ratio(
        trio.required_ids
            .iter()
            .filter(|id| trio.b.delivered_ids.iter().any(|got| got == *id))
            .count(),
        trio.required_ids.len(),
    );
    let precise = trio
        .b
        .delivered_ids
        .iter()
        .filter(|id| {
            trio.b
                .attributions
                .iter()
                .any(|item| item.item_id == **id && item.score > 0.0)
        })
        .count();
    let precision = ratio(precise, trio.b.delivered_ids.len());
    let decisions = ratio(
        trio.decision_ids
            .iter()
            .filter(|id| trio.b.cited_ids.iter().any(|cited| cited == *id))
            .count(),
        trio.decision_ids.len(),
    );
    let rules = ratio(
        trio.rule_ids
            .iter()
            .filter(|id| trio.b.cited_ids.iter().any(|cited| cited == *id))
            .count(),
        trio.rule_ids.len(),
    );
    let pass_rate = ratio(trio.b.passed_tests.len(), trio.acceptance.len());
    let equivalence = if trio.a.passed_tests == trio.b.passed_tests {
        1.0
    } else {
        0.0
    };
    let overhead = if trio.a.input_tokens == 0 {
        0.0
    } else {
        f64::from(trio.b.input_tokens) / f64::from(trio.a.input_tokens)
    };
    let control = ratio(trio.c.passed_tests.len(), trio.acceptance.len());
    let (low, high) = confidence(pass_rate, repeats);
    Scorecard {
        fact_recall: recall,
        fact_precision: precision,
        decision_consistency: decisions,
        rule_compliance: rules,
        acceptance_pass_rate: pass_rate,
        outcome_equivalence: equivalence,
        token_overhead: overhead,
        retrieval_latency_ms: trio.b.retrieval_latency_ms,
        negative_control_pass_rate: control,
        confidence_low: low,
        confidence_high: high,
        historical_dependency: pass_rate == 1.0 && control == 0.0 && recall == 1.0,
    }
}

fn ratio(hit: usize, total: usize) -> f64 {
    if total == 0 {
        1.0
    } else {
        hit as f64 / total as f64
    }
}

fn confidence(point: f64, repeats: u32) -> (Option<f64>, Option<f64>) {
    if repeats < 2 {
        return (None, None);
    }
    let n = f64::from(repeats);
    let z = 1.96;
    let denom = 1.0 + (z * z) / n;
    let center = (point + (z * z) / (2.0 * n)) / denom;
    let margin = z * ((point * (1.0 - point) / n) + (z * z) / (4.0 * n * n)).sqrt() / denom;
    let low = (center - margin).clamp(0.0, 1.0);
    let high = (center + margin).clamp(0.0, 1.0);
    (Some(low), Some(high))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scenario {
    UnfinishedFeature,
    ArchitecturalDecision,
    DiagnosedBug,
    ProjectRule,
    EarlyMemory,
    ModifiedFile,
    DaemonRestart,
    IdeRestart,
    DifferentAgent,
    Replica,
    ConflictingDecisions,
    UnrelatedHistory,
}

impl Scenario {
    pub fn all() -> [Self; 12] {
        [
            Self::UnfinishedFeature,
            Self::ArchitecturalDecision,
            Self::DiagnosedBug,
            Self::ProjectRule,
            Self::EarlyMemory,
            Self::ModifiedFile,
            Self::DaemonRestart,
            Self::IdeRestart,
            Self::DifferentAgent,
            Self::Replica,
            Self::ConflictingDecisions,
            Self::UnrelatedHistory,
        ]
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnfinishedFeature => "unfinished_feature",
            Self::ArchitecturalDecision => "architectural_decision",
            Self::DiagnosedBug => "diagnosed_bug",
            Self::ProjectRule => "project_rule",
            Self::EarlyMemory => "early_memory",
            Self::ModifiedFile => "modified_file",
            Self::DaemonRestart => "daemon_restart",
            Self::IdeRestart => "ide_restart",
            Self::DifferentAgent => "different_agent",
            Self::Replica => "replica",
            Self::ConflictingDecisions => "conflicting_decisions",
            Self::UnrelatedHistory => "unrelated_history",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenarioResult {
    pub scenario: Scenario,
    pub gates: Vec<String>,
    pub score: Scorecard,
    pub checkpoint_hash: String,
    pub mode: TestMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationReport {
    pub scenarios: Vec<ScenarioResult>,
    pub equivalence_claimed: bool,
    pub live_agent_experiments: u32,
    pub latency_model: String,
}

pub fn validate_continuity() -> Result<ValidationReport, Vec<String>> {
    let mut scenarios = Vec::new();
    let mut errors = Vec::new();
    for scenario in Scenario::all() {
        match run_scenario(scenario) {
            Ok(result) => scenarios.push(result),
            Err(message) => errors.push(format!("{}: {message}", scenario.as_str())),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(ValidationReport {
        scenarios,
        equivalence_claimed: false,
        live_agent_experiments: 0,
        latency_model: "1ms per retrieved item; input tokens are utf-8 bytes divided by 4".into(),
    })
}

fn run_scenario(scenario: Scenario) -> Result<ScenarioResult, String> {
    let built = build(scenario);
    let trio = run_trio(&built.world, built.mode, &built.cutoff, true);
    let failed = gate_failures(&trio);
    if !failed.is_empty() {
        return Err(format!("gates failed: {}", failed.join(",")));
    }
    built.extra(&trio)?;
    if trio.b.attributions.iter().any(|item| item.why.is_none()) {
        return Err("diagnostic mode omitted a reason".into());
    }
    let quiet = run_trio(&built.world, built.mode, &built.cutoff, false);
    if quiet.b.attributions.iter().any(|item| item.why.is_some()) {
        return Err("diagnostic reasons appeared when the mode was off".into());
    }
    let checkpoint = seal_checkpoint(&built.world.items, &built.seal)?;
    let restored = Checkpoint::from_bytes(&checkpoint.to_bytes())?;
    if restored.hash() != checkpoint.hash() {
        return Err("restarted checkpoint hash changed".into());
    }
    Ok(ScenarioResult {
        scenario,
        gates: Vec::new(),
        score: score(&trio, built.repeats),
        checkpoint_hash: checkpoint.hash().to_string(),
        mode: built.mode,
    })
}

struct Built {
    world: World,
    mode: TestMode,
    cutoff: String,
    seal: SealMeta,
    repeats: u32,
    check: fn(&Trio) -> Result<(), String>,
}

impl Built {
    fn extra(&self, trio: &Trio) -> Result<(), String> {
        (self.check)(trio)
    }
}

fn build(scenario: Scenario) -> Built {
    let cutoff = match scenario {
        Scenario::ConflictingDecisions => "2026-06-01T00:00:00Z".to_string(),
        _ => "2026-11-01T00:00:00Z".to_string(),
    };
    let mode = TestMode::Historical;
    let (required, decisions, rules, acceptance) = match scenario {
        Scenario::ArchitecturalDecision | Scenario::ConflictingDecisions => (
            vec!["decision-storage".into()],
            vec!["decision-storage".into()],
            vec![],
            vec!["tests_pass".into()],
        ),
        Scenario::ProjectRule => (
            vec!["rule-logging".into()],
            vec![],
            vec!["rule-logging".into()],
            vec!["rule_checked".into()],
        ),
        Scenario::DiagnosedBug => (
            vec!["failure-sqlite".into()],
            vec![],
            vec![],
            vec!["bug_not_repeated".into()],
        ),
        Scenario::EarlyMemory => (
            vec!["memory-early".into()],
            vec![],
            vec![],
            vec!["memory_used".into()],
        ),
        Scenario::ModifiedFile => (
            vec!["file-continuity".into()],
            vec![],
            vec![],
            vec!["file_found".into()],
        ),
        Scenario::UnfinishedFeature => (
            vec!["pending-feature".into()],
            vec![],
            vec![],
            vec!["feature_continues".into()],
        ),
        _ => (
            vec!["pending-feature".into()],
            vec![],
            vec![],
            vec!["feature_continues".into()],
        ),
    };
    let extra: fn(&Trio) -> Result<(), String> = match scenario {
        Scenario::ConflictingDecisions => conflicting_extra,
        Scenario::DifferentAgent => different_agent_extra,
        Scenario::Replica => replica_extra,
        Scenario::IdeRestart => ide_restart_extra,
        Scenario::UnrelatedHistory => unrelated_extra,
        Scenario::DaemonRestart => daemon_extra,
        _ => no_extra,
    };
    Built {
        world: World {
            snapshot: format!("snap-{scenario}", scenario = scenario.as_str()),
            model: "fixture-model-1".into(),
            source_session: "axs_source".into(),
            transcript: format!("{TRANSCRIPT_MARK}\n{}", "x".repeat(2000)),
            items: items_for(scenario),
            task: TaskSpec {
                description: format!("Continue {}", scenario.as_str()),
                tags: vec!["work".into()],
                required_ids: required,
                decision_ids: decisions,
                rule_ids: rules,
                acceptance,
                forbidden_answers: vec!["secret historical answer".into()],
            },
            limits_tokens: 8000,
            config: "offline-project".into(),
        },
        mode,
        cutoff: cutoff.clone(),
        seal: SealMeta {
            id: format!("cp-{}", scenario.as_str()),
            commit: "3d4bcb3803da262b20e93b70ad0286c84bb22bee".into(),
            cutoff: cutoff.clone(),
        },
        repeats: if scenario == Scenario::UnfinishedFeature {
            2
        } else {
            1
        },
        check: extra,
    }
}

fn no_extra(_: &Trio) -> Result<(), String> {
    Ok(())
}

fn items_for(scenario: Scenario) -> Vec<KnowledgeItem> {
    let mut items = vec![
        item(
            "objective",
            ItemKind::Objective,
            "Keep logging and retrieval local.",
            "2026-01-01T00:00:00Z",
            true,
            None,
        ),
        item(
            "impl-state",
            ItemKind::Implementation,
            "Logging UI is on main.",
            "2026-06-01T00:00:00Z",
            true,
            None,
        ),
        item(
            "readme-tool",
            ItemKind::File,
            "README.md",
            "2026-01-01T00:00:00Z",
            false,
            None,
        ),
        item(
            "global-memory",
            ItemKind::GlobalMemory,
            "machine-wide note",
            "2026-06-01T00:00:00Z",
            true,
            None,
        ),
        item(
            "handoff",
            ItemKind::Handoff,
            format!("{TRANSCRIPT_MARK} parent"),
            "2026-06-01T00:00:00Z",
            true,
            None,
        ),
        item(
            "transcript-item",
            ItemKind::Transcript,
            format!("{TRANSCRIPT_MARK} chat"),
            "2026-06-01T00:00:00Z",
            true,
            None,
        ),
        item(
            "unrelated",
            ItemKind::Memory,
            "recipe for soup",
            "2026-11-01T00:00:00Z",
            true,
            None,
        ),
    ];
    if let Some(unrelated) = items.iter_mut().find(|item| item.id == "unrelated") {
        unrelated.tags = vec!["cooking".into()];
    }
    match scenario {
        Scenario::ArchitecturalDecision => items.push(decision("use sqlite for the project graph")),
        Scenario::ConflictingDecisions => {
            items.push(decision("use sqlite for the project graph"));
            let mut later = decision("use postgres for the project graph");
            later.id = "decision-postgres".into();
            later.at = "2026-11-01T00:00:00Z".into();
            later.text = "secret historical answer".into();
            items.push(later);
        }
        Scenario::ProjectRule => items.push(item(
            "rule-logging",
            ItemKind::Rule,
            "Stamp session and turn on every log line.",
            "2026-06-01T00:00:00Z",
            true,
            None,
        )),
        Scenario::DiagnosedBug => items.push(item(
            "failure-sqlite",
            ItemKind::Failure,
            "A missing ax.db made the hook miss the project. Create the empty database first.",
            "2026-06-01T00:00:00Z",
            true,
            None,
        )),
        Scenario::EarlyMemory => items.push(item(
            "memory-early",
            ItemKind::Memory,
            "The release tag comes from the ax-cli package version.",
            "2026-01-01T00:00:00Z",
            true,
            Some("turn-1".into()),
        )),
        Scenario::ModifiedFile => items.push(item(
            "file-continuity",
            ItemKind::File,
            "crates/ax-usage/src/continuity.rs",
            "2026-06-01T00:00:00Z",
            true,
            None,
        )),
        _ => items.push(item(
            "pending-feature",
            ItemKind::Pending,
            "Finish context continuity validation.",
            "2026-06-01T00:00:00Z",
            true,
            None,
        )),
    }
    items
}

fn decision(text: &str) -> KnowledgeItem {
    item(
        "decision-storage",
        ItemKind::Decision,
        text,
        "2026-06-01T00:00:00Z",
        true,
        None,
    )
    .with_key("storage")
}

fn item(
    id: &str,
    kind: ItemKind,
    text: impl Into<String>,
    at: &str,
    historical: bool,
    turn_id: Option<String>,
) -> KnowledgeItem {
    KnowledgeItem {
        id: id.into(),
        kind,
        text: text.into(),
        session_id: "axs_source".into(),
        turn_id,
        version: "v1".into(),
        at: at.into(),
        tags: vec!["work".into()],
        historical,
        decision_key: None,
    }
}

impl KnowledgeItem {
    fn with_key(mut self, key: &str) -> Self {
        self.decision_key = Some(key.into());
        self
    }
}

fn conflicting_extra(trio: &Trio) -> Result<(), String> {
    if trio.b.delivered_text.contains("postgres")
        || trio.b.delivered_text.contains("secret historical answer")
    {
        return Err("historical mode included the later decision".into());
    }
    let current = Query {
        arm: Arm::NewWithAx,
        mode: TestMode::Current,
        tags: vec!["work".into()],
        cutoff: "2026-06-01T00:00:00Z".into(),
        explain: false,
        now: "2026-11-20T00:00:00Z".into(),
    };
    let world_items = items_for(Scenario::ConflictingDecisions);
    let selected = select_context(&world_items, &current);
    if !selected.iter().any(|item| item.id == "decision-postgres") {
        return Err("current mode did not keep the later decision".into());
    }
    if selected.iter().any(|item| item.id == "decision-storage") {
        return Err("current mode kept both conflicting decisions".into());
    }
    let mixed = Trio {
        a: ArmReport {
            mode: TestMode::Current,
            ..trio.a.clone()
        },
        ..trio.clone()
    };
    if !gate_failures(&mixed).contains(&"independent") {
        return Err("mixed modes were accepted".into());
    }
    Ok(())
}

fn different_agent_extra(trio: &Trio) -> Result<(), String> {
    let cursor = SessionIdentity::try_new(
        Some("cursor-chat-1".into()),
        None,
        trio.a.session_id.clone(),
        None,
        Some("cursor".into()),
        "ax",
        "2026-11-20T09:00:00Z",
        None,
    )?;
    let claude = SessionIdentity::try_new(
        Some("claude-chat-9".into()),
        None,
        trio.b.session_id.clone(),
        None,
        Some("claude".into()),
        "ax",
        "2026-11-20T10:00:00Z",
        Some(cursor.ax_session_id.clone()),
    )?;
    if cursor.agent_provider == claude.agent_provider
        || cursor.ax_session_id == claude.ax_session_id
    {
        return Err("the second agent reused the first chat identity".into());
    }
    match copy_identifier(&claude, CopyKind::ExternalTurn, None) {
        Ok(text) => Err(format!("a missing turn id was fabricated: {text}")),
        Err(error) if error.contains("axs_") || error.contains("chat-") => {
            Err("the missing-id error contained a fabricated id".into())
        }
        Err(_) => Ok(()),
    }
}

fn replica_extra(trio: &Trio) -> Result<(), String> {
    let seal = SealMeta {
        id: "cp-replica".into(),
        commit: "3d4bcb3803da262b20e93b70ad0286c84bb22bee".into(),
        cutoff: "2026-11-01T00:00:00Z".into(),
    };
    let original = seal_checkpoint(&items_for(Scenario::Replica), &seal)?;
    let copy = Checkpoint::from_bytes(&original.to_bytes())?;
    if copy.hash() != original.hash() || copy.items().len() != original.items().len() {
        return Err("replica checkpoint diverged".into());
    }
    if trio.b.session_id == trio.a.session_id {
        return Err("replica reused the source session".into());
    }
    Ok(())
}

fn ide_restart_extra(_: &Trio) -> Result<(), String> {
    let identity = SessionIdentity::try_new(
        None,
        None,
        "axs_source",
        None,
        None,
        "ax",
        "2026-11-20T09:00:00Z",
        None,
    )?;
    let bytes = serde_json::to_vec(&identity).map_err(|error| error.to_string())?;
    let restored: SessionIdentity =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if restored.external_chat_id.is_some() || restored.external_turn_id.is_some() {
        return Err("restart fabricated a provider id".into());
    }
    match copy_identifier(&restored, CopyKind::ExternalChat, None) {
        Ok(text) => Err(format!("restart fabricated a provider id: {text}")),
        Err(error) if error.contains("axs_") || error.contains("chat-") => {
            Err("the missing-id error contained a fabricated id".into())
        }
        Err(_) => Ok(()),
    }
}

fn unrelated_extra(trio: &Trio) -> Result<(), String> {
    if trio.b.delivered_ids.iter().any(|id| id == "unrelated") {
        return Err("unrelated memory was delivered".into());
    }
    Ok(())
}

fn daemon_extra(trio: &Trio) -> Result<(), String> {
    if trio.b.worktree == trio.c.worktree || trio.b.files == trio.c.files {
        return Err("worktrees were shared".into());
    }
    Ok(())
}

pub fn render_report(report: &ValidationReport) -> String {
    let mut lines = vec![
        "# Context continuity results".to_string(),
        String::new(),
        "These figures come from the in-process harness in `crates/ax-usage/src/continuity.rs`.".into(),
        "The agent is a fixture that passes a task only when every required Ax item was delivered.".into(),
        "Retrieval latency is one millisecond per retrieved item. Input tokens are UTF-8 bytes divided by four.".into(),
        format!(
            "Equivalence claimed: {}. Live agent experiments: {}.",
            report.equivalence_claimed, report.live_agent_experiments
        ),
        String::new(),
        "| Scenario | Recall | Precision | Decisions | Rules | Pass | Equivalence | Token overhead | Control pass |".into(),
        "|---|---:|---:|---:|---:|---:|---:|---:|---:|".into(),
    ];
    for scenario in &report.scenarios {
        let score = &scenario.score;
        lines.push(format!(
            "| {} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} |",
            scenario.scenario.as_str(),
            score.fact_recall,
            score.fact_precision,
            score.decision_consistency,
            score.rule_compliance,
            score.acceptance_pass_rate,
            score.outcome_equivalence,
            score.token_overhead,
            score.negative_control_pass_rate,
        ));
    }
    lines.push(String::new());
    lines.push("Ax does not provide long-chat equivalence. No live model was compared.".into());
    lines.push(String::new());
    lines.join("\n")
}

pub fn same_mode(left: TestMode, right: TestMode) -> Result<(), String> {
    if left != right {
        Err("historical and current results cannot be compared".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_session_id_does_not_deserialize() {
        let raw = r#"{"external_chat_id":null,"external_turn_id":null,"ax_session_id":"","ax_turn_id":null,"agent_provider":null,"project_id":"ax","created_at":"2026-11-20T09:00:00Z","parent_session_id":null}"#;
        let parsed = serde_json::from_str::<SessionIdentity>(raw);
        assert!(parsed.is_err(), "{parsed:?}");
    }

    #[test]
    fn missing_provider_ids_are_not_invented() {
        let identity = SessionIdentity::try_new(
            Some("  ".into()),
            None,
            "axs_source",
            None,
            None,
            "ax",
            "2026-11-20T09:00:00Z",
            None,
        )
        .unwrap();
        let error = copy_identifier(&identity, CopyKind::ExternalChat, None).unwrap_err();
        assert_eq!(error, "external chat id was not issued");
        let all = copy_identifier(&identity, CopyKind::All, None).unwrap();
        assert!(all.contains("external_chat_id: not issued"), "{all}");
        assert!(
            !all.contains("axs_minted") && !all.contains("chat-"),
            "{all}"
        );
        assert!(all.contains("ax_session_id: axs_source"), "{all}");
    }

    #[test]
    fn prompt_names_the_reference_and_not_the_answer() {
        let reference = TestReference {
            id: "ctr-1".into(),
            source_session_id: "axs_source".into(),
            checkpoint_id: "cp-1".into(),
            mode: TestMode::Historical,
            task: "Continue the unfinished feature.".into(),
            target_agent: "cursor".into(),
            spec_id: None,
            spec_version: None,
            spec_hash: None,
        };
        let prompt = continuity_prompt(&reference).unwrap();
        assert!(prompt.contains("ctr-1"));
        assert!(prompt.contains("cp-1"));
        assert!(prompt.contains("Spec: none"));
        assert!(!prompt.contains("secret historical answer"));
        assert!(!prompt.contains(TRANSCRIPT_MARK));
        assert!(continuity_prompt(&TestReference {
            task: format!("{TRANSCRIPT_MARK} pasted"),
            ..reference
        })
        .is_err());
    }

    #[test]
    fn prompt_carries_a_spec_pointer_and_not_the_body() {
        let prompt = continuity_prompt(&TestReference {
            id: "ctr-1".into(),
            source_session_id: "axs_source".into(),
            checkpoint_id: "cp-1".into(),
            mode: TestMode::Historical,
            task: "Continue the unfinished feature.".into(),
            target_agent: "cursor".into(),
            spec_id: Some("billing".into()),
            spec_version: Some(2),
            spec_hash: Some("abc".into()),
        })
        .unwrap();
        assert!(prompt.contains("Spec: billing@2 abc"));
        assert!(prompt.contains("Pointer: ax spec show billing@2"));
        assert!(!prompt.contains("Keep the first price."));
    }

    #[test]
    fn checkpoint_is_immutable_and_drops_the_transcript() {
        let items = items_for(Scenario::UnfinishedFeature);
        let seal = SealMeta {
            id: "cp-1".into(),
            commit: "abc".into(),
            cutoff: "2026-11-01T00:00:00Z".into(),
        };
        let checkpoint = seal_checkpoint(&items, &seal).unwrap();
        assert!(checkpoint
            .items()
            .iter()
            .all(|item| item.kind != ItemKind::Transcript));
        assert!(checkpoint
            .items()
            .iter()
            .all(|item| !item.text.contains(TRANSCRIPT_MARK)));
        let bytes = checkpoint.to_bytes();
        let mut tampered = bytes.clone();
        let last = tampered.len() - 2;
        tampered[last] ^= 0x1;
        assert!(Checkpoint::from_bytes(&tampered).is_err());
        assert_eq!(
            Checkpoint::from_bytes(&bytes).unwrap().hash(),
            checkpoint.hash()
        );
    }

    #[test]
    fn negative_control_drops_historical_and_indirect_paths() {
        let items = items_for(Scenario::UnfinishedFeature);
        let query = Query {
            arm: Arm::NegativeControl,
            mode: TestMode::Current,
            tags: vec!["work".into()],
            cutoff: "2026-11-01T00:00:00Z".into(),
            explain: false,
            now: "2026-11-20T00:00:00Z".into(),
        };
        let visible = select_context(&items, &query);
        assert!(visible.iter().all(|item| !item.historical));
        assert!(visible
            .iter()
            .all(|item| item.kind != ItemKind::GlobalMemory));
        assert!(visible.iter().any(|item| item.id == "readme-tool"));
    }

    #[test]
    fn each_gate_fails_on_its_own_counterexample() {
        let report = validate_continuity().unwrap();
        let scenario = report
            .scenarios
            .iter()
            .find(|item| item.scenario == Scenario::UnfinishedFeature)
            .unwrap();
        assert!(scenario.gates.is_empty());
        let world = build(Scenario::UnfinishedFeature).world;
        let trio = run_trio(&world, TestMode::Historical, "2026-11-01T00:00:00Z", false);
        assert!(
            gate_failures(&trio).is_empty(),
            "{:?}",
            gate_failures(&trio)
        );
        let mut shared = trio.clone();
        shared.b.session_id = shared.a.session_id.clone();
        assert!(gate_failures(&shared).contains(&"independent"));
        let mut pasted = trio.clone();
        pasted.b.delivered_text = TRANSCRIPT_MARK.into();
        assert!(gate_failures(&pasted).contains(&"transcript"));
        let mut missed = trio.clone();
        missed.b.delivered_ids.clear();
        missed.b.passed_tests.clear();
        missed.b.cited_ids.clear();
        let failed = gate_failures(&missed);
        assert!(
            failed.contains(&"retrieved") && failed.contains(&"task"),
            "{failed:?}"
        );
        let mut uncited = trio.clone();
        uncited.b.cited_ids.clear();
        uncited.decision_ids = vec!["decision-storage".into()];
        assert!(gate_failures(&uncited).contains(&"policy"));
        let mut leaked = trio.clone();
        leaked.c.delivered_ids.push("pending-feature".into());
        assert!(gate_failures(&leaked).contains(&"leakage"));
        let mut unsourced = trio.clone();
        unsourced.b.attributions[0].method.clear();
        assert!(gate_failures(&unsourced).contains(&"sources"));
        let mut drifted = trio.clone();
        drifted.repeat_ids.push("extra".into());
        assert!(gate_failures(&drifted).contains(&"reproducible"));
    }

    fn read_lf(path: &std::path::Path) -> String {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .replace("\r\n", "\n")
    }

    #[test]
    fn the_twelve_scenarios_pass_and_do_not_claim_equivalence() {
        let report = validate_continuity().expect("scenarios");
        assert_eq!(report.scenarios.len(), 12);
        assert!(!report.equivalence_claimed);
        assert_eq!(report.live_agent_experiments, 0);
        for scenario in &report.scenarios {
            assert!(
                scenario.score.fact_recall == 1.0,
                "{}",
                scenario.scenario.as_str()
            );
            assert!(
                scenario.score.fact_precision == 1.0,
                "{}",
                scenario.scenario.as_str()
            );
            assert!(scenario.score.outcome_equivalence == 1.0);
            assert!(scenario.score.negative_control_pass_rate == 0.0);
            assert!(scenario.score.historical_dependency);
            assert!(scenario.score.token_overhead < 1.0);
            assert!(scenario.score.acceptance_pass_rate == 1.0);
        }
        let repeated = &report.scenarios[0].score;
        assert!(repeated.confidence_low.is_some() && repeated.confidence_high.is_some());
        assert!(report.scenarios[1].score.confidence_low.is_none());
        assert!(same_mode(TestMode::Historical, TestMode::Current).is_err());
        let json = serde_json::to_string_pretty(&report).unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/specs/context-continuity");
        let results = root.join("CONTINUITY-RESULTS.json");
        let written = root.join("CONTINUITY-REPORT.md");
        if std::env::var_os("AX_WRITE_CONTINUITY").is_some() {
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(&results, format!("{json}\n")).unwrap();
            std::fs::write(&written, render_report(&report)).unwrap();
        }
        assert_eq!(read_lf(&results), format!("{json}\n"));
        assert_eq!(read_lf(&written), render_report(&report));
    }

    #[test]
    fn an_unrelated_item_received_by_ax_is_not_marked_used() {
        let world = build(Scenario::UnfinishedFeature).world;
        let trio = run_trio(&world, TestMode::Historical, "2026-11-01T00:00:00Z", true);
        let objective = trio
            .b
            .attributions
            .iter()
            .find(|item| item.item_id == "objective")
            .unwrap();
        assert!(objective.received);
        assert!(!objective.use_observed);
        let pending = trio
            .b
            .attributions
            .iter()
            .find(|item| item.item_id == "pending-feature")
            .unwrap();
        assert!(pending.received && pending.use_observed);
        assert!(trio.a.available_text.contains(TRANSCRIPT_MARK));
        assert!(!trio.b.available_text.contains(TRANSCRIPT_MARK));
        assert_ne!(trio.a.files, trio.b.files);
    }
}
