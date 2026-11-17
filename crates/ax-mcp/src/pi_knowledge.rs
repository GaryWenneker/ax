//! Build Pi context sections from the existing policy, memory, and graph stores.
//!
//! Ax does not write these sections into Pi's transcript. Callers return them
//! on the existing context tool.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use ax_core::Ax;
use ax_pi::{
    create_pi_integration, ContextRequest, ContextResult, ContextSection, PiOptions, SectionType,
};
use ax_policy::MatchInput;
use ax_types::ExploreOptions;

fn integrations() -> &'static Mutex<HashMap<String, ax_pi::PiIntegration>> {
    static INTEGRATIONS: OnceLock<Mutex<HashMap<String, ax_pi::PiIntegration>>> = OnceLock::new();
    INTEGRATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn mcp_current_ids(root: &Path) -> (String, String) {
    let fallback = ("mcp".to_string(), "context".to_string());
    let Ok(text) = std::fs::read_to_string(root.join(".ax").join("turns").join("current.json"))
    else {
        return fallback;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return fallback;
    };
    let session = value
        .get("session_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .trim();
    let turn = value
        .get("turn_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .trim();
    if session.is_empty() || turn.is_empty() {
        return fallback;
    }
    (session.to_string(), turn.to_string())
}

pub async fn pi_context_for_task(ax: &mut Ax, task: &str) -> Result<ContextResult, String> {
    let mut sections = Vec::new();
    let mut entities = Vec::new();
    append_policy(ax, task, &mut sections).await;
    append_memories(ax, task, &mut sections).await;
    append_graph(ax, task, &mut sections, &mut entities).await;

    let key = ax.project_root().display().to_string();
    let mut guard = integrations()
        .lock()
        .map_err(|err| format!("AX_PI_INTEGRATION_ERROR {err}"))?;
    let integration = guard.entry(key.clone()).or_insert_with(|| {
        let options = PiOptions {
            project_id: key,
            repository_root: Some(ax.project_root().display().to_string()),
            ..PiOptions::default()
        };
        create_pi_integration(options)
    });
    integration
        .set_knowledge(sections)
        .map_err(|err| err.to_string())?;
    integration
        .set_graph_entities(entities)
        .map_err(|err| err.to_string())?;
    let (session_id, turn_id) = mcp_current_ids(ax.project_root());
    integration
        .get_session_context(&ContextRequest {
            session_id,
            turn_id,
            user_intent: Some(task.to_string()),
            repository_root: Some(ax.project_root().display().to_string()),
            changed_files: Vec::new(),
            previous_tool_names: Vec::new(),
        })
        .map_err(|err| err.to_string())
}

async fn append_policy(ax: &Ax, task: &str, sections: &mut Vec<ContextSection>) {
    let matched = match ax
        .match_policy(MatchInput {
            prompt: task.to_string(),
            cwd: ax.project_root().to_path_buf(),
            open_files: Vec::new(),
            changed_files: Vec::new(),
        })
        .await
    {
        Ok(matched) => matched,
        Err(err) => {
            tracing::warn!("AX_PI_INTEGRATION_ERROR {err}");
            return;
        }
    };
    for rule in matched.rules {
        sections.push(section(
            format!("rule:{}", rule.id),
            SectionType::Rule,
            rule.body,
            rule.always_apply,
            if rule.always_apply { 100 } else { 80 },
            Vec::new(),
        ));
    }
    for skill in matched.skills {
        let content = if skill.body.is_empty() {
            skill.description
        } else {
            skill.body
        };
        sections.push(section(
            format!("skill:{}", skill.name),
            SectionType::Skill,
            content,
            skill.always_apply,
            90,
            Vec::new(),
        ));
    }
}

async fn append_memories(ax: &Ax, task: &str, sections: &mut Vec<ContextSection>) {
    let memories = match ax_memory::recall_for_prompt(ax.db_pool(), task, 5).await {
        Ok(memories) => memories,
        Err(err) => {
            tracing::warn!("AX_PI_INTEGRATION_ERROR {err}");
            return;
        }
    };
    for item in memories {
        let content = format!("{}: {}", item.memory.title, item.memory.body);
        let paths = item.memory.files.clone();
        sections.push(section(
            format!("memory:{}", item.memory.id),
            SectionType::Memory,
            content,
            false,
            50,
            paths,
        ));
    }
}

async fn append_graph(
    ax: &mut Ax,
    task: &str,
    sections: &mut Vec<ContextSection>,
    entities: &mut Vec<String>,
) {
    if task.trim().is_empty() {
        return;
    }
    let explored = match ax
        .explore(
            task,
            ExploreOptions {
                limit: Some(3),
                depth: Some(1),
                include_code: Some(false),
                max_lines_per_snippet: Some(20),
                max_source_chars: Some(800),
            },
        )
        .await
    {
        Ok(explored) => explored,
        Err(err) => {
            tracing::warn!("AX_PI_INTEGRATION_ERROR {err}");
            return;
        }
    };
    for entry in explored.entries {
        let name = entry.node.qualified_name.clone();
        entities.push(name.clone());
        let path = entry.node.file_path.clone();
        sections.push(section(
            format!("graph:{}", entry.node.id),
            SectionType::Graph,
            format!("{name} {path}"),
            false,
            70,
            vec![path],
        ));
    }
}

fn section(
    id: String,
    kind: SectionType,
    content: String,
    mandatory: bool,
    priority: u32,
    source_paths: Vec<String>,
) -> ContextSection {
    ContextSection {
        token_estimate: ax_usage::count_tokens(&content) as u32,
        id,
        kind,
        priority,
        content,
        mandatory,
        source_paths,
    }
}

#[cfg(test)]
mod tests {
    use super::mcp_current_ids;

    #[test]
    fn mcp_context_uses_the_current_turn_and_keeps_the_placeholder_without_one() {
        let root = std::env::temp_dir().join(format!("ax-mcp-turn-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".ax")).unwrap();
        assert_eq!(
            mcp_current_ids(&root),
            ("mcp".to_string(), "context".to_string())
        );
        std::fs::create_dir_all(root.join(".ax/turns")).unwrap();
        std::fs::write(
            root.join(".ax/turns/current.json"),
            r#"{"session_id":"conv-1","turn_id":"gen-1"}"#,
        )
        .unwrap();
        assert_eq!(
            mcp_current_ids(&root),
            ("conv-1".to_string(), "gen-1".to_string())
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
