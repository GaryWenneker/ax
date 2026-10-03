//! Versioned Ax architecture seed.
//!
//! The engine applies a manifest. The catalog is the knowledge. Foundational
//! facts land in the existing domain overlay (`.ax/domain-graph.json`) and in
//! the existing rules and skills directories. This module does not start Pi
//! and does not copy a conversation.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const AX_SEED_VERSION: &str = "2.0.0";
const PREVIOUS_SEED_VERSION: &str = "1.0.0";
const MINIMUM_AX_VERSION: &str = "6.4.0";
const SEED_MARKER: &str = "seed: ax-bootstrap";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SeedManifest {
    pub version: String,
    pub minimum_ax_version: String,
    pub description: String,
    pub packages: Vec<SeedPackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SeedPackage {
    pub id: String,
    pub version: String,
    pub required: bool,
    pub dependencies: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BootstrapReport {
    pub seed_version: String,
    pub status: String,
    pub dry_run: bool,
    pub migration: Option<String>,
    pub entities_created: usize,
    pub relationships_created: usize,
    pub rules_created: usize,
    pub skills_created: usize,
    pub plan: String,
}

#[derive(Clone)]
struct Entity {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    package: &'static str,
}

#[derive(Clone)]
struct Edge {
    source: &'static str,
    kind: &'static str,
    target: &'static str,
    package: &'static str,
}

#[derive(Clone)]
struct RuleSeed {
    id: &'static str,
    level: &'static str,
    always_apply: bool,
    description: &'static str,
    body: &'static str,
    package: &'static str,
}

#[derive(Clone)]
struct SkillSeed {
    id: &'static str,
    description: &'static str,
    body: &'static str,
    package: &'static str,
}

#[derive(Debug, Serialize, Deserialize)]
struct DomainFile {
    #[serde(default = "domain_version")]
    version: u32,
    #[serde(default)]
    nodes: Vec<DomainNode>,
    #[serde(default)]
    edges: Vec<DomainEdge>,
}

fn domain_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct DomainNode {
    id: String,
    kind: String,
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    code_node_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct DomainEdge {
    source: String,
    target: String,
    kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    order: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct VersionFile {
    seed_version: String,
    #[serde(default)]
    migrations: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ProjectSeed {
    #[serde(default)]
    rules: Vec<ProjectRule>,
}

#[derive(Debug, Deserialize)]
struct ProjectRule {
    id: String,
    description: String,
}

pub fn manifest() -> SeedManifest {
    let packages = package_order()
        .into_iter()
        .map(|(id, deps)| SeedPackage {
            id: id.to_string(),
            version: AX_SEED_VERSION.to_string(),
            required: true,
            dependencies: deps.iter().map(|d| (*d).to_string()).collect(),
            checksum: Some(package_checksum(id)),
        })
        .collect();
    SeedManifest {
        version: AX_SEED_VERSION.to_string(),
        minimum_ax_version: MINIMUM_AX_VERSION.to_string(),
        description: "Ax and Pi responsibility model. Pi owns execution. Ax owns intelligence."
            .into(),
        packages,
    }
}

pub fn apply(project: &Path, dry_run: bool) -> Result<BootstrapReport, String> {
    let manifest = manifest();
    validate_manifest(&manifest)?;
    let order = sorted_packages(&manifest)?;
    let previous = read_version(project)?;
    let migration = migration_note(previous.as_ref())?;
    let project_rules = load_project_rules(project)?;
    let mut graph = read_graph(project)?;
    let mut entities_created = 0;
    let mut relationships_created = 0;
    let mut rules_created = 0;
    let mut skills_created = 0;
    let mut counts: BTreeMap<&str, (usize, usize, usize, usize)> = BTreeMap::new();

    for package in &order {
        let (e, r, ru, s) = plan_package(project, package, &mut graph, &project_rules, dry_run)?;
        entities_created += e;
        relationships_created += r;
        rules_created += ru;
        skills_created += s;
        counts.insert(package, (e, r, ru, s));
    }
    verify_graph(&graph)?;
    if !dry_run {
        write_graph(project, &graph)?;
        let mut version = previous.unwrap_or(VersionFile {
            seed_version: AX_SEED_VERSION.into(),
            migrations: Vec::new(),
        });
        if let Some(note) = &migration {
            version.migrations.push(note.clone());
        }
        version.seed_version = AX_SEED_VERSION.into();
        write_version(project, &version)?;
    }
    Ok(BootstrapReport {
        seed_version: AX_SEED_VERSION.into(),
        status: "success".into(),
        dry_run,
        migration,
        entities_created,
        relationships_created,
        rules_created,
        skills_created,
        plan: format_plan(
            &counts,
            entities_created,
            relationships_created,
            rules_created,
            skills_created,
        ),
    })
}

pub fn verify(project: &Path) -> Result<(), String> {
    let graph = read_graph(project)?;
    verify_graph(&graph)?;
    verify_required(&graph, project)
}

pub fn answer(project: &Path, question: &str) -> Result<String, String> {
    let graph = read_graph(project)?;
    let q = question.to_ascii_lowercase();
    if q.contains("agent execution") || q.contains("agent loop") {
        let owner = owner_of(&graph, "pi.agent-loop");
        return Ok(owner.unwrap_or_else(|| "unresolved".into()));
    }
    if q.contains("what does ax own") || q.contains("what ax own") {
        let owned = targets_of(&graph, "ax", "owns");
        return Ok(owned.join(", "));
    }
    if q.contains("duplicate") && q.contains("session") {
        let blocked = graph.edges.iter().any(|edge| {
            edge.source == "ax" && edge.kind == "must_not_duplicate" && edge.target == "pi.session"
        });
        return Ok(if blocked { "No." } else { "unresolved" }.into());
    }
    if q.contains("terminate") || q.contains("fail") {
        let rule = agents_rules(project).join("fail-open.mdc");
        let body = fs::read_to_string(rule).unwrap_or_default();
        return Ok(if body.contains("must not terminate Pi") {
            "No, under normal fail-open integration mode.".into()
        } else {
            "unresolved".into()
        });
    }
    Err(format!("no seeded answer for {question}"))
}

fn plan_package(
    project: &Path,
    package: &str,
    graph: &mut DomainFile,
    project_rules: &[ProjectRule],
    dry_run: bool,
) -> Result<(usize, usize, usize, usize), String> {
    let mut entity_count = 0;
    let mut relationship_count = 0;
    for entity in entities() {
        if entity.package != package {
            continue;
        }
        if graph.nodes.iter().any(|node| node.id == entity.id) {
            continue;
        }
        entity_count += 1;
        if !dry_run {
            graph.nodes.push(DomainNode {
                id: entity.id.into(),
                kind: "domain".into(),
                name: entity.name.into(),
                summary: Some(format!(
                    "{}. source=ax-core sourceVersion={MINIMUM_AX_VERSION} piVersion=undetected",
                    entity.description
                )),
                code_node_ids: Vec::new(),
            });
        }
    }
    for edge in edges() {
        if edge.package != package {
            continue;
        }
        if graph.edges.iter().any(|existing| {
            existing.source == edge.source
                && existing.kind == edge.kind
                && existing.target == edge.target
        }) {
            continue;
        }
        relationship_count += 1;
        if !dry_run {
            graph.edges.push(DomainEdge {
                source: edge.source.into(),
                target: edge.target.into(),
                kind: edge.kind.into(),
                order: None,
            });
        }
    }
    let mut rule_count = 0;
    for rule in rules() {
        if rule.package != package {
            continue;
        }
        if write_rule(project, &rule, dry_run)? {
            rule_count += 1;
        }
    }
    for rule in project_rules {
        if package != "rules" {
            continue;
        }
        if write_project_rule(project, rule, dry_run)? {
            rule_count += 1;
        }
    }
    let mut skill_count = 0;
    for skill in skills() {
        if skill.package != package {
            continue;
        }
        if write_skill(project, &skill, dry_run)? {
            skill_count += 1;
        }
    }
    Ok((entity_count, relationship_count, rule_count, skill_count))
}

fn write_rule(project: &Path, rule: &RuleSeed, dry_run: bool) -> Result<bool, String> {
    let path = agents_rules(project).join(format!("{}.mdc", rule.id));
    let body = render_rule(rule);
    write_if_changed(&path, &body, dry_run)
}

fn write_project_rule(project: &Path, rule: &ProjectRule, dry_run: bool) -> Result<bool, String> {
    let path = agents_rules(project).join(format!("{}.mdc", rule.id));
    let body = format!(
        "---\nid: {}\nlevel: INFO\nalwaysApply: false\n---\n\n{}\n\n{SEED_MARKER}\nseedVersion: {AX_SEED_VERSION}\n",
        rule.id, rule.description
    );
    write_if_changed(&path, &body, dry_run)
}

fn write_skill(project: &Path, skill: &SkillSeed, dry_run: bool) -> Result<bool, String> {
    let path = agents_skills(project).join(skill.id).join("SKILL.md");
    let body = format!(
        "---\nname: {}\ndescription: {}\n---\n\n{}\n\n{SEED_MARKER}\nseedVersion: {AX_SEED_VERSION}\n",
        skill.id, skill.description, skill.body
    );
    write_if_changed(&path, &body, dry_run)
}

fn write_if_changed(path: &Path, body: &str, dry_run: bool) -> Result<bool, String> {
    if path.is_file() {
        let current = fs::read_to_string(path).map_err(|err| err.to_string())?;
        if current == body || !current.contains(SEED_MARKER) {
            return Ok(false);
        }
    }
    if dry_run {
        return Ok(true);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, body).map_err(|err| err.to_string())?;
    fs::rename(&tmp, path).map_err(|err| err.to_string())?;
    Ok(true)
}

fn render_rule(rule: &RuleSeed) -> String {
    format!(
        "---\nid: {}\nlevel: {}\nalwaysApply: {}\ndescription: {}\n---\n\n{}\n\n{SEED_MARKER}\nseedVersion: {AX_SEED_VERSION}\n",
        rule.id, rule.level, rule.always_apply, rule.description, rule.body
    )
}

fn verify_graph(graph: &DomainFile) -> Result<(), String> {
    let mut ids = BTreeSet::new();
    for node in &graph.nodes {
        if node.id.trim().is_empty() || node.name.trim().is_empty() {
            return Err(format!("seed entity {} is missing an id or name", node.id));
        }
        let missing_summary = node.summary.as_deref().unwrap_or("").trim().is_empty();
        if missing_summary && (node.id.starts_with("ax.") || node.id.starts_with("pi")) {
            return Err(format!("seed entity {} has no description", node.id));
        }
        if !ids.insert(node.id.clone()) {
            return Err(format!("duplicate seed entity {}", node.id));
        }
    }
    for edge in &graph.edges {
        if !ids.contains(&edge.source) || !ids.contains(&edge.target) {
            return Err(format!(
                "seed relationship {} -{}-> {} is dangling",
                edge.source, edge.kind, edge.target
            ));
        }
    }
    Ok(())
}

fn verify_required(graph: &DomainFile, project: &Path) -> Result<(), String> {
    for id in [
        "ax",
        "pi",
        "ax.intelligence",
        "ax.context",
        "ax.graph",
        "ax.memory",
        "ax.tool-economics",
        "ax.optimization",
    ] {
        if !graph.nodes.iter().any(|node| node.id == id) {
            return Err(format!("required seed entity {id} is missing"));
        }
    }
    for (source, kind, target) in [
        ("pi", "owns", "pi.agent-loop"),
        ("pi", "owns", "pi.session"),
        ("ax", "owns", "ax.context"),
        ("ax", "owns", "ax.graph"),
        ("ax", "owns", "ax.memory"),
    ] {
        if !graph
            .edges
            .iter()
            .any(|edge| edge.source == source && edge.kind == kind && edge.target == target)
        {
            return Err(format!(
                "required relationship {source} {kind} {target} is missing"
            ));
        }
    }
    for (id, needle) in [
        ("pi-owns-runtime", "Pi is authoritative for agent execution"),
        ("no-duplicate-persistence", "Do not create an Ax copy"),
        ("fail-open", "must not terminate Pi"),
        (
            "measure-before-optimize",
            "Do not automatically replace a tool",
        ),
    ] {
        let path = agents_rules(project).join(format!("{id}.mdc"));
        let body = fs::read_to_string(&path).unwrap_or_default();
        if !body.contains(needle) {
            return Err(format!("required rule {id} is missing"));
        }
    }
    for id in [
        "pi-runtime-integration",
        "ax-context-optimization",
        "tool-economics",
    ] {
        let path = agents_skills(project).join(id).join("SKILL.md");
        let body = fs::read_to_string(&path).unwrap_or_default();
        if !body.contains("purpose:") {
            return Err(format!("required skill {id} is missing"));
        }
    }
    Ok(())
}

fn owner_of(graph: &DomainFile, target: &str) -> Option<String> {
    graph.edges.iter().find_map(|edge| {
        if edge.kind == "owns" && edge.target == target {
            graph
                .nodes
                .iter()
                .find(|node| node.id == edge.source)
                .map(|node| node.name.clone())
        } else {
            None
        }
    })
}

fn targets_of(graph: &DomainFile, source: &str, kind: &str) -> Vec<String> {
    graph
        .edges
        .iter()
        .filter(|edge| edge.source == source && edge.kind == kind)
        .filter_map(|edge| {
            graph
                .nodes
                .iter()
                .find(|node| node.id == edge.target)
                .map(|node| node.name.clone())
        })
        .collect()
}

fn load_project_rules(project: &Path) -> Result<Vec<ProjectRule>, String> {
    let path = project.join(".ax").join("seed").join("project.json");
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(&path).map_err(|err| err.to_string())?;
    let seed: ProjectSeed =
        serde_json::from_str(&raw).map_err(|err| format!("invalid project seed: {err}"))?;
    let foundational: BTreeSet<&str> = rules().iter().map(|rule| rule.id).collect();
    for rule in &seed.rules {
        if foundational.contains(rule.id.as_str()) {
            return Err(format!(
                "project seed cannot replace foundational rule {}",
                rule.id
            ));
        }
        let text = rule.description.to_ascii_lowercase();
        if text.contains("ax owns pi") || text.contains("ax owns the agent loop") {
            return Err("project seed cannot move Pi runtime ownership to Ax".into());
        }
    }
    Ok(seed.rules)
}

pub fn validate_manifest(manifest: &SeedManifest) -> Result<(), String> {
    if manifest.version.trim().is_empty() {
        return Err("seed manifest version is empty".into());
    }
    if manifest.packages.is_empty() {
        return Err("seed manifest has no packages".into());
    }
    let ids: BTreeSet<&str> = manifest
        .packages
        .iter()
        .map(|pkg| pkg.id.as_str())
        .collect();
    if ids.len() != manifest.packages.len() {
        return Err("seed manifest has a duplicate package id".into());
    }
    for package in &manifest.packages {
        for dep in &package.dependencies {
            if !ids.contains(dep.as_str()) {
                return Err(format!(
                    "seed package {} depends on missing package {dep}",
                    package.id
                ));
            }
        }
    }
    sorted_packages(manifest)?;
    for entity in entities() {
        if entity.id.is_empty() || entity.description.is_empty() || entity.name.is_empty() {
            return Err(format!("corrupted seed entity {}", entity.id));
        }
        if !ids.contains(entity.package) {
            return Err(format!(
                "seed entity {} is in missing package {}",
                entity.id, entity.package
            ));
        }
    }
    Ok(())
}

fn sorted_packages(manifest: &SeedManifest) -> Result<Vec<String>, String> {
    let mut pending: Vec<&SeedPackage> = manifest.packages.iter().collect();
    let mut done = BTreeSet::new();
    let mut order = Vec::new();
    while !pending.is_empty() {
        let ready = pending.iter().position(|pkg| {
            pkg.dependencies
                .iter()
                .all(|dep| done.contains(dep.as_str()))
        });
        let Some(index) = ready else {
            return Err("seed package dependencies contain a cycle".into());
        };
        let pkg = pending.remove(index);
        done.insert(pkg.id.clone());
        order.push(pkg.id.clone());
    }
    Ok(order)
}

fn migration_note(previous: Option<&VersionFile>) -> Result<Option<String>, String> {
    let Some(previous) = previous else {
        return Ok(None);
    };
    if previous.seed_version == AX_SEED_VERSION {
        return Ok(None);
    }
    if previous.seed_version == PREVIOUS_SEED_VERSION {
        return Ok(Some(format!(
            "{PREVIOUS_SEED_VERSION} -> {AX_SEED_VERSION}"
        )));
    }
    Err(format!(
        "unsupported seed version {} (supported: {PREVIOUS_SEED_VERSION}, {AX_SEED_VERSION})",
        previous.seed_version
    ))
}

fn read_version(project: &Path) -> Result<Option<VersionFile>, String> {
    let path = version_path(project);
    if !path.is_file() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path).map_err(|err| err.to_string())?;
    serde_json::from_str(&raw)
        .map(Some)
        .map_err(|err| format!("invalid seed version: {err}"))
}

fn write_version(project: &Path, version: &VersionFile) -> Result<(), String> {
    let path = version_path(project);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let raw = serde_json::to_string_pretty(version).map_err(|err| err.to_string())?;
    fs::write(path, raw).map_err(|err| err.to_string())
}

fn read_graph(project: &Path) -> Result<DomainFile, String> {
    let path = graph_path(project);
    if !path.is_file() {
        return Ok(DomainFile {
            version: 1,
            nodes: Vec::new(),
            edges: Vec::new(),
        });
    }
    let raw = fs::read_to_string(path).map_err(|err| err.to_string())?;
    serde_json::from_str(&raw).map_err(|err| format!("invalid domain graph: {err}"))
}

fn write_graph(project: &Path, graph: &DomainFile) -> Result<(), String> {
    let path = graph_path(project);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let raw = serde_json::to_string_pretty(graph).map_err(|err| err.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, raw).map_err(|err| err.to_string())?;
    fs::rename(&tmp, path).map_err(|err| err.to_string())
}

fn version_path(project: &Path) -> PathBuf {
    project.join(".ax").join("bootstrap-version.json")
}

fn graph_path(project: &Path) -> PathBuf {
    project.join(".ax").join("domain-graph.json")
}

fn agents_rules(project: &Path) -> PathBuf {
    crate::agents_share::agents_dir(project).join("rules")
}

fn agents_skills(project: &Path) -> PathBuf {
    crate::agents_share::agents_dir(project).join("skills")
}

fn format_plan(
    counts: &BTreeMap<&str, (usize, usize, usize, usize)>,
    entities: usize,
    relationships: usize,
    rules: usize,
    skills: usize,
) -> String {
    let mut grouped: BTreeMap<&str, (usize, usize, usize, usize)> = BTreeMap::new();
    for (package, count) in counts {
        let section = match *package {
            "core" => "Core",
            "architecture" | "runtime" => "Runtime",
            "knowledge" | "context" | "memory" => "Context",
            "tools" | "economics" => "Economics",
            "optimization" => "Optimization",
            _ => "Rules",
        };
        let slot = grouped.entry(section).or_insert((0, 0, 0, 0));
        slot.0 += count.0;
        slot.1 += count.1;
        slot.2 += count.2;
        slot.3 += count.3;
    }
    let mut lines = vec!["SEED PLAN".into(), String::new()];
    for section in [
        "Core",
        "Runtime",
        "Context",
        "Rules",
        "Economics",
        "Optimization",
    ] {
        let (entities, relationships, rules, skills) =
            grouped.get(section).copied().unwrap_or((0, 0, 0, 0));
        lines.push(format!(
            "{section}:\n  {entities} entities\n  {relationships} relationships\n  {rules} rules\n  {skills} skills"
        ));
        lines.push(String::new());
    }
    lines.push("Changes:".into());
    lines.push(format!("  +{entities} entities"));
    lines.push(format!("  +{relationships} relationships"));
    lines.push(format!("  +{rules} rules"));
    lines.push(format!("  +{skills} skills"));
    lines.join("\n")
}

fn package_checksum(id: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for entity in entities() {
        if entity.package == id {
            for byte in entity.id.bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x100000001b3);
            }
        }
    }
    format!("{hash:016x}")
}

fn package_order() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("core", vec![]),
        ("architecture", vec!["core"]),
        ("runtime", vec!["architecture"]),
        ("knowledge", vec!["runtime"]),
        ("context", vec!["knowledge"]),
        ("memory", vec!["context"]),
        ("rules", vec!["memory"]),
        ("skills", vec!["rules"]),
        ("tools", vec!["skills"]),
        ("economics", vec!["tools"]),
        ("optimization", vec!["economics"]),
    ]
}

