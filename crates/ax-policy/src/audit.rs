//! Read-only inventory: compare authored files with SQLite, never choose a winner.
use ax_utils::errors::AxError;
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::collections::BTreeMap;
use std::path::Path;

fn hash(text: &str) -> String {
    blake3::hash(text.as_bytes()).to_hex().to_string()
}

pub async fn inventory(pool: &SqlitePool, root: &Path) -> Result<Value, AxError> {
    let config = crate::load_policy_config(root);
    let mut items: BTreeMap<(String, String), Value> = BTreeMap::new();
    for row in crate::list_rules(pool).await? {
        let doc = crate::rule_row_to_doc(&row, root);
        items.insert(("rule".into(), row.id.clone()), json!({
            "kind":"rule", "id":row.id, "scope":row.scope, "sourcePath":row.source_path,
            "dbBodyHash":hash(&row.body),
            "dbMetadataHash":hash(&serde_json::to_value(&doc.frontmatter).map_err(|e| AxError::Other(e.to_string()))?.to_string()),
            "enabled":row.enabled, "status":row.status,
            "authority":crate::effective_storage(config.storage, row.storage.as_deref()).as_str(),
            "origin":row.source, "rootId":row.root_id, "classificationRequired":row.source.is_none(),
        }));
    }
    for row in crate::list_skills(pool).await? {
        let doc = crate::skill_row_to_doc(&row, root);
        items.insert(("skill".into(), row.name.clone()), json!({
            "kind":"skill", "id":row.name, "scope":row.scope, "sourcePath":row.source_path,
            "dbBodyHash":hash(&row.body),
            "dbMetadataHash":hash(&serde_json::to_value(&doc.frontmatter).map_err(|e| AxError::Other(e.to_string()))?.to_string()),
            "enabled":row.enabled, "status":row.status,
            "authority":crate::effective_storage(config.storage, row.storage.as_deref()).as_str(),
            "origin":row.source, "rootId":row.root_id, "classificationRequired":row.source.is_none(),
        }));
    }
    let mut issues = Vec::new();
    for layer in crate::policy_layers(root) {
        for entry in walkdir::WalkDir::new(&layer.dir).follow_links(false) {
            let entry = entry.map_err(|e| AxError::Other(e.to_string()))?;
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let rule = path.extension().is_some_and(|e| e == "mdc");
            let skill = path.file_name().is_some_and(|n| n == "SKILL.md");
            if !rule && !skill {
                continue;
            }
            let raw = std::fs::read_to_string(path).map_err(|e| AxError::Other(e.to_string()))?;
            let parsed = if rule {
                crate::parse::parse_rule_file(path, &raw).map(|d| {
                    (
                        "rule",
                        d.frontmatter.id.clone(),
                        d.body,
                        serde_json::to_value(d.frontmatter),
                    )
                })
            } else {
                crate::parse::parse_skill_file(path, &raw).map(|d| {
                    (
                        "skill",
                        d.frontmatter.name.clone(),
                        d.body,
                        serde_json::to_value(d.frontmatter),
                    )
                })
            };
            let (kind, id, body, metadata) = match parsed {
                Ok(parsed) => parsed,
                Err(error) => {
                    issues.push(json!({"path":path, "issue":error}));
                    continue;
                }
            };
            let metadata = metadata.map_err(|e| AxError::Other(e.to_string()))?;
            let key = (kind.to_string(), id.clone());
            let item = items.entry(key).or_insert_with(|| json!({"kind":kind,"id":id,"authority":config.storage.as_str(),"classificationRequired":true}));
            let disk = json!({"path":path,"scope":layer.scope.as_str(),"bodyHash":hash(&body),"metadataHash":hash(&metadata.to_string()), "origin":metadata.get("originProject")});
            if item.get("diskSources").is_none() {
                item["diskSources"] = json!([]);
            }
            item["diskSources"]
                .as_array_mut()
                .ok_or_else(|| AxError::Other("invalid audit inventory".into()))?
                .push(disk);
        }
    }
    for item in items.values_mut() {
        let same = item
            .get("diskSources")
            .and_then(Value::as_array)
            .is_some_and(|sources| {
                sources.iter().any(|disk| {
                    disk["bodyHash"] == item["dbBodyHash"]
                        && disk["metadataHash"] == item["dbMetadataHash"]
                })
            });
        item["parity"] = json!(if same {
            "same"
        } else if item.get("dbBodyHash").is_none() {
            "disk-only"
        } else if item.get("diskSources").is_none() {
            "database-only"
        } else {
            "different"
        });
    }
    let memories = sqlx::query_as::<_, (String, String, String, String, i64)>(
        "SELECT id, title, body, source, enabled FROM memories ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| AxError::Other(e.to_string()))?
    .into_iter()
    .map(|(id, title, body, source, enabled)| {
        json!({"id":id,"title":title,
            "bodyHash":hash(&body),"origin":source,"scope":"project","enabled":enabled != 0,
            "classificationRequired":matches!(source.as_str(), "manual" | "import" | "unknown")})
    })
    .collect::<Vec<_>>();
    Ok(
        json!({"memories":memories,"project":root,"readOnly":true,"items":items.into_values().collect::<Vec<_>>(),"issues":issues,
        "instruction":"Review origin and scope before importing, sharing or deleting. Database authority is not overwritten automatically; hashes identify differences, not a winning revision."}),
    )
}
