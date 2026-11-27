//! Print a stored spec or plan body.

use std::path::PathBuf;

pub async fn show(spec: String) -> Result<(), String> {
    let root = project_root()?;
    let database = ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .map_err(|error| error.to_string())?;
    let body = ax_usage::show_spec(database.pool(), &spec).await?;
    println!("{body}");
    Ok(())
}

enum HierarchyPost {
    Create,
    Alternative,
    Revise,
    Link,
    Select,
    Compare,
    Tree,
    Provenance,
    Policy,
    Compatibility,
    SpecRevision,
    Import,
    Classify,
}

pub async fn import_from_json(body: String) -> Result<(), String> {
    post_json(body, HierarchyPost::Import).await
}

pub async fn classify_from_json(body: String) -> Result<(), String> {
    post_json(body, HierarchyPost::Classify).await
}

pub async fn create_from_json(body: String) -> Result<(), String> {
    post_json(body, HierarchyPost::Create).await
}

pub async fn alternative_from_json(body: String) -> Result<(), String> {
    post_json(body, HierarchyPost::Alternative).await
}

pub async fn revise_from_json(body: String) -> Result<(), String> {
    post_json(body, HierarchyPost::Revise).await
}

pub async fn revise_spec_from_json(body: String) -> Result<(), String> {
    post_json(body, HierarchyPost::SpecRevision).await
}

pub async fn link_from_json(body: String) -> Result<(), String> {
    post_json(body, HierarchyPost::Link).await
}

pub async fn select_from_json(body: String) -> Result<(), String> {
    post_json(body, HierarchyPost::Select).await
}

pub async fn compare_from_json(body: String) -> Result<(), String> {
    post_json(body, HierarchyPost::Compare).await
}

pub async fn list_from_json(body: String) -> Result<(), String> {
    post_json(body, HierarchyPost::Tree).await
}

pub async fn provenance_from_json(body: String) -> Result<(), String> {
    post_json(body, HierarchyPost::Provenance).await
}

pub async fn policy_from_json(body: String) -> Result<(), String> {
    post_json(body, HierarchyPost::Policy).await
}

pub async fn compatibility_from_json(body: String) -> Result<(), String> {
    post_json(body, HierarchyPost::Compatibility).await
}

pub async fn review_queue() -> Result<(), String> {
    let root = project_root()?;
    let database = ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .map_err(|error| error.to_string())?;
    let result = ax_usage::import_review_from_json(database.pool()).await;
    finish_json(result)
}

pub async fn flag_from_value(value: Option<String>) -> Result<(), String> {
    let body = match value.as_deref().map(str::trim).unwrap_or("") {
        "" => serde_json::json!({}),
        "on" => serde_json::json!({ "enabled": true }),
        "off" => serde_json::json!({ "enabled": false }),
        other => serde_json::from_str(other).map_err(|error| error.to_string())?,
    };
    let root = project_root()?;
    let database = ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .map_err(|error| error.to_string())?;
    let result = ax_usage::hierarchy_flag_from_json(database.pool(), body).await;
    finish_json(result)
}

pub async fn approve_from_json(body: String) -> Result<(), String> {
    let value: serde_json::Value =
        serde_json::from_str(&body).map_err(|error| error.to_string())?;
    let root = project_root()?;
    let database = ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .map_err(|error| error.to_string())?;
    let path = ax_global_db::global_db_path().map_err(|error| error.to_string())?;
    let membership = ax_global_db::open_and_init(&path)
        .await
        .map_err(|error| error.to_string())?;
    let members = ax_global_db::access::list_members(&membership)
        .await
        .map_err(|error| error.to_string())?;
    let owner = members
        .into_iter()
        .find(|member| {
            member.role == ax_global_db::access::MemberRole::Owner && member.status == "active"
        })
        .ok_or_else(|| "no active owner".to_string())?;
    let value = ax_usage::bind_approval_actor(value, &owner.id)?;
    let result = ax_usage::approve_from_json(database.pool(), &membership, value).await;
    finish_json(result)
}

async fn post_json(body: String, kind: HierarchyPost) -> Result<(), String> {
    let value: serde_json::Value =
        serde_json::from_str(&body).map_err(|error| error.to_string())?;
    let root = project_root()?;
    let database = ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .map_err(|error| error.to_string())?;
    let result = match kind {
        HierarchyPost::Create => ax_usage::create_hierarchy_from_json(database.pool(), value).await,
        HierarchyPost::Alternative => {
            ax_usage::alternative_plan_from_json(database.pool(), value).await
        }
        HierarchyPost::Revise => ax_usage::revise_plan_from_json(database.pool(), value).await,
        HierarchyPost::SpecRevision => {
            ax_usage::revise_spec_from_json(database.pool(), value).await
        }
        HierarchyPost::Link => ax_usage::link_session_from_json(database.pool(), value).await,
        HierarchyPost::Select => ax_usage::select_plan_from_json(database.pool(), value).await,
        HierarchyPost::Compare => ax_usage::compare_plan_from_json(database.pool(), value).await,
        HierarchyPost::Tree => ax_usage::list_hierarchy_from_json(database.pool(), value).await,
        HierarchyPost::Provenance => ax_usage::provenance_from_json(database.pool(), value).await,
        HierarchyPost::Policy => ax_usage::link_policy_from_json(database.pool(), value).await,
        HierarchyPost::Compatibility => {
            ax_usage::confirm_compatibility_from_json(database.pool(), value).await
        }
        HierarchyPost::Import => ax_usage::import_spec_from_json(database.pool(), value).await,
        HierarchyPost::Classify => {
            ax_usage::classify_documents_from_json(database.pool(), value).await
        }
    };
    finish_json(result)
}

fn finish_json(result: serde_json::Value) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string(&result).map_err(|error| error.to_string())?
    );
    if result.get("ok").and_then(|ok| ok.as_bool()) == Some(true) {
        Ok(())
    } else {
        Err(result
            .get("error")
            .and_then(|error| error.as_str())
            .unwrap_or("request failed")
            .to_string())
    }
}

fn project_root() -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    super::find_project_root(&cwd).ok_or_else(|| "no ax project database found".into())
}