fn ent(
    id: &'static str,
    name: &'static str,
    description: &'static str,
    package: &'static str,
) -> Entity {
    Entity {
        id,
        name,
        description,
        package,
    }
}

fn edge(
    source: &'static str,
    kind: &'static str,
    target: &'static str,
    package: &'static str,
) -> Edge {
    Edge {
        source,
        kind,
        target,
        package,
    }
}

fn entities() -> Vec<Entity> {
    vec![
        ent(
            "ax",
            "Ax",
            "Ax is the intelligence layer around a coding agent",
            "core",
        ),
        ent(
            "ax.runtime",
            "Ax Runtime",
            "Ax runtime adapter boundary, not an agent loop",
            "core",
        ),
        ent(
            "ax.intelligence",
            "Ax Intelligence",
            "Repository understanding, context, and policy",
            "core",
        ),
        ent(
            "ax.graph",
            "Ax Knowledge Graph",
            "Indexed source and architecture facts",
            "core",
        ),
        ent(
            "ax.memory",
            "Ax Memory",
            "Retrieved memories relevant to the current request",
            "core",
        ),
        ent(
            "ax.context",
            "Ax Context Engine",
            "Selects knowledge for one turn inside a token budget",
            "core",
        ),
        ent(
            "ax.rules",
            "Ax Rules",
            "Policy rules, including always-apply constraints",
            "core",
        ),
        ent(
            "ax.skills",
            "Ax Skills",
            "Task skills selected for the current request",
            "core",
        ),
        ent(
            "ax.mcp",
            "Ax MCP",
            "Existing MCP server that exposes Ax tools",
            "core",
        ),
        ent(
            "ax.tool-economics",
            "Ax Tool Economics",
            "Measures tool output that may enter model context",
            "core",
        ),
        ent(
            "ax.context-cache",
            "Ax Context Cache",
            "Caches selected context until its inputs change",
            "core",
        ),
        ent(
            "ax.optimization",
            "Ax Optimization",
            "Advisory and approved deterministic optimizations",
            "core",
        ),
        ent(
            "ax.cost",
            "Ax Cost Engine",
            "Attributes model and context cost to a session and turn",
            "core",
        ),
        ent(
            "ax.budget",
            "Ax Budget Engine",
            "Budget concept with period, amount, and currency",
            "core",
        ),
        ent(
            "pi",
            "Pi",
            "Pi agent runtime. Ax does not replace it",
            "architecture",
        ),
        ent(
            "pi.runtime",
            "Pi Agent Runtime",
            "Pi owns the agent loop",
            "architecture",
        ),
        ent(
            "pi.session",
            "Pi Session",
            "Pi owns session and conversation state",
            "architecture",
        ),
        ent(
            "pi.agent",
            "Pi Agent",
            "The running Pi agent",
            "architecture",
        ),
        ent(
            "pi.tool-execution",
            "Pi Tool Execution",
            "Pi executes tools",
            "architecture",
        ),
        ent(
            "pi.model-provider",
            "Pi Model Provider",
            "Pi calls the model provider",
            "architecture",
        ),
        ent(
            "pi.telemetry",
            "Pi Telemetry",
            "Pi emits diagnostic events without prompts by default",
            "architecture",
        ),
        ent(
            "pi.durable-state",
            "Pi Durable State",
            "Pi stores durable session state",
            "architecture",
        ),
        ent(
            "pi.coding-agent",
            "Pi Coding Agent",
            "Pi coding-agent SDK is the host, not an Ax fork",
            "architecture",
        ),
        ent(
            "pi.agent-loop",
            "agent execution",
            "The agent execution loop",
            "runtime",
        ),
        ent("user", "User", "The person who started the turn", "runtime"),
        ent(
            "pi.event",
            "Pi Event",
            "A Pi telemetry or agent event",
            "runtime",
        ),
        ent(
            "ax.adapter",
            "Ax Runtime Adapter",
            "Translates Pi events into Ax agent events",
            "runtime",
        ),
        ent(
            "pi.adapter",
            "Pi Adapter",
            "Public-API adapter. It does not patch Pi",
            "runtime",
        ),
        ent(
            "ax.agent-event",
            "Ax Agent Event",
            "Normalized event. It does not store prompts",
            "runtime",
        ),
        ent(
            "intent",
            "Intent",
            "The current user or agent request",
            "context",
        ),
        ent(
            "context.budget",
            "Context Budget",
            "Maximum tokens of Ax context for one turn",
            "context",
        ),
        ent(
            "pi.model-context",
            "Pi Model Context",
            "Context Pi sends to a model. Ax does not persist it",
            "context",
        ),
        ent(
            "repository",
            "Repository",
            "The project under analysis",
            "knowledge",
        ),
        ent("module", "Module", "A repository module", "knowledge"),
        ent("symbol", "Symbol", "A source symbol", "knowledge"),
        ent(
            "source-file",
            "Source File",
            "A source file in the repository",
            "knowledge",
        ),
        ent(
            "tool.search",
            "search",
            "Search can emit large output and may have a graph alternative",
            "tools",
        ),
        ent(
            "tool.read",
            "read",
            "A whole-file read may have an indexed-source alternative",
            "tools",
        ),
        ent(
            "tool.write",
            "write",
            "Write changes repository state",
            "tools",
        ),
        ent(
            "tool.edit",
            "edit",
            "Edit changes repository state",
            "tools",
        ),
        ent(
            "tool.shell",
            "shell",
            "Shell output size depends on the command",
            "tools",
        ),
        ent(
            "tool.git",
            "git",
            "Git output is usually smaller than a repository search",
            "tools",
        ),
        ent(
            "tool.network",
            "network",
            "Network tools are not assumed cacheable",
            "tools",
        ),
        ent(
            "tool.build",
            "build",
            "Build output is command-specific",
            "tools",
        ),
        ent(
            "tool.test",
            "test",
            "Test output is command-specific",
            "tools",
        ),
        ent(
            "tool.execution",
            "Tool Execution",
            "Running a tool is not itself a model token cost",
            "economics",
        ),
        ent(
            "tool.output",
            "Tool Output",
            "Bytes a tool produced",
            "economics",
        ),
        ent(
            "model.context",
            "Model Context",
            "Tokens actually delivered to a model",
            "economics",
        ),
        ent(
            "token.cost",
            "Token Cost",
            "Cost of tokens a model call consumed",
            "economics",
        ),
        ent(
            "repeated.call",
            "Repeated Tool Call",
            "Same tool, arguments, and repository state",
            "economics",
        ),
        ent(
            "cache.candidate",
            "Cache Candidate",
            "A repeat may be a cache candidate. Output is not cached automatically",
            "economics",
        ),
        ent(
            "optimization.opportunity",
            "Optimization Opportunity",
            "An estimate, not an automatic replacement",
            "optimization",
        ),
        ent(
            "mode.observe",
            "observe",
            "Collect measurements. This is the default mode",
            "optimization",
        ),
        ent(
            "mode.advise",
            "advise",
            "Collect measurements and expose recommendations",
            "optimization",
        ),
        ent(
            "mode.optimize",
            "optimize",
            "Run only approved deterministic optimizations",
            "optimization",
        ),
        ent(
            "budget",
            "Budget",
            "A budget has a period, an amount, and a currency",
            "economics",
        ),
        ent(
            "provider",
            "Provider",
            "Model provider named on a call",
            "economics",
        ),
        ent("model", "Model", "Model named on a call", "economics"),
        ent(
            "budget.period",
            "period",
            "Budget period, supplied by project settings",
            "economics",
        ),
        ent(
            "budget.amount",
            "amount",
            "Budget amount, supplied by project settings",
            "economics",
        ),
        ent(
            "budget.currency",
            "currency",
            "Budget currency, supplied by project settings",
            "economics",
        ),
        ent(
            "turn",
            "Turn",
            "One agent turn inside a Pi session",
            "context",
        ),
        ent(
            "entity.resolution",
            "Entity Resolution",
            "Resolve names in the request to graph entities",
            "context",
        ),
        ent(
            "source.selection",
            "Source Selection",
            "Choose source that the turn actually needs",
            "context",
        ),
        ent(
            "context.ranking",
            "Context Ranking",
            "Order selected context by the priority rules",
            "context",
        ),
        ent(
            "fact.repository",
            "Repository facts",
            "Facts that come from the repository",
            "knowledge",
        ),
        ent(
            "fact.architecture",
            "Architecture facts",
            "Facts about Ax and Pi responsibilities",
            "knowledge",
        ),
        ent(
            "fact.runtime",
            "Runtime facts",
            "Facts about a running agent session",
            "knowledge",
        ),
        ent(
            "fact.agent",
            "Agent facts",
            "Facts about the agent, not a second runtime",
            "knowledge",
        ),
        ent(
            "fact.tool",
            "Tool facts",
            "Facts about tool categories, not one provider",
            "knowledge",
        ),
        ent(
            "fact.policy",
            "Policy facts",
            "Facts from rules and skills",
            "knowledge",
        ),
        ent(
            "fact.optimization",
            "Optimization facts",
            "Facts about advice, not automatic replacement",
            "knowledge",
        ),
        ent(
            "event.session.start",
            "session.start",
            "A Pi session started",
            "runtime",
        ),
        ent(
            "event.session.end",
            "session.end",
            "A Pi session ended",
            "runtime",
        ),
        ent(
            "event.turn.start",
            "turn.start",
            "A turn started",
            "runtime",
        ),
        ent("event.turn.end", "turn.end", "A turn ended", "runtime"),
        ent(
            "event.model.request",
            "model.request",
            "Pi requested a model call",
            "runtime",
        ),
        ent(
            "event.model.response",
            "model.response",
            "Pi received a model response",
            "runtime",
        ),
        ent(
            "event.tool.start",
            "tool.start",
            "Pi started a tool",
            "runtime",
        ),
        ent(
            "event.tool.output",
            "tool.output",
            "Pi produced tool output",
            "runtime",
        ),
        ent(
            "event.tool.end",
            "tool.end",
            "Pi finished a tool",
            "runtime",
        ),
        ent(
            "event.compaction",
            "compaction",
            "Pi compacted its own context",
            "runtime",
        ),
        ent(
            "event.session.write",
            "session.write",
            "Pi wrote session state",
            "runtime",
        ),
    ]
}

