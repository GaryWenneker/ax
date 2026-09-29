//! Global rules and skills in `~/.ax/global.db`, as the vault and the links API see them:
//! one item per name, the copy that leads among the global rows.

use std::path::Path;

use ax_global_db::policy::PolicyKind;
use ax_policy::global_level::{self, Kind};
use ax_policy::{PolicyRuleDoc, PolicySkillDoc, RuleFrontmatter, SkillFrontmatter};
use serde_json::Value;

use crate::policy::{payload_to_rule, payload_to_skill, rule_doc_from_parts, skill_doc_from_parts};

pub struct GlobalItem {
    pub name: String,
    pub project_id: i64,
    pub changed_ms: i64,
    payload: Value,
}

impl GlobalItem {
    fn source(&self) -> String {
        format!("global.db:{}:{}", self.project_id, self.name)
    }

    pub fn rule_doc(&self) -> Option<PolicyRuleDoc> {
        let (fm, body) = payload_to_rule(&self.payload).ok()?;
        Some(rule_doc_from_parts(fm, body, self.source()))
    }

    pub fn skill_doc(&self) -> Option<PolicySkillDoc> {
        let (fm, body) = payload_to_skill(&self.payload).ok()?;
        Some(skill_doc_from_parts(fm, body, self.source()))
    }
}

fn policy_kind(kind: Kind) -> PolicyKind {
    match kind {
        Kind::Rule => PolicyKind::Rules,
        Kind::Skill => PolicyKind::Skills,
    }
}

/// The leading global items of one kind, by name. Reading never creates `global.db`.
pub async fn leaders(kind: Kind) -> Result<Vec<GlobalItem>, String> {
    let path = ax_global_db::global_db_path().map_err(|e| e.to_string())?;
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let pool = ax_global_db::open_pool(&path, false)
        .await
        .map_err(|e| e.to_string())?;
    let level = global_level::load_from_pool(&pool)
        .await
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for name in level.items(kind).keys() {
        let Some(lead) = level.leader(kind, name) else {
            continue;
        };
        let payload =
            ax_global_db::policy::load_policy_item(&pool, lead.project_id, policy_kind(kind), name)
                .await
                .map_err(|e| e.to_string())?;
        if let Some(payload) = payload {
            out.push(GlobalItem {
                name: name.clone(),
                project_id: lead.project_id,
                changed_ms: lead.changed_ms,
                payload,
            });
        }
    }
    pool.close().await;
    Ok(out)
}

pub async fn leader(kind: Kind, name: &str) -> Result<Option<GlobalItem>, String> {
    Ok(leaders(kind).await?.into_iter().find(|i| i.name == name))
}

/// Write one item the way the Command Center does. `project_id` is the row to update;
/// `None` files a new item under this project.
async fn save(
    root: &Path,
    project_id: Option<i64>,
    kind: Kind,
    name: &str,
    doc: impl FnOnce(String) -> serde_json::Result<Value>,
) -> Result<(), String> {
    let (pool, pid) = crate::policy::open_global_pool(root, project_id).await?;
    let result = async {
        let value = doc(format!("global.db:{pid}:{name}")).map_err(|e| e.to_string())?;
        let flat = ax_global_db::policy::flatten_list_item(value, policy_kind(kind), name);
        ax_global_db::policy::upsert_policy_item(&pool, pid, policy_kind(kind), name, &flat)
            .await
            .map_err(|e| e.to_string())
    }
    .await;
    pool.close().await;
    result
}

pub async fn save_rule(
    root: &Path,
    project_id: Option<i64>,
    fm: RuleFrontmatter,
    body: String,
) -> Result<(), String> {
    let name = fm.id.clone();
    save(root, project_id, Kind::Rule, &name, |source| {
        serde_json::to_value(rule_doc_from_parts(fm, body, source))
    })
    .await
}

pub async fn save_skill(
    root: &Path,
    project_id: Option<i64>,
    fm: SkillFrontmatter,
    body: String,
) -> Result<(), String> {
    let name = fm.name.clone();
    save(root, project_id, Kind::Skill, &name, |source| {
        serde_json::to_value(skill_doc_from_parts(fm, body, source))
    })
    .await
}
