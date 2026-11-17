//! `ax neural` — required Neural Intelligence. Prints the same JSON as the MCP tools.

use ax_db::Database;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub async fn run(root: PathBuf, action: &str, args: Value) -> Result<(), String> {
    let db_path = root.join(".ax").join("ax.db");
    // The bake-off reads the built-in coding fixture only. It must not open the live database.
    let db = if action != "benchmark"
        && action != "benchmark-abc"
        && action != "embed-bakeoff"
        && action != "path-validate"
        && action != "agent-eval"
        && db_path.is_file()
    {
        Some(
            Database::open(&db_path)
                .await
                .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    let value =
        ax_neural::dispatch::handle(&root, action, &args, db.as_ref().map(Database::pool)).await?;
    if action == "models" {
        if let Some(line) = value.get("status").and_then(|v| v.as_str()) {
            eprintln!("{line}");
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".into())
    );
    Ok(())
}

/// The canonical project root, found the way the MCP server finds it: the nearest
/// directory with `.ax`, else the canonical start path. Neural keys rows by this path.
pub fn project_root(path: Option<String>) -> Result<PathBuf, String> {
    let start = match path {
        Some(path) => PathBuf::from(path),
        None => std::env::current_dir().map_err(|error| error.to_string())?,
    };
    if let Some(root) = ax_context::directory::find_nearest_ax_root(&start) {
        return Ok(root);
    }
    start
        .canonicalize()
        .map_err(|error| format!("{}: {error}", start.display()))
}

pub struct FeedbackFlags {
    pub execution_id: String,
    pub task_category: String,
    pub outcome: String,
    pub context_ids: Vec<String>,
    pub skill_ids: Vec<String>,
    pub rule_ids: Vec<String>,
    pub retry_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

pub fn feedback_args(flags: FeedbackFlags) -> Value {
    json!({
        "executionId": flags.execution_id,
        "taskCategory": flags.task_category,
        "outcome": flags.outcome,
        "contextIds": flags.context_ids,
        "skillIds": flags.skill_ids,
        "ruleIds": flags.rule_ids,
        "retryCount": flags.retry_count,
        "inputTokens": flags.input_tokens,
        "outputTokens": flags.output_tokens,
    })
}

pub struct RegisterFlags {
    pub model_id: String,
    pub version: String,
    pub checksum: String,
    pub model_type: String,
    pub dataset_version: String,
    pub platforms: String,
    pub metrics: Option<String>,
}

pub fn register_args(flags: RegisterFlags) -> Result<Value, String> {
    let metrics = match flags.metrics {
        Some(text) => parse_json(&text, "--metrics")?,
        None => json!({}),
    };
    if !metrics.is_object() {
        return Err("--metrics must be a JSON object".into());
    }
    Ok(json!({
        "modelId": flags.model_id,
        "version": flags.version,
        "checksum": flags.checksum,
        "modelType": flags.model_type,
        "datasetVersion": flags.dataset_version,
        "platforms": flags.platforms,
        "metrics": metrics,
    }))
}

pub fn rank_args(query: String, blocks: Option<String>) -> Result<Value, String> {
    let blocks = match blocks {
        Some(text) => json_array(&text, "--blocks")?,
        None => json!([]),
    };
    Ok(json!({ "query": query, "blocks": blocks }))
}

/// Without `--edges` the graph command reads this project's edges and stores suggestions.
pub fn graph_args(edges: Option<String>) -> Result<Value, String> {
    match edges {
        Some(text) => Ok(json!({ "edges": json_array(&text, "--edges")? })),
        None => Ok(json!({})),
    }
}

fn parse_json(text: &str, flag: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|error| format!("{flag} is not valid JSON: {error}"))
}

fn json_array(text: &str, flag: &str) -> Result<Value, String> {
    let value = parse_json(text, flag)?;
    if value.is_array() {
        Ok(value)
    } else {
        Err(format!("{flag} must be a JSON array"))
    }
}

pub async fn export(root: PathBuf, out: PathBuf) -> Result<(), String> {
    let summary = write_export(&root, &out).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&summary).unwrap_or_else(|_| "{}".into())
    );
    Ok(())
}

/// Writes the training export to `out` and returns a summary without the rows.
pub async fn write_export(root: &Path, out: &Path) -> Result<Value, String> {
    let db_path = root.join(".ax").join("ax.db");
    if !db_path.is_file() {
        return Err(format!("no database at {}", db_path.display()));
    }
    let db = Database::open(&db_path)
        .await
        .map_err(|error| error.to_string())?;
    let project = ax_neural::dispatch::project_id(root);
    let export = ax_neural::store::training_export(db.pool(), &project).await?;
    let text = serde_json::to_string_pretty(&export).map_err(|error| error.to_string())?;
    std::fs::write(out, text).map_err(|error| format!("{}: {error}", out.display()))?;
    Ok(json!({
        "out": out.display().to_string(),
        "format": export["format"],
        "exported": export["exported"],
        "skippedSecretShaped": export["skippedSecretShaped"],
        "projectPathIncluded": false,
        "sourceIncluded": false
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(value: &Value) -> Vec<String> {
        let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        keys
    }

    fn sorted(names: &[&str]) -> Vec<String> {
        let mut names: Vec<String> = names.iter().map(|name| name.to_string()).collect();
        names.sort();
        names
    }

    #[test]
    fn feedback_args_use_mcp_field_names() {
        let args = feedback_args(FeedbackFlags {
            execution_id: "e1".into(),
            task_category: "parser".into(),
            outcome: "success".into(),
            context_ids: vec!["memory:m1".into()],
            skill_ids: vec!["parse-errors".into()],
            rule_ids: vec!["english-only".into()],
            retry_count: 2,
            input_tokens: 300,
            output_tokens: 40,
        });
        assert_eq!(
            keys(&args),
            sorted(&[
                "executionId",
                "taskCategory",
                "outcome",
                "contextIds",
                "skillIds",
                "ruleIds",
                "retryCount",
                "inputTokens",
                "outputTokens",
            ])
        );
        assert_eq!(args["contextIds"], json!(["memory:m1"]));
        assert_eq!(args["retryCount"], 2);
        assert_eq!(args["outputTokens"], 40);
    }

    fn register(metrics: Option<&str>) -> Result<Value, String> {
        register_args(RegisterFlags {
            model_id: "minilm".into(),
            version: "1".into(),
            checksum: "sha256:abc".into(),
            model_type: "embedding".into(),
            dataset_version: "coding-v1".into(),
            platforms: "macos-arm64".into(),
            metrics: metrics.map(str::to_string),
        })
    }

    #[test]
    fn register_args_use_mcp_field_names() {
        let args = register(Some(r#"{"precision":0.8}"#)).unwrap();
        assert_eq!(
            keys(&args),
            sorted(&[
                "modelId",
                "version",
                "checksum",
                "modelType",
                "datasetVersion",
                "platforms",
                "metrics",
            ])
        );
        assert_eq!(args["metrics"]["precision"], 0.8);
        assert_eq!(args["datasetVersion"], "coding-v1");
        assert!(register(Some("not json")).is_err());
        assert!(register(Some("[1]")).is_err());
        assert_eq!(register(None).unwrap()["metrics"], json!({}));
    }

    #[test]
    fn rank_and_graph_take_json_arrays() {
        let rank = rank_args(
            "parser".into(),
            Some(r#"[{"id":"a","class":"optional","text":"parser"}]"#.into()),
        )
        .unwrap();
        assert_eq!(rank["blocks"][0]["id"], "a");
        assert!(rank_args("parser".into(), Some("{}".into())).is_err());
        let graph = graph_args(Some(r#"[{"source":"a","target":"b"}]"#.into())).unwrap();
        assert_eq!(graph["edges"][0]["target"], "b");
        assert!(graph_args(Some("not json".into())).is_err());
        assert!(graph_args(None).unwrap().get("edges").is_none());
    }

    #[test]
    fn project_root_walks_up_to_the_canonical_ax_root() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".ax")).unwrap();
        std::fs::write(dir.path().join(".ax").join("ax.db"), b"").unwrap();
        let nested = dir.path().join("src").join("deep");
        std::fs::create_dir_all(&nested).unwrap();
        let root = project_root(Some(nested.to_string_lossy().into_owned())).unwrap();
        assert_eq!(root, dir.path().canonicalize().unwrap());
    }

    #[test]
    fn project_root_without_ax_is_the_canonical_path() {
        let dir = tempfile::tempdir().unwrap();
        let dotted = dir.path().join("a").join("..");
        std::fs::create_dir(dir.path().join("a")).unwrap();
        let root = project_root(Some(dotted.to_string_lossy().into_owned())).unwrap();
        assert_eq!(root, dir.path().canonicalize().unwrap());
    }

    #[tokio::test]
    async fn export_writes_feedback_without_the_project_path() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::write(
            root.join("ax.json"),
            r#"{"neural":{"learning":{"feedback_collection":true}}}"#,
        )
        .unwrap();
        std::fs::create_dir(root.join(".ax")).unwrap();
        let db = Database::open(&root.join(".ax").join("ax.db"))
            .await
            .unwrap();
        ax_neural::dispatch::handle(
            &root,
            "feedback",
            &json!({"executionId":"e1","taskCategory":"parser","outcome":"success"}),
            Some(db.pool()),
        )
        .await
        .unwrap();
        drop(db);
        let out = root.join("export.json");
        let summary = write_export(&root, &out).await.unwrap();
        assert_eq!(summary["exported"], 1);
        let text = std::fs::read_to_string(&out).unwrap();
        let written: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(written["format"], "ax-neural-feedback-v1");
        assert_eq!(written["rows"][0]["taskCategory"], "parser");
        assert!(!text.contains(&*root.to_string_lossy()), "{text}");
    }

    #[tokio::test]
    async fn export_without_a_database_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("export.json");
        let error = write_export(dir.path(), &out).await.unwrap_err();
        assert!(error.contains("no database"), "{error}");
        assert!(!out.exists());
    }
}