fn edges() -> Vec<Edge> {
    vec![
        edge("ax", "contains", "ax.intelligence", "core"),
        edge("ax", "contains", "ax.graph", "core"),
        edge("ax", "contains", "ax.memory", "core"),
        edge("ax", "contains", "ax.context", "core"),
        edge("ax", "contains", "ax.tool-economics", "core"),
        edge("ax", "contains", "ax.optimization", "core"),
        edge("ax", "contains", "ax.cost", "core"),
        edge("ax", "contains", "ax.budget", "core"),
        edge("ax", "integrates", "pi", "architecture"),
        edge("pi", "provides", "pi.runtime", "architecture"),
        edge("pi", "provides", "pi.session", "architecture"),
        edge("pi", "provides", "pi.tool-execution", "architecture"),
        edge("pi", "provides", "pi.telemetry", "architecture"),
        edge("pi", "provides", "pi.durable-state", "architecture"),
        edge("pi", "owns", "pi.agent-loop", "runtime"),
        edge("pi", "owns", "pi.session", "runtime"),
        edge("pi", "owns", "pi.model-provider", "runtime"),
        edge("pi", "owns", "pi.tool-execution", "runtime"),
        edge("pi", "owns", "pi.durable-state", "runtime"),
        edge("ax", "owns", "ax.graph", "runtime"),
        edge("ax", "owns", "ax.memory", "runtime"),
        edge("ax", "owns", "ax.rules", "runtime"),
        edge("ax", "owns", "ax.skills", "runtime"),
        edge("ax", "owns", "ax.context", "runtime"),
        edge("ax", "owns", "ax.tool-economics", "runtime"),
        edge("ax", "owns", "ax.context-cache", "runtime"),
        edge("ax", "owns", "ax.optimization", "runtime"),
        edge("ax", "owns", "ax.cost", "runtime"),
        edge("ax", "must_not_own", "pi.agent-loop", "runtime"),
        edge("ax", "must_not_duplicate", "pi.session", "runtime"),
        edge("ax", "must_not_fork", "pi.runtime", "runtime"),
        edge("user", "consumed_by", "pi.agent", "runtime"),
        edge("pi", "emits", "pi.event", "runtime"),
        edge("pi.event", "consumed_by", "pi.adapter", "runtime"),
        edge("pi.adapter", "produces", "ax.agent-event", "runtime"),
        edge(
            "ax.agent-event",
            "consumed_by",
            "ax.intelligence",
            "runtime",
        ),
        edge("ax.context", "depends_on", "intent", "context"),
        edge("ax.context", "depends_on", "context.budget", "context"),
        edge("ax.context", "produces", "pi.model-context", "context"),
        edge("source-file", "defines", "symbol", "knowledge"),
        edge("symbol", "references", "symbol", "knowledge"),
        edge("symbol", "belongs_to", "module", "knowledge"),
        edge("module", "belongs_to", "repository", "knowledge"),
        edge("tool.execution", "produces", "tool.output", "economics"),
        edge("tool.output", "may_become", "model.context", "economics"),
        edge("model.context", "creates", "token.cost", "economics"),
        edge("model", "consumes", "token.cost", "economics"),
        edge("pi.session", "accumulates", "token.cost", "economics"),
        edge("budget", "limits", "token.cost", "economics"),
        edge("repeated.call", "may_be", "cache.candidate", "optimization"),
        edge("tool.search", "proposes", "ax.graph", "optimization"),
        edge("tool.read", "proposes", "ax.graph", "optimization"),
        edge(
            "ax.context-cache",
            "depends_on",
            "repository",
            "optimization",
        ),
        edge("ax.context-cache", "depends_on", "ax.graph", "optimization"),
        edge("ax.context-cache", "depends_on", "ax.rules", "optimization"),
        edge(
            "ax.context-cache",
            "depends_on",
            "ax.skills",
            "optimization",
        ),
        edge(
            "mode.observe",
            "produces",
            "ax.tool-economics",
            "optimization",
        ),
        edge(
            "mode.advise",
            "produces",
            "optimization.opportunity",
            "optimization",
        ),
        edge(
            "mode.optimize",
            "depends_on",
            "optimization.opportunity",
            "optimization",
        ),
        edge("pi.agent", "produces", "pi.tool-execution", "runtime"),
        edge("pi.tool-execution", "produces", "pi.event", "runtime"),
        edge("pi.model-provider", "produces", "pi.event", "runtime"),
        edge("ax.adapter", "produces", "ax.agent-event", "runtime"),
        edge("turn", "produces", "intent", "context"),
        edge("intent", "produces", "entity.resolution", "context"),
        edge("entity.resolution", "produces", "ax.graph", "context"),
        edge("source.selection", "produces", "context.ranking", "context"),
        edge("context.ranking", "produces", "context.budget", "context"),
        edge("context.budget", "produces", "pi.model-context", "context"),
        edge("budget", "has", "budget.period", "economics"),
        edge("budget", "has", "budget.amount", "economics"),
        edge("budget", "has", "budget.currency", "economics"),
        edge("ax.graph", "contains", "fact.repository", "knowledge"),
        edge("ax.graph", "contains", "fact.architecture", "knowledge"),
        edge("ax.graph", "contains", "fact.runtime", "knowledge"),
        edge("ax.graph", "contains", "fact.agent", "knowledge"),
        edge("ax.graph", "contains", "fact.tool", "knowledge"),
        edge("ax.graph", "contains", "fact.policy", "knowledge"),
        edge("ax.graph", "contains", "fact.optimization", "knowledge"),
        edge("pi.event", "has", "event.session.start", "runtime"),
        edge("pi.event", "has", "event.session.end", "runtime"),
        edge("pi.event", "has", "event.turn.start", "runtime"),
        edge("pi.event", "has", "event.turn.end", "runtime"),
        edge("pi.event", "has", "event.model.request", "runtime"),
        edge("pi.event", "has", "event.model.response", "runtime"),
        edge("pi.event", "has", "event.tool.start", "runtime"),
        edge("pi.event", "has", "event.tool.output", "runtime"),
        edge("pi.event", "has", "event.tool.end", "runtime"),
        edge("pi.event", "has", "event.compaction", "runtime"),
        edge("pi.event", "has", "event.session.write", "runtime"),
    ]
}

fn rules() -> Vec<RuleSeed> {
    vec![
        RuleSeed {
            id: "pi-owns-runtime",
            level: "CRITICAL",
            always_apply: true,
            description: "Pi is authoritative for agent execution.",
            body: "Pi is authoritative for agent execution. Ax must not start a second agent loop.",
            package: "rules",
        },
        RuleSeed {
            id: "ax-owns-intelligence",
            level: "CRITICAL",
            always_apply: true,
            description: "Ax provides contextual intelligence around the runtime.",
            body: "Ax provides contextual intelligence around the runtime: graph, memory, rules, skills, context, and cost.",
            package: "rules",
        },
        RuleSeed {
            id: "no-duplicate-persistence",
            level: "CRITICAL",
            always_apply: true,
            description: "Do not create an Ax copy of the canonical Pi conversation.",
            body: "Do not create an Ax copy of the canonical Pi conversation.",
            package: "rules",
        },
        RuleSeed {
            id: "fail-open",
            level: "CRITICAL",
            always_apply: true,
            description: "Ax integration failure must not terminate Pi execution.",
            body: "Ax integration failure must not terminate Pi execution unless a future strict mode is explicitly enabled.",
            package: "rules",
        },
        RuleSeed {
            id: "measure-before-optimize",
            level: "WARNING",
            always_apply: false,
            description: "Do not automatically replace a tool until measurements exist.",
            body: "Do not automatically replace a tool until sufficient measurements exist. A recommendation is not a replacement.",
            package: "optimization",
        },
        RuleSeed {
            id: "preserve-existing-behavior",
            level: "WARNING",
            always_apply: false,
            description: "Optimization stays off until it is explicitly enabled.",
            body: "New optimization features must remain backward compatible unless explicitly enabled. The default mode is observe.",
            package: "optimization",
        },
        RuleSeed {
            id: "context-priorities",
            level: "INFO",
            always_apply: false,
            description: "Context selection order.",
            body: "Context priority: 1 mandatory rules, 2 explicit user or project constraints, 3 required skills, 4 directly relevant graph entities, 5 relevant memory, 6 relevant source, 7 previous tool results, 8 background information. Do not drop mandatory rules to meet the budget.",
            package: "context",
        },
        RuleSeed {
            id: "memory-retrieval",
            level: "INFO",
            always_apply: false,
            description: "Retrieve relevant memories only.",
            body: "Memory belongs to Ax. Pi owns conversation state. Retrieve only relevant high-confidence memories that fit the context budget. Do not inject the entire memory store and do not copy the Pi conversation into memory.",
            package: "memory",
        },
        RuleSeed {
            id: "search-optimization",
            level: "INFO",
            always_apply: false,
            description: "Known-entity search may propose ax_explore.",
            body: "If a search targets a known graph entity and the entity resolves confidently, ax_explore may be proposed. Do not always replace search with ax_explore.",
            package: "optimization",
        },
        RuleSeed {
            id: "read-optimization",
            level: "INFO",
            always_apply: false,
            description: "Indexed source may propose graph retrieval.",
            body: "If requested source is indexed and only structural information is required, graph-backed retrieval may be proposed. A recommendation is not an automatic replacement.",
            package: "optimization",
        },
        RuleSeed {
            id: "cache-invalidation",
            level: "INFO",
            always_apply: false,
            description: "Invalidate only the context a change affects.",
            body: "If source affecting context changes, invalidate that context. If unrelated source changes, retain the rest. Policy, skill, and graph-version changes invalidate only the context that depends on them.",
            package: "optimization",
        },
        RuleSeed {
            id: "tool-output-cost",
            level: "INFO",
            always_apply: false,
            description: "Large tool output can increase model context cost.",
            body: "Large search output can increase model context cost when that output is delivered to a model. The tool process itself is not a model token cost. Do not record grep as universally expensive.",
            package: "economics",
        },
    ]
}

fn skills() -> Vec<SkillSeed> {
    vec![
        SkillSeed {
            id: "pi-runtime-integration",
            description: "Use when a change touches how Ax observes a Pi run.",
            body: "purpose: keep Pi as the execution runtime and Ax as the observer.\nwhen_to_use: Pi events, sessions, or tool lifecycle cross into Ax.\ninputs: Pi public events and the project root.\noutputs: normalized Ax events and derived metrics.\nconstraints: do not fork Pi, do not copy the transcript, fail open.\nrelated_entities: pi, ax.adapter, ax.agent-event.\nrelated_tools: ax agent economics.",
            package: "skills",
        },
        SkillSeed {
            id: "ax-context-optimization",
            description: "Use when selecting Ax context for a turn.",
            body: "purpose: supply relevant graph, memory, rules, and skills inside the context budget.\nwhen_to_use: a turn needs repository knowledge.\ninputs: intent, changed files, budget.\noutputs: ordered context sections.\nconstraints: do not scan the whole repository and do not drop mandatory rules.\nrelated_entities: ax.context, ax.graph, ax.memory.\nrelated_tools: ax_context, ax_explore.",
            package: "skills",
        },
        SkillSeed {
            id: "tool-economics",
            description: "Use when attributing tool output to context cost.",
            body: "purpose: measure output tokens and separate them from model cost.\nwhen_to_use: a tool result may be delivered to a model.\ninputs: tool name, output size, session, turn.\noutputs: an economics record and any advisory alternative.\nconstraints: do not price tool runtime as model tokens.\nrelated_entities: ax.tool-economics, tool.output, token.cost.\nrelated_tools: ax agent economics.",
            package: "skills",
        },
        SkillSeed {
            id: "context-cache-management",
            description: "Use when context cache keys or invalidation change.",
            body: "purpose: reuse context until project, git, intent, graph, policy, or skill inputs change.\nwhen_to_use: a cache hit or invalidation bug.\ninputs: cache key parts.\noutputs: hit or invalidated entries.\nconstraints: do not invalidate the entire cache for an unrelated file.\nrelated_entities: ax.context-cache, repository, ax.graph.\nrelated_tools: ax_context.",
            package: "skills",
        },
        SkillSeed {
            id: "agent-cost-analysis",
            description: "Use when explaining session or budget cost.",
            body: "purpose: attribute cost to session, turn, model, and tool-induced context.\nwhen_to_use: a cost or budget question.\ninputs: persisted usage and budget settings.\noutputs: spent, remaining, and projected figures.\nconstraints: do not invent a euro amount without usdPerEur. Do not change the model automatically.\nrelated_entities: ax.cost, ax.budget, model.\nrelated_tools: ax agent economics, ax budget.",
            package: "skills",
        },
        SkillSeed {
            id: "ax-pi-debugging",
            description: "Use when Ax observation fails and Pi must keep running.",
            body: "purpose: diagnose AX_PI_INTEGRATION_ERROR without stopping Pi.\nwhen_to_use: economics, context, or telemetry recording failed.\ninputs: the error line and whether strict mode is off.\noutputs: a degraded Ax result and a continuing Pi run.\nconstraints: fail open unless strict mode was explicitly enabled.\nrelated_entities: pi, ax.adapter.\nrelated_tools: ax agent economics.",
            package: "skills",
        },
        SkillSeed {
            id: "ax-agent-bootstrap",
            description: "Use when starting work in a repository that has Ax seed data.",
            body: "purpose: initialize understanding of Ax architecture, runtime integration, rules, skills, and optimization without scanning the whole repository.\nwhen_to_use: the first turn in a project, or when architecture ownership is unclear.\ninputs: repository root and the user request.\noutputs: a minimal context for that request.\nconstraints: inspect the repository structure, load architecture and runtime ownership, resolve the current project, resolve relevant graph entities, load applicable rules and skills, then begin the task. Do not scan the entire repository on every task.\nrelated_entities: ax, pi, ax.context.\nrelated_tools: ax bootstrap --verify, ax_context, ax_explore.",
            package: "skills",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("ax-bootstrap-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn fresh_install_seeds_required_facts() {
        let dir = project("fresh");
        let report = apply(&dir, false).unwrap();
        assert_eq!(report.seed_version, "2.0.0");
        assert!(report.entities_created > 0);
        verify(&dir).unwrap();
        assert_eq!(
            answer(&dir, "What owns the agent execution loop?").unwrap(),
            "Pi"
        );
        let owned = answer(&dir, "What does Ax own?").unwrap();
        for concept in [
            "Knowledge Graph",
            "Memory",
            "Context",
            "Rules",
            "Skills",
            "Optimization",
            "Cost",
        ] {
            assert!(owned.contains(concept), "{owned}");
        }
        assert_eq!(
            answer(&dir, "Should Ax duplicate Pi session persistence?").unwrap(),
            "No."
        );
        assert!(answer(&dir, "Can Ax failure terminate Pi?")
            .unwrap()
            .contains("No"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn existing_user_node_is_kept_and_seed_is_not_duplicated() {
        let dir = project("existing");
        let graph = DomainFile {
            version: 1,
            nodes: vec![DomainNode {
                id: "billing".into(),
                kind: "domain".into(),
                name: "Billing".into(),
                summary: Some("user domain".into()),
                code_node_ids: vec!["fn:bill".into()],
            }],
            edges: Vec::new(),
        };
        write_graph(&dir, &graph).unwrap();
        apply(&dir, false).unwrap();
        let again = apply(&dir, false).unwrap();
        assert_eq!(again.entities_created, 0);
        assert_eq!(again.relationships_created, 0);
        assert_eq!(again.rules_created, 0);
        assert_eq!(again.skills_created, 0);
        let stored = read_graph(&dir).unwrap();
        assert_eq!(
            stored.nodes.iter().filter(|node| node.id == "ax").count(),
            1
        );
        assert!(stored
            .nodes
            .iter()
            .any(|node| { node.id == "billing" && node.code_node_ids == ["fn:bill"] }));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn three_runs_leave_the_same_graph() {
        let dir = project("repeat");
        apply(&dir, false).unwrap();
        let first = fs::read(graph_path(&dir)).unwrap();
        apply(&dir, false).unwrap();
        apply(&dir, false).unwrap();
        assert_eq!(fs::read(graph_path(&dir)).unwrap(), first);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn migration_from_1_0_0_records_the_step() {
        let dir = project("migrate");
        write_version(
            &dir,
            &VersionFile {
                seed_version: PREVIOUS_SEED_VERSION.into(),
                migrations: Vec::new(),
            },
        )
        .unwrap();
        write_graph(
            &dir,
            &DomainFile {
                version: 1,
                nodes: vec![DomainNode {
                    id: "ax.legacy".into(),
                    kind: "domain".into(),
                    name: "Legacy".into(),
                    summary: Some("kept from seed 1.0.0".into()),
                    code_node_ids: Vec::new(),
                }],
                edges: Vec::new(),
            },
        )
        .unwrap();
        let report = apply(&dir, false).unwrap();
        assert_eq!(report.migration.as_deref(), Some("1.0.0 -> 2.0.0"));
        let version = read_version(&dir).unwrap().unwrap();
        assert_eq!(version.seed_version, AX_SEED_VERSION);
        assert_eq!(version.migrations, vec!["1.0.0 -> 2.0.0".to_string()]);
        let second = apply(&dir, false).unwrap();
        assert!(second.migration.is_none());
        assert!(read_graph(&dir)
            .unwrap()
            .nodes
            .iter()
            .any(|node| node.id == "ax.legacy"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn dry_run_does_not_write() {
        let dir = project("dry");
        let report = apply(&dir, true).unwrap();
        assert!(report.dry_run);
        assert!(report.entities_created > 0);
        assert!(!graph_path(&dir).exists());
        assert!(!version_path(&dir).exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_package_dependency_fails() {
        let mut manifest = manifest();
        manifest.packages.push(SeedPackage {
            id: "orphan".into(),
            version: "2.0.0".into(),
            required: true,
            dependencies: vec!["missing".into()],
            checksum: None,
        });
        let err = validate_manifest(&manifest).unwrap_err();
        assert!(err.contains("missing"), "{err}");
    }

    #[test]
    fn empty_manifest_version_is_refused() {
        let mut manifest = manifest();
        manifest.version.clear();
        assert!(validate_manifest(&manifest)
            .unwrap_err()
            .contains("version"));
    }

    #[test]
    fn bootstrap_does_not_need_pi() {
        let dir = project("no-pi");
        assert!(apply(&dir, false).is_ok());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn project_seed_cannot_move_pi_ownership() {
        let dir = project("override");
        let seed = dir.join(".ax").join("seed");
        fs::create_dir_all(&seed).unwrap();
        fs::write(
            seed.join("project.json"),
            r#"{"rules":[{"id":"pi-owns-runtime","description":"Ax owns the agent loop"}]}"#,
        )
        .unwrap();
        assert!(apply(&dir, false).unwrap_err().contains("foundational"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn project_preference_is_allowed() {
        let dir = project("nunit");
        let seed = dir.join(".ax").join("seed");
        fs::create_dir_all(&seed).unwrap();
        fs::write(
            seed.join("project.json"),
            r#"{"rules":[{"id":"project-nunit","description":"project prefers NUnit"}]}"#,
        )
        .unwrap();
        apply(&dir, false).unwrap();
        let body = fs::read_to_string(agents_rules(&dir).join("project-nunit.mdc")).unwrap();
        assert!(body.contains("NUnit"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn project_cannot_claim_pi_persistence() {
        let dir = project("pi-persist");
        let seed = dir.join(".ax").join("seed");
        fs::create_dir_all(&seed).unwrap();
        fs::write(
            seed.join("project.json"),
            r#"{"rules":[{"id":"local-owner","description":"Ax owns Pi persistence"}]}"#,
        )
        .unwrap();
        assert!(apply(&dir, false).unwrap_err().contains("Pi runtime"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn unsupported_seed_version_is_refused() {
        let dir = project("old");
        write_version(
            &dir,
            &VersionFile {
                seed_version: "0.9.0".into(),
                migrations: Vec::new(),
            },
        )
        .unwrap();
        assert!(apply(&dir, false)
            .unwrap_err()
            .contains("unsupported seed version"));
        assert!(!graph_path(&dir).exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn dry_run_plan_names_the_sections() {
        let dir = project("plan");
        let report = apply(&dir, true).unwrap();
        assert!(report.plan.starts_with("SEED PLAN"));
        for section in [
            "Core:",
            "Runtime:",
            "Context:",
            "Economics:",
            "Optimization:",
            "Changes:",
        ] {
            assert!(report.plan.contains(section), "{}", report.plan);
        }
        let _ = fs::remove_dir_all(dir);
    }
}
