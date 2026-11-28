use crate::config::load;
use crate::graph::{suggest_report_capped, GraphReport, InputEdge};
use crate::learning::{validate_feedback, FeedbackInput};
use crate::share::{packet_for, ModelManifest, SharePolicy};
use crate::status::status_report;
use crate::store::{
    feedback_count, insert_feedback, last_evaluation, list_models, observations, outcome_counts,
    project_edges, promote, promoted_checksum, promoted_models, rank_totals, record_evaluation,
    register, replace_suggestions, rollback, success_context_sets, training_export, ModelRecord,
};
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use std::path::Path;

const EDGE_LIMIT: usize = 200_000;
const FEEDBACK_ROWS: usize = 2_000;

async fn models_reply(pool: Option<&SqlitePool>) -> Result<Value, String> {
    let models: Vec<Value> = if let Some(pool) = pool {
        let rows = list_models(pool).await?;
        rows.into_iter()
            .map(|row| {
                json!({
                    "modelId": row.get::<String, _>("model_id"),
                    "version": row.get::<String, _>("version"),
                    "checksum": row.get::<String, _>("checksum"),
                    "platforms": row.get::<String, _>("platforms"),
                    "promoted": row.get::<i64, _>("promoted") == 1
                })
            })
            .collect()
    } else {
        vec![]
    };
    let admitted = crate::bakeoff::challenger_admitted();
    let mut value = json!({ "models": models });
    if admitted {
        value["bakeOffChallenger"] = json!(crate::lexical::CHALLENGER_ID);
        value["status"] = json!(format!(
            "Bake-off challenger {} admitted; not promoted (explicit promote required)",
            crate::lexical::CHALLENGER_ID
        ));
    } else {
        let note = if !crate::runtime_built() {
            "Build with --features neural-runtime to measure the bake-off challenger"
        } else {
            "No bake-off challenger admitted on the coding fixture"
        };
        value["admissionNote"] = json!(note);
        value["status"] = json!(note);
    }
    Ok(value)
}

/// Strip optional `ax_neural_` / `ax-neural-` prefix and map hyphenated CLI names to underscores.
pub(crate) fn normalize_dispatch_tool(tool: &str) -> String {
    let stripped = tool
        .strip_prefix("ax_neural_")
        .or_else(|| tool.strip_prefix("ax-neural-"))
        .unwrap_or(tool);
    stripped.replace('-', "_")
}

pub async fn handle(
    root: &Path,
    tool: &str,
    args: &Value,
    pool: Option<&SqlitePool>,
) -> Result<Value, String> {
    let name = normalize_dispatch_tool(tool);
    match name.as_str() {
        "status" => {
            let mut value = status_report(root);
            if let Some(pool) = pool {
                add_measured(&mut value, pool, &project_id(root)).await;
            }
            Ok(value)
        }
        "search" => {
            let cfg = load(root);
            if !cfg.semantic_enabled {
                return Ok(json!({
                    "enabled": true,
                    "hits": [],
                    "fallback": "hash-v1",
                    "scannedMemoryColumn": false
                }));
            }
            let Some(pool) = pool else {
                return Ok(json!({
                    "enabled": true,
                    "hits": [],
                    "fallback": "hash-v1",
                    "scannedMemoryColumn": false
                }));
            };
            let query = args.get("query").and_then(|v| v.as_str());
            let model_id = args
                .get("modelId")
                .or_else(|| args.get("model_id"))
                .and_then(|v| v.as_str());
            crate::store::search_semantic_query(pool, query, model_id).await
        }
        "rank" => {
            let cfg = load(root);
            if !cfg.ranking_active() {
                return Ok(json!({ "applied": false, "enabled": false }));
            }
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let blocks = args
                .get("blocks")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let items = blocks_from_json(&blocks);
            let notes = crate::plan_blocks(&items, query);
            let order = crate::rank::keep_order(&items, &notes);
            Ok(json!({ "applied": true, "order": order, "notes": notes }))
        }
        "predict" => {
            let cfg = load(root);
            if !cfg.prediction_active() {
                let mut value = json!({ "applied": false, "predictions": [] });
                if let Some(error) = &cfg.config_error {
                    value["configError"] = json!(error);
                }
                return Ok(value);
            }
            let Some(pool) = pool else {
                return Ok(json!({ "applied": false, "predictions": [], "reason": "no database" }));
            };
            let task = args.get("task").and_then(|v| v.as_str()).unwrap_or("");
            let rows = observations(pool, &project_id(root)).await?;
            let (edges, _) = project_edges(pool, EDGE_LIMIT).await?;
            let edge_pairs: Vec<(String, String)> = edges
                .into_iter()
                .map(|edge| (edge.source, edge.target))
                .collect();
            let predictions = crate::predict::predict_with_edges(
                &rows,
                task,
                cfg.minimum_confidence,
                &edge_pairs,
            );
            Ok(json!({
                "applied": true,
                "label": "predicted",
                "predictions": predictions
            }))
        }
        "explain" => {
            let cfg = load(root);
            Ok(json!({
                "enabled": cfg.enabled,
                "memoryModel": "hash-v1",
                "semanticWeight": crate::SEMANTIC_WEIGHT,
                "fusionWeights": {
                    "fts": ax_memory::fusion::W_FTS,
                    "vector": ax_memory::fusion::W_VECTOR,
                    "actR": ax_memory::fusion::W_ACT_R,
                    "graph": ax_memory::fusion::W_GRAPH,
                    "usage": ax_memory::fusion::W_USAGE
                },
                "gnn": crate::graph::gnn_status(),
                "configError": cfg.config_error
            }))
        }
        "feedback" => {
            let cfg = load(root);
            if !cfg.feedback_active() {
                return Err(match &cfg.config_error {
                    Some(error) => format!("feedback collection is disabled; ax.json: {error}"),
                    None => "feedback collection is disabled".into(),
                });
            }
            let Some(pool) = pool else {
                return Err("no database".into());
            };
            let input = FeedbackInput {
                project_id: project_id(root),
                execution_id: required_str(args, "executionId")?,
                task_category: required_str(args, "taskCategory")?,
                context_ids: string_list(args, "contextIds"),
                skill_ids: string_list(args, "skillIds"),
                rule_ids: string_list(args, "ruleIds"),
                outcome: required_str(args, "outcome")?,
                retry_count: args.get("retryCount").and_then(|v| v.as_i64()).unwrap_or(0),
                input_tokens: args
                    .get("inputTokens")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0),
                output_tokens: args
                    .get("outputTokens")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0),
                source_text: args
                    .get("sourceText")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            };
            let row = validate_feedback(&input)?;
            insert_feedback(pool, &row).await?;
            Ok(json!({ "stored": true, "contextAttribution": "unverified", "id": row.id }))
        }
        "graph" => {
            let cfg = load(root);
            if !cfg.graph_enabled {
                return Ok(json!({ "applied": false, "suggestions": [], "gnn": "not_in_binary" }));
            }
            let passed = input_edges(args);
            if !passed.is_empty() {
                let report = suggest_report_capped(&passed, &[], EDGE_LIMIT);
                return Ok(graph_reply(&report, report.edge_limit_reached, 0, 0));
            }
            let Some(pool) = pool else {
                return Err("no database".into());
            };
            let project = project_id(root);
            let (edges, edge_limit_reached) = project_edges(pool, EDGE_LIMIT).await?;
            let rows = success_context_sets(pool, &project, FEEDBACK_ROWS).await?;
            let report = suggest_report_capped(&edges, &rows, EDGE_LIMIT);
            let stored = replace_suggestions(pool, &project, &report.suggestions).await?;
            Ok(graph_reply(
                &report,
                edge_limit_reached || report.edge_limit_reached,
                rows.len(),
                stored,
            ))
        }
        "model_list" | "models" => models_reply(pool).await,
        "model_status" => {
            let Some(pool) = pool else {
                return Ok(json!({ "model": "hash-v1", "promoted": null }));
            };
            let model_id = args.get("modelId").and_then(|v| v.as_str()).unwrap_or("");
            let checksum = if model_id.is_empty() {
                None
            } else {
                promoted_checksum(pool, model_id).await?
            };
            Ok(json!({
                "memoryModel": "hash-v1",
                "modelId": model_id,
                "promotedChecksum": checksum
            }))
        }
        "benchmark" => crate::bakeoff::baseline_report(crate::CODING_FIXTURE).await,
        "benchmark_abc" => crate::benchmark_abc::run_abc(crate::CODING_FIXTURE).await,
        "embed_bakeoff" => crate::embed_bakeoff::run_embed_bakeoff(crate::CODING_FIXTURE),
        "path_validate" => crate::path_validate::run_path_validate(crate::CODING_FIXTURE).await,
        "agent_eval" => crate::agent_eval::run_agent_eval(crate::CODING_FIXTURE).await,
        "learn" | "train" => {
            let cfg = load(root);
            if !cfg.feedback_active() && pool.is_none() {
                return Ok(serde_json::json!({
                    "applied": false,
                    "reason": "feedback collection disabled and no database rows"
                }));
            }
            let seed = args.get("seed").and_then(|v| v.as_u64()).unwrap_or(1);
            let ratio = args
                .get("validationRatio")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.25);
            let dataset_version = args
                .get("datasetVersion")
                .and_then(|v| v.as_str())
                .unwrap_or("feedback-export");
            let model_version = args
                .get("modelVersion")
                .and_then(|v| v.as_str())
                .unwrap_or("rank-prior-1");
            let rows = if let Some(arr) = args.get("rows").and_then(|v| v.as_array()) {
                arr.clone()
            } else if let Some(pool) = pool {
                let export = training_export(pool, &project_id(root)).await?;
                export
                    .get("rows")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let mut report = crate::learning::train_ranking_prior(
                &rows,
                ratio,
                seed,
                dataset_version,
                model_version,
            )?;
            report["applied"] = serde_json::json!(true);
            report["rowCount"] = serde_json::json!(rows.len());
            Ok(report)
        }
        "share" => {
            let cfg = load(root);
            let (models, counts) = if let Some(pool) = pool {
                let rows = list_models(pool).await?;
                let models = rows
                    .into_iter()
                    .map(|row| ModelManifest {
                        model_id: row.get("model_id"),
                        version: row.get("version"),
                        checksum: row.get("checksum"),
                        platforms: vec![row.get::<String, _>("platforms")],
                    })
                    .collect::<Vec<_>>();
                let counts = outcome_counts(pool, &project_id(root)).await?;
                (models, counts)
            } else {
                (Vec::new(), Vec::new())
            };
            let policy = SharePolicy {
                mode: &cfg.mode,
                models: cfg.distribution_enabled,
                aggregates: cfg.share_learning_data,
                endpoint: None,
            };
            Ok(packet_for(policy, &models, &counts))
        }
        "promote" => {
            let Some(pool) = pool else {
                return Err("no database".into());
            };
            promote(
                pool,
                &project_id(root),
                &required_str(args, "modelId")?,
                &required_str(args, "version")?,
                args.get("explicit")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                args.get("beatsBaseline")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                args.get("regressionsClear")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            )
            .await?;
            Ok(json!({ "promoted": true }))
        }
        "rollback" => {
            let Some(pool) = pool else {
                return Err("no database".into());
            };
            let version =
                rollback(pool, &project_id(root), &required_str(args, "modelId")?).await?;
            Ok(json!({ "rolledBackTo": version }))
        }
        "register" => {
            let Some(pool) = pool else {
                return Err("no database".into());
            };
            let metrics = args
                .get("metrics")
                .map(|value| value.to_string())
                .unwrap_or_else(|| "{}".into());
            let model_id = required_str(args, "modelId")?;
            let version = required_str(args, "version")?;
            let checksum = required_str(args, "checksum")?;
            register(
                pool,
                &ModelRecord {
                    model_id: &model_id,
                    version: &version,
                    model_type: args
                        .get("modelType")
                        .and_then(|v| v.as_str())
                        .unwrap_or("embedding"),
                    dataset_version: args
                        .get("datasetVersion")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unspecified"),
                    checksum: &checksum,
                    platforms: args
                        .get("platforms")
                        .and_then(|v| v.as_str())
                        .unwrap_or("untested"),
                    metrics_json: &metrics,
                },
            )
            .await?;
            Ok(json!({ "registered": true }))
        }
        "evaluate" => {
            let cfg = load(root);
            let mut report =
                crate::predict::evaluate_fixture(crate::CODING_FIXTURE, cfg.minimum_confidence)?;
            let stored = match pool {
                Some(pool) => {
                    record_evaluation(pool, &project_id(root), &report).await?;
                    true
                }
                None => false,
            };
            report["stored"] = json!(stored);
            Ok(report)
        }
        _ => Err(format!("unknown neural command: {tool}")),
    }
}

/// Measured numbers for status. A failed read leaves that metric unavailable.
async fn add_measured(value: &mut Value, pool: &SqlitePool, project: &str) {
    if let Ok(count) = feedback_count(pool, project).await {
        value["metrics"]["feedbackCount"] = json!({ "value": count, "kind": "measured" });
    }
    if let Ok(Some(totals)) = rank_totals(pool, project).await {
        if totals.preflights > 0 {
            value["metrics"]["tokenDelta"] = json!({
                "value": -totals.duplicate_tokens,
                "kind": "measured",
                "unit": "tokens",
                "preflights": totals.preflights
            });
            value["metrics"]["rankLatency"] = json!({
                "value": totals.rank_micros / totals.preflights,
                "kind": "measured",
                "unit": "microseconds",
                "preflights": totals.preflights
            });
        }
    }
    if let Ok(Some(evaluation)) = last_evaluation(pool, project).await {
        if !evaluation["precision"].is_null() {
            value["metrics"]["predictionAccuracy"] = json!({
                "value": evaluation["precision"],
                "kind": "measured",
                "source": evaluation["source"],
                "predictions": evaluation["predictions"]
            });
        }
    }
    if let Ok(models) = promoted_models(pool).await {
        value["promotedModels"] = json!(models);
    }
}

fn input_edges(args: &Value) -> Vec<InputEdge> {
    args.get("edges")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(InputEdge {
                        source: item.get("source")?.as_str()?.to_string(),
                        target: item.get("target")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn graph_reply(
    report: &GraphReport,
    edge_limit_reached: bool,
    feedback_rows: usize,
    stored: usize,
) -> Value {
    let limit_hit = edge_limit_reached || report.edge_limit_reached;
    let mut value = json!({
        "applied": true,
        "edgesRead": report.edges_read,
        "edgeLimitReached": limit_hit,
        "feedbackRows": feedback_rows,
        "skippedHubs": report.skipped_hubs,
        "truncated": report.truncated,
        "suggestions": report.suggestions,
        "stored": stored,
        "gnn": "not_in_binary",
        "rewroteEdges": false
    });
    if limit_hit {
        value["edgeLimit"] = json!(report.edge_limit.unwrap_or(EDGE_LIMIT));
    }
    if report.truncated {
        if let Some(limit) = report.candidate_pair_limit {
            value["candidatePairLimit"] = json!(limit);
        }
    }
    value
}

pub fn project_id(root: &Path) -> String {
    root.to_string_lossy().to_string()
}

fn required_str(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("{key} is required"))
}

fn string_list(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn blocks_from_json(blocks: &[Value]) -> Vec<crate::RankItem> {
    blocks
        .iter()
        .filter_map(|block| {
            let id = block.get("id")?.as_str()?.to_string();
            let text = block
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let class = match block
                .get("class")
                .and_then(|v| v.as_str())
                .unwrap_or("optional")
            {
                "hard_required" => crate::RankClass::HardRequired,
                "high_value" => crate::RankClass::HighValue,
                _ => crate::RankClass::Optional,
            };
            Some(crate::RankItem { id, class, text })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ax_memory::fusion::fuse_scores;

    #[tokio::test]
    async fn status_without_a_database_matches_the_required_report() {
        let dir = tempfile::tempdir().unwrap();
        let value = handle(dir.path(), "status", &json!({}), None)
            .await
            .unwrap();
        assert_eq!(value["enabled"], true);
        assert_eq!(value["model"], "hash-v1");
        assert_eq!(value["backend"], "hash");
        assert_eq!(value["semanticWeight"], 0.0);
        assert_eq!(value["productionDefaultsUnchanged"], true);
    }

    #[test]
    fn normalize_dispatch_tool_maps_hyphen_aliases_and_prefixes() {
        assert_eq!(normalize_dispatch_tool("benchmark-abc"), "benchmark_abc");
        assert_eq!(normalize_dispatch_tool("embed-bakeoff"), "embed_bakeoff");
        assert_eq!(normalize_dispatch_tool("model-status"), "model_status");
        assert_eq!(normalize_dispatch_tool("ax_neural_rank"), "rank");
        assert_eq!(
            normalize_dispatch_tool("ax-neural-model-list"),
            "model_list"
        );
        assert_eq!(normalize_dispatch_tool("status"), "status");
    }

    fn bakeoff_report_without_timing(mut value: Value) -> Value {
        if let Some(candidates) = value.get_mut("candidates").and_then(|v| v.as_array_mut()) {
            for candidate in candidates {
                if let Some(latency) = candidate.get_mut("latency") {
                    if let Some(kind) = latency.get("kind").cloned() {
                        *latency = json!({ "kind": kind });
                    }
                }
            }
        }
        value
    }

    #[tokio::test]
    async fn hyphen_and_underscore_tool_names_hit_the_same_handler() {
        let dir = tempfile::tempdir().unwrap();
        let hyphen = bakeoff_report_without_timing(
            handle(dir.path(), "embed-bakeoff", &json!({}), None)
                .await
                .unwrap(),
        );
        let underscore = bakeoff_report_without_timing(
            handle(dir.path(), "embed_bakeoff", &json!({}), None)
                .await
                .unwrap(),
        );
        assert_eq!(hyphen, underscore);
        let mcp = bakeoff_report_without_timing(
            handle(dir.path(), "ax_neural_embed_bakeoff", &json!({}), None)
                .await
                .unwrap(),
        );
        assert_eq!(hyphen, mcp);
    }

    #[test]
    fn semantic_blend_does_not_move_the_locked_fusion_score() {
        let fused = fuse_scores(0.4, 0.2, 0.1, 0.0, 0.0);
        assert!((crate::blend_semantic(fused, 1.0) - fused).abs() < f64::EPSILON);
    }

    #[test]
    fn a_zero_semantic_score_leaves_the_fused_score_unchanged() {
        let fused = fuse_scores(0.4, 0.2, 0.1, 0.0, 0.0);
        assert_eq!(crate::blend_semantic(fused, 0.0), fused);
    }

    async fn open(dir: &Path) -> ax_db::Database {
        ax_db::Database::open(&dir.join("neural-test.db"))
            .await
            .expect("temp database")
    }

    async fn add_node(pool: &SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language,
                start_line, end_line, start_column, end_column, updated_at)
             VALUES (?, 'function', ?, ?, 'src/lib.rs', 'rust', 1, 1, 0, 0, 0)",
        )
        .bind(id)
        .bind(id)
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn add_edge(pool: &SqlitePool, source: &str, target: &str, kind: &str) {
        sqlx::query("INSERT INTO edges (source, target, kind) VALUES (?, ?, ?)")
            .bind(source)
            .bind(target)
            .bind(kind)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn count(pool: &SqlitePool, sql: &str) -> i64 {
        sqlx::query(sql).fetch_one(pool).await.unwrap().get(0)
    }

    fn pairs(value: &Value) -> Vec<(String, String)> {
        value["suggestions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| {
                let a = item["source"].as_str().unwrap().to_string();
                let b = item["target"].as_str().unwrap().to_string();
                if a <= b {
                    (a, b)
                } else {
                    (b, a)
                }
            })
            .collect()
    }

    #[tokio::test]
    async fn graph_reads_project_edges_and_writes_no_edges() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("ax.json"),
            r#"{"neural":{"graph":{"enabled":true}}}"#,
        )
        .unwrap();
        let database = open(dir.path()).await;
        let pool = database.pool();
        for id in ["a", "b", "c", "d", "hub", "file"] {
            add_node(pool, id).await;
        }
        add_edge(pool, "a", "hub", "calls").await;
        add_edge(pool, "b", "hub", "calls").await;
        add_edge(pool, "c", "hub", "references").await;
        add_edge(pool, "a", "b", "calls").await;
        add_edge(pool, "file", "a", "contains").await;
        add_edge(pool, "file", "d", "contains").await;
        let before = count(pool, "SELECT COUNT(*) FROM edges").await;

        let value = handle(dir.path(), "graph", &json!({}), Some(pool))
            .await
            .unwrap();
        assert_eq!(value["applied"], true);
        assert_eq!(value["edgesRead"], 4);
        assert_eq!(value["rewroteEdges"], false);
        let found = pairs(&value);
        assert!(found.contains(&("a".into(), "c".into())), "{found:?}");
        assert!(
            found
                .iter()
                .all(|(a, b)| a != "d" && b != "d" && a != "file" && b != "file"),
            "contains edges are not read: {found:?}"
        );
        assert_eq!(count(pool, "SELECT COUNT(*) FROM edges").await, before);
        let stored = count(pool, "SELECT COUNT(*) FROM neural_graph_suggestion").await;
        assert_eq!(stored as usize, found.len());
    }

    #[tokio::test]
    async fn passed_edges_are_a_preview_that_stores_nothing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("ax.json"),
            r#"{"neural":{"graph":{"enabled":true}}}"#,
        )
        .unwrap();
        let database = open(dir.path()).await;
        let pool = database.pool();
        let edges = json!({"edges": [
            {"source": "a", "target": "hub"},
            {"source": "c", "target": "hub"}
        ]});

        let value = handle(dir.path(), "graph", &edges, Some(pool))
            .await
            .unwrap();
        assert!(pairs(&value).contains(&("a".into(), "c".into())));
        assert_eq!(value["stored"], 0);
        assert_eq!(
            count(pool, "SELECT COUNT(*) FROM neural_graph_suggestion").await,
            0
        );
    }

    #[tokio::test]
    async fn export_is_not_an_mcp_command() {
        let dir = tempfile::tempdir().unwrap();
        let database = open(dir.path()).await;
        let error = handle(
            dir.path(),
            "ax_neural_export",
            &json!({}),
            Some(database.pool()),
        )
        .await
        .unwrap_err();
        assert!(error.contains("unknown neural command"), "{error}");
    }

    #[tokio::test]
    async fn status_without_rank_data_is_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        let database = open(dir.path()).await;
        let status = handle(dir.path(), "status", &json!({}), Some(database.pool()))
            .await
            .unwrap();
        let metrics = &status["metrics"];
        assert_eq!(metrics["feedbackCount"]["kind"], "measured");
        assert_eq!(metrics["tokenDelta"]["kind"], "unavailable");
        assert_eq!(metrics["rankLatency"]["kind"], "unavailable");
        assert_eq!(metrics["predictionAccuracy"]["kind"], "unavailable");
        assert_eq!(status["promotedModels"], json!([]));
    }

    #[tokio::test]
    async fn a_rank_row_with_zero_preflights_is_unavailable_and_divides_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let database = open(dir.path()).await;
        let pool = database.pool();
        sqlx::query(
            "INSERT INTO neural_rank_total (
                project_id, preflights, duplicate_blocks, duplicate_tokens, rank_micros, updated_at
            ) VALUES (?, 0, 0, 0, 0, 0)",
        )
        .bind(project_id(dir.path()))
        .execute(pool)
        .await
        .unwrap();

        let status = handle(dir.path(), "status", &json!({}), Some(pool))
            .await
            .unwrap();
        assert_eq!(status["metrics"]["tokenDelta"]["kind"], "unavailable");
        assert_eq!(status["metrics"]["rankLatency"]["kind"], "unavailable");
    }

    #[tokio::test]
    async fn status_reports_measured_metrics() {
        let dir = tempfile::tempdir().unwrap();
        let database = open(dir.path()).await;
        let pool = database.pool();
        let project = project_id(dir.path());
        crate::store::record_rank(pool, &project, 1, 120, 300)
            .await
            .unwrap();
        crate::store::record_rank(pool, &project, 1, 80, 100)
            .await
            .unwrap();
        handle(dir.path(), "evaluate", &json!({}), Some(pool))
            .await
            .unwrap();
        let model = ModelRecord {
            model_id: "candidate",
            version: "1",
            model_type: "embedding",
            dataset_version: "ds",
            checksum: "abc",
            platforms: "macos-arm64",
            metrics_json: "{}",
        };
        register(pool, &model).await.unwrap();
        crate::store::promote(pool, &project, "candidate", "1", true, true, true)
            .await
            .unwrap();

        let status = handle(dir.path(), "status", &json!({}), Some(pool))
            .await
            .unwrap();
        let metrics = &status["metrics"];
        assert_eq!(
            metrics["tokenDelta"],
            json!({ "value": -200, "kind": "measured", "unit": "tokens", "preflights": 2 })
        );
        assert_eq!(
            metrics["rankLatency"],
            json!({ "value": 200, "kind": "measured", "unit": "microseconds", "preflights": 2 })
        );
        assert_eq!(metrics["predictionAccuracy"]["kind"], "measured");
        assert_eq!(metrics["predictionAccuracy"]["value"], 1.0);
        assert_eq!(
            metrics["predictionAccuracy"]["source"],
            "coding fixture, leave-one-out"
        );
        assert_eq!(
            status["promotedModels"],
            json!([{ "modelId": "candidate", "version": "1", "checksum": "abc" }])
        );
    }

    #[tokio::test]
    async fn evaluate_runs_the_held_out_fixture() {
        let dir = tempfile::tempdir().unwrap();
        let value = handle(dir.path(), "evaluate", &json!({}), None)
            .await
            .unwrap();
        assert_eq!(value["kind"], "measured");
        assert_eq!(value["method"], "leave-one-out");
        assert_eq!(value["predictions"], 5);
        assert_eq!(value["stored"], false);
    }

    #[tokio::test]
    async fn share_follows_the_distribution_and_share_learning_keys() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("ax.json"),
            r#"{"neural":{"mode":"hybrid","distribution":{"enabled":true}}}"#,
        )
        .unwrap();
        let database = open(dir.path()).await;
        let model = ModelRecord {
            model_id: "candidate",
            version: "1",
            model_type: "embedding",
            dataset_version: "ds",
            checksum: "abc",
            platforms: "macos-arm64",
            metrics_json: "{}",
        };
        register(database.pool(), &model).await.unwrap();
        let value = handle(dir.path(), "share", &json!({}), Some(database.pool()))
            .await
            .unwrap();
        assert_eq!(value["mode"], "hybrid");
        assert_eq!(value["sent"], false);
        assert_eq!(value["models"].as_array().unwrap().len(), 1);
        assert_eq!(value["aggregates"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn project_edges_say_when_more_edges_exist_than_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let database = open(dir.path()).await;
        let pool = database.pool();
        for id in ["a", "b", "c", "d"] {
            add_node(pool, id).await;
        }
        for (source, target) in [("a", "b"), ("b", "c"), ("c", "d")] {
            add_edge(pool, source, target, "calls").await;
        }
        let (edges, more) = project_edges(pool, 2).await.unwrap();
        assert_eq!(edges.len(), 2);
        assert!(more);
        let (edges, more) = project_edges(pool, 3).await.unwrap();
        assert_eq!(edges.len(), 3);
        assert!(!more);
    }

    #[tokio::test]
    async fn benchmark_runs_the_coding_fixture_on_a_temporary_database() {
        let dir = tempfile::tempdir().unwrap();
        let database = open(dir.path()).await;
        let value = handle(dir.path(), "benchmark", &json!({}), Some(database.pool()))
            .await
            .unwrap();
        if crate::runtime_built() {
            assert_eq!(value["winner"], crate::lexical::CHALLENGER_ID);
            assert_eq!(value["challengerAdmitted"], true);
        } else {
            assert_eq!(value["winner"], "hash-v1");
            assert_eq!(value["challengerAdmitted"], false);
        }
        assert_eq!(value["database"], "temporary");
        assert_eq!(value["openedLiveDatabase"], false);
        assert_eq!(value["families"]["named_trigger"]["hits"], 1);
    }

    #[tokio::test]
    async fn benchmark_abc_reports_three_arms_without_live_database() {
        let dir = tempfile::tempdir().unwrap();
        let value = handle(dir.path(), "benchmark-abc", &json!({}), None)
            .await
            .unwrap();
        assert_eq!(value["kind"], "measured");
        assert_eq!(value["productionRankingUnchanged"], true);
        assert_eq!(value["semanticWeight"], 0.0);
        assert_eq!(value["arms"]["C_ax_neural"]["claimsNeuralInference"], false);
        assert!(
            value["arms"]["A_ax_without_neural_ranking"]["locked"]["questionsScored"]
                .as_u64()
                .unwrap()
                >= 1
        );
        assert_eq!(value["openedLiveDatabase"], false);
    }

    #[tokio::test]
    async fn embed_bakeoff_reports_not_built_neural_runtimes() {
        let dir = tempfile::tempdir().unwrap();
        let value = handle(dir.path(), "embed-bakeoff", &json!({}), None)
            .await
            .unwrap();
        assert_eq!(value["kind"], "measured");
        assert_eq!(value["semanticWeight"], 0.0);
        #[cfg(not(feature = "neural-ort"))]
        assert_eq!(value["neuralEmbeddingAdmitted"], false);
        let ids: Vec<&str> = value["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|c| c["id"].as_str())
            .collect();
        assert!(ids.contains(&"onnx-runtime-cpu"));
        assert!(ids.contains(&"candle-cpu"));
    }

    #[tokio::test]
    async fn projects_sharing_a_database_keep_their_own_feedback() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        for dir in [&first, &second] {
            std::fs::write(
                dir.path().join("ax.json"),
                r#"{"neural":{"prediction":{"enabled":true},"learning":{"feedback_collection":true}}}"#,
            )
            .unwrap();
        }
        let database = open(first.path()).await;
        let pool = database.pool();
        parser_feedback(first.path(), pool).await;
        let task = json!({ "task": "fix the parser" });
        let mine = handle(first.path(), "predict", &task, Some(pool))
            .await
            .unwrap();
        assert!(mine["predictions"].as_array().unwrap().len() >= 3);
        let theirs = handle(second.path(), "predict", &task, Some(pool))
            .await
            .unwrap();
        assert_eq!(theirs["predictions"], json!([]));
        let status = handle(second.path(), "status", &json!({}), Some(pool))
            .await
            .unwrap();
        assert_eq!(status["metrics"]["feedbackCount"]["value"], 0);
        assert_eq!(project_id(first.path()), first.path().to_string_lossy());
    }

    #[tokio::test]
    async fn rank_tool_maps_class_names_and_drops_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let value = handle(
            dir.path(),
            "ax_neural_rank",
            &json!({
                "query": "memory hash",
                "blocks": [
                    { "id": "opt-match", "class": "optional", "text": "memory hash vectors" },
                    { "id": "high-unrelated", "class": "high_value", "text": "release checklist" },
                    { "id": "required", "class": "hard_required", "text": "policy" },
                    { "id": "copy", "text": "memory hash vectors" }
                ]
            }),
            None,
        )
        .await
        .unwrap();
        assert_eq!(value["applied"], true);
        assert_eq!(
            value["order"],
            json!(["required", "high-unrelated", "opt-match"])
        );
        let notes = value["notes"].as_array().unwrap();
        let copy = notes.iter().find(|note| note["id"] == "copy").unwrap();
        assert_eq!(copy["reason"], "duplicate");
        assert_eq!(copy["keep"], false);
    }

    async fn outcome_feedback(
        root: &Path,
        pool: &SqlitePool,
        execution: &str,
        outcome: &str,
        context: Value,
    ) {
        handle(
            root,
            "feedback",
            &json!({
                "executionId": execution,
                "taskCategory": "parser",
                "outcome": outcome,
                "contextIds": context
            }),
            Some(pool),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn share_and_predict_report_a_failed_database_read() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("ax.json"),
            r#"{"neural":{"prediction":{"enabled":true},"distribution":{"enabled":true,"share_learning_data":true},"learning":{"feedback_collection":true}}}"#,
        )
        .unwrap();
        let database = open(dir.path()).await;
        let pool = database.pool();
        sqlx::query("DROP TABLE neural_model")
            .execute(pool)
            .await
            .unwrap();
        let error = handle(dir.path(), "share", &json!({}), Some(pool))
            .await
            .unwrap_err();
        assert!(error.contains("neural_model"), "{error}");

        sqlx::query("DROP TABLE edges").execute(pool).await.unwrap();
        let error = handle(dir.path(), "predict", &json!({ "task": "x" }), Some(pool))
            .await
            .unwrap_err();
        assert!(error.contains("edges"), "{error}");
    }

    #[tokio::test]
    async fn share_counts_the_outcomes_of_this_projects_feedback_only() {
        let dir = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("ax.json"),
            r#"{"neural":{"distribution":{"share_learning_data":true},"learning":{"feedback_collection":true}}}"#,
        )
        .unwrap();
        std::fs::write(
            other.path().join("ax.json"),
            r#"{"neural":{"learning":{"feedback_collection":true}}}"#,
        )
        .unwrap();
        let database = open(dir.path()).await;
        let pool = database.pool();
        for (execution, outcome) in [("e1", "success"), ("e2", "success"), ("e3", "failure")] {
            outcome_feedback(dir.path(), pool, execution, outcome, json!([])).await;
        }
        outcome_feedback(other.path(), pool, "e4", "partial", json!([])).await;
        let value = handle(dir.path(), "share", &json!({}), Some(pool))
            .await
            .unwrap();
        let mut aggregates = value["aggregates"].as_array().unwrap().clone();
        aggregates.sort_by_key(|item| item["outcome"].to_string());
        assert_eq!(
            aggregates,
            vec![
                json!({ "outcome": "failure", "count": 1 }),
                json!({ "outcome": "success", "count": 2 })
            ]
        );
        assert_eq!(value["rawFeedback"], false);
    }

    #[tokio::test]
    async fn share_omits_secret_shaped_feedback_from_the_packet() {
        use crate::learning::FeedbackRow;
        use crate::store::insert_feedback;

        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("ax.json"),
            r#"{"neural":{"distribution":{"enabled":true,"share_learning_data":true},"learning":{"feedback_collection":true}}}"#,
        )
        .unwrap();
        let database = open(dir.path()).await;
        let pool = database.pool();
        let project = project_id(dir.path());
        outcome_feedback(dir.path(), pool, "e1", "success", json!([])).await;
        let secret_row = FeedbackRow {
            id: "leaked-row".into(),
            project_id: project.clone(),
            execution_id: "exec-leak-42".into(),
            task_category: "memory".into(),
            context_ids: r#"["password=hunter2"]"#.into(),
            skill_ids: "[]".into(),
            rule_ids: "[]".into(),
            outcome: "success".into(),
            context_attribution: "unverified".into(),
            retry_count: 0,
            input_tokens: 1,
            output_tokens: 1,
        };
        insert_feedback(pool, &secret_row).await.unwrap();
        let value = handle(dir.path(), "share", &json!({}), Some(pool))
            .await
            .unwrap();
        assert_eq!(value["rawFeedback"], false);
        assert_eq!(value["aggregates"][0]["count"], 2);
        let text = value.to_string();
        assert!(!text.contains("hunter2"), "{text}");
        assert!(!text.contains("exec-leak-42"), "{text}");
        assert!(!text.contains("password="), "{text}");
        assert!(value.get("rows").is_none());
    }

    #[tokio::test]
    async fn graph_suggests_context_ids_that_succeed_together() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("ax.json"),
            r#"{"neural":{"graph":{"enabled":true},"learning":{"feedback_collection":true}}}"#,
        )
        .unwrap();
        let database = open(dir.path()).await;
        let pool = database.pool();
        for execution in ["e1", "e2"] {
            let context = json!(["memory:a", "rule:b"]);
            outcome_feedback(dir.path(), pool, execution, "success", context).await;
        }
        for execution in ["e3", "e4"] {
            let context = json!(["memory:x", "rule:y"]);
            outcome_feedback(dir.path(), pool, execution, "failure", context).await;
        }
        let value = handle(dir.path(), "graph", &json!({}), Some(pool))
            .await
            .unwrap();
        assert_eq!(value["feedbackRows"], 2);
        let pairs: Vec<(&str, &str)> = value["suggestions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| {
                (
                    item["source"].as_str().unwrap(),
                    item["target"].as_str().unwrap(),
                )
            })
            .collect();
        assert_eq!(pairs, vec![("memory:a", "rule:b")]);
    }

    async fn parser_feedback(root: &Path, pool: &SqlitePool) {
        for execution in ["e1", "e2"] {
            handle(
                root,
                "feedback",
                &json!({
                    "executionId": execution,
                    "taskCategory": "parser",
                    "outcome": "success",
                    "skillIds": ["parse-errors"],
                    "ruleIds": ["english-only"],
                    "contextIds": ["memory:parser-lines"],
                    "retryCount": 1,
                    "inputTokens": 300,
                    "outputTokens": 40
                }),
                Some(pool),
            )
            .await
            .unwrap();
        }
    }

    #[tokio::test]
    async fn feedback_tool_stores_the_cli_field_names() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("ax.json"),
            r#"{"neural":{"learning":{"feedback_collection":true}}}"#,
        )
        .unwrap();
        let database = open(dir.path()).await;
        parser_feedback(dir.path(), database.pool()).await;
        let row = sqlx::query(
            "SELECT task_category, skill_ids, rule_ids, context_ids, retry_count, input_tokens,
                    output_tokens
             FROM neural_feedback WHERE execution_id = 'e1'",
        )
        .fetch_one(database.pool())
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("task_category"), "parser");
        assert_eq!(row.get::<String, _>("skill_ids"), r#"["parse-errors"]"#);
        assert_eq!(row.get::<String, _>("rule_ids"), r#"["english-only"]"#);
        assert_eq!(
            row.get::<String, _>("context_ids"),
            r#"["memory:parser-lines"]"#
        );
        assert_eq!(row.get::<i64, _>("retry_count"), 1);
        assert_eq!(row.get::<i64, _>("input_tokens"), 300);
        assert_eq!(row.get::<i64, _>("output_tokens"), 40);
    }

    #[tokio::test]
    async fn feedback_needs_collection_on_a_database_and_the_required_fields() {
        let dir = tempfile::tempdir().unwrap();
        let args = json!({ "executionId": "e1", "taskCategory": "parser", "outcome": "success" });
        let off = handle(dir.path(), "feedback", &args, None)
            .await
            .unwrap_err();
        assert_eq!(off, "feedback collection is disabled");
        std::fs::write(
            dir.path().join("ax.json"),
            r#"{"neural":{"learning":{"feedback_collection":true}}}"#,
        )
        .unwrap();
        let no_database = handle(dir.path(), "feedback", &args, None)
            .await
            .unwrap_err();
        assert_eq!(no_database, "no database");
        let database = open(dir.path()).await;
        let missing = handle(
            dir.path(),
            "feedback",
            &json!({ "executionId": "e1", "outcome": "success" }),
            Some(database.pool()),
        )
        .await
        .unwrap_err();
        assert_eq!(missing, "taskCategory is required");
    }

    #[tokio::test]
    async fn refused_feedback_and_predict_name_the_config_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("ax.json"),
            r#"{"neural":{"learning":{"feedback_colection":true}}}"#,
        )
        .unwrap();
        let args = json!({ "executionId": "e1", "taskCategory": "parser", "outcome": "success" });
        let error = handle(dir.path(), "feedback", &args, None)
            .await
            .unwrap_err();
        assert!(
            error.starts_with("feedback collection is disabled; ax.json: "),
            "{error}"
        );
        assert!(error.contains("feedback_colection"), "{error}");
        let off = handle(dir.path(), "predict", &json!({ "task": "x" }), None)
            .await
            .unwrap();
        assert_eq!(off["applied"], false);
        assert!(
            off["configError"]
                .as_str()
                .is_some_and(|text| text.contains("feedback_colection")),
            "{off}"
        );
    }

    #[tokio::test]
    async fn predict_tool_labels_predictions_from_this_projects_feedback() {
        let dir = tempfile::tempdir().unwrap();
        let task = json!({ "task": "fix the parser" });
        let off = handle(dir.path(), "predict", &task, None).await.unwrap();
        assert_eq!(off, json!({ "applied": false, "predictions": [] }));
        std::fs::write(
            dir.path().join("ax.json"),
            r#"{"neural":{"prediction":{"enabled":true},"learning":{"feedback_collection":true}}}"#,
        )
        .unwrap();
        let no_database = handle(dir.path(), "predict", &task, None).await.unwrap();
        assert_eq!(no_database["applied"], false);
        assert_eq!(no_database["reason"], "no database");

        let database = open(dir.path()).await;
        parser_feedback(dir.path(), database.pool()).await;
        let value = handle(dir.path(), "predict", &task, Some(database.pool()))
            .await
            .unwrap();
        assert_eq!(value["applied"], true);
        assert_eq!(value["label"], "predicted");
        let preds = value["predictions"].as_array().unwrap();
        for item in preds {
            assert_eq!(item["label"], "predicted");
            assert!(!item["explanation"].as_str().unwrap().is_empty());
            assert!(!item["evidence"].as_array().unwrap().is_empty());
            let rel = item["relationship"].as_str().unwrap();
            assert!(rel == "inferred" || rel == "verified");
        }
        let mut predicted: Vec<(String, String)> = preds
            .iter()
            .map(|item| {
                (
                    item["kind"].as_str().unwrap().to_string(),
                    item["target_id"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        predicted.sort();
        assert!(predicted.contains(&("memory".into(), "parser-lines".into())));
        assert!(predicted.contains(&("rule".into(), "english-only".into())));
        assert!(predicted.contains(&("skill".into(), "parse-errors".into())));
        assert!(predicted.iter().any(|(k, _)| k == "pattern"));
    }

    #[tokio::test]
    async fn explain_and_search_report_hash_v1() {
        let dir = tempfile::tempdir().unwrap();
        let explain = handle(dir.path(), "explain", &json!({}), None)
            .await
            .unwrap();
        assert_eq!(explain["enabled"], true);
        assert_eq!(explain["memoryModel"], "hash-v1");
        assert_eq!(explain["semanticWeight"], 0.0);
        assert_eq!(
            explain["fusionWeights"],
            json!({
                "fts": ax_memory::fusion::W_FTS,
                "vector": ax_memory::fusion::W_VECTOR,
                "actR": ax_memory::fusion::W_ACT_R,
                "graph": ax_memory::fusion::W_GRAPH,
                "usage": ax_memory::fusion::W_USAGE
            })
        );
        assert_eq!(explain["configError"], Value::Null);

        let fallback = json!({
            "enabled": true,
            "hits": [],
            "fallback": "hash-v1",
            "scannedMemoryColumn": false
        });
        let search = json!({ "query": "parser" });
        assert_eq!(
            handle(dir.path(), "search", &search, None).await.unwrap(),
            fallback
        );
        let database = open(dir.path()).await;
        let stored = handle(dir.path(), "search", &search, Some(database.pool()))
            .await
            .unwrap();
        assert_eq!(stored["hits"], json!([]));
        assert_eq!(stored["fallback"], "hash-v1");
        assert_eq!(stored["storedVectors"], 0);
        assert_eq!(stored["scannedMemoryColumn"], false);
        assert_eq!(
            stored["challengerAdmitted"],
            crate::bakeoff::challenger_admitted()
        );
        // The only built-in runtime backend is a text hash, so search never
        // reports it as the model that ranked the hits.
        assert_eq!(stored["model"], "hash-v1");
        std::fs::write(
            dir.path().join("ax.json"),
            r#"{"neural":{"semantic":{"enabled":false}}}"#,
        )
        .unwrap();
        assert_eq!(
            handle(dir.path(), "search", &search, Some(database.pool()))
                .await
                .unwrap(),
            fallback
        );
    }

    fn register_args(version: &str, checksum: &str) -> Value {
        json!({
            "modelId": "candidate",
            "version": version,
            "checksum": checksum,
            "modelType": "reranker",
            "datasetVersion": "coding-v1",
            "platforms": "macos-arm64",
            "metrics": { "precision": 0.8 }
        })
    }

    fn promote_args(version: &str) -> Value {
        json!({
            "modelId": "candidate",
            "version": version,
            "explicit": true,
            "beatsBaseline": true,
            "regressionsClear": true
        })
    }

    #[tokio::test]
    async fn model_commands_without_a_database() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let empty = json!({});
        let listed = handle(root, "model_list", &empty, None).await.unwrap();
        assert_eq!(listed["models"], json!([]));
        assert!(listed.get("bakeOffChallenger").is_some() || listed.get("admissionNote").is_some());
        assert!(listed["status"].as_str().is_some());
        assert_eq!(
            handle(root, "model_status", &empty, None).await.unwrap(),
            json!({ "model": "hash-v1", "promoted": null })
        );
        for (command, args) in [
            ("register", register_args("1", "sha256:aaa")),
            ("promote", promote_args("1")),
            ("rollback", json!({ "modelId": "candidate" })),
        ] {
            let error = handle(root, command, &args, None).await.unwrap_err();
            assert_eq!(error, "no database", "{command}");
        }
    }

    #[tokio::test]
    async fn register_stores_every_tool_field() {
        let dir = tempfile::tempdir().unwrap();
        let database = open(dir.path()).await;
        let pool = database.pool();
        let value = handle(
            dir.path(),
            "register",
            &register_args("1", "sha256:aaa"),
            Some(pool),
        )
        .await
        .unwrap();
        assert_eq!(value, json!({ "registered": true }));
        let row = sqlx::query(
            "SELECT model_id, version, checksum, model_type, dataset_version, platforms,
                    metrics_json, promoted
             FROM neural_model",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("model_id"), "candidate");
        assert_eq!(row.get::<String, _>("version"), "1");
        assert_eq!(row.get::<String, _>("checksum"), "sha256:aaa");
        assert_eq!(row.get::<String, _>("model_type"), "reranker");
        assert_eq!(row.get::<String, _>("dataset_version"), "coding-v1");
        assert_eq!(row.get::<String, _>("platforms"), "macos-arm64");
        let metrics: Value = serde_json::from_str(&row.get::<String, _>("metrics_json")).unwrap();
        assert_eq!(metrics, json!({ "precision": 0.8 }));
        assert_eq!(row.get::<i64, _>("promoted"), 0);

        let missing = handle(
            dir.path(),
            "register",
            &json!({ "modelId": "candidate", "version": "2" }),
            Some(pool),
        )
        .await
        .unwrap_err();
        assert_eq!(missing, "checksum is required");
    }

    #[tokio::test]
    async fn promote_and_rollback_move_the_promoted_checksum() {
        let dir = tempfile::tempdir().unwrap();
        let database = open(dir.path()).await;
        let pool = database.pool();
        let root = dir.path();
        for (version, checksum) in [("1", "sha256:aaa"), ("2", "sha256:bbb")] {
            handle(
                root,
                "register",
                &register_args(version, checksum),
                Some(pool),
            )
            .await
            .unwrap();
        }
        let unasked = handle(
            root,
            "promote",
            &json!({ "modelId": "candidate", "version": "1" }),
            Some(pool),
        )
        .await;
        assert!(unasked.is_err(), "promotion without explicit must fail");

        for version in ["1", "2"] {
            let value = handle(root, "promote", &promote_args(version), Some(pool))
                .await
                .unwrap();
            assert_eq!(value, json!({ "promoted": true }));
        }
        let model = json!({ "modelId": "candidate" });
        let status = handle(root, "model_status", &model, Some(pool))
            .await
            .unwrap();
        assert_eq!(
            status,
            json!({
                "memoryModel": "hash-v1",
                "modelId": "candidate",
                "promotedChecksum": "sha256:bbb"
            })
        );
        let back = handle(root, "rollback", &model, Some(pool)).await.unwrap();
        assert_eq!(back, json!({ "rolledBackTo": "1" }));
        let status = handle(root, "model_status", &model, Some(pool))
            .await
            .unwrap();
        assert_eq!(status["promotedChecksum"], "sha256:aaa");

        let list = handle(root, "models", &json!({}), Some(pool))
            .await
            .unwrap();
        let mut models: Vec<(String, String, bool)> = list["models"]
            .as_array()
            .unwrap()
            .iter()
            .map(|model| {
                assert_eq!(model["modelId"], "candidate");
                assert_eq!(model["platforms"], "macos-arm64");
                (
                    model["version"].as_str().unwrap().to_string(),
                    model["checksum"].as_str().unwrap().to_string(),
                    model["promoted"].as_bool().unwrap(),
                )
            })
            .collect();
        models.sort();
        assert_eq!(
            models,
            vec![
                ("1".to_string(), "sha256:aaa".to_string(), true),
                ("2".to_string(), "sha256:bbb".to_string(), false),
            ]
        );
        if crate::runtime_built() && crate::bakeoff::challenger_admitted() {
            assert_eq!(list["bakeOffChallenger"], crate::lexical::CHALLENGER_ID);
        } else {
            assert!(list.get("admissionNote").is_some());
        }
        assert!(
            list.get("bakeOffChallenger").is_none()
                || list["models"].as_array().unwrap().iter().all(|model| {
                    model.get("modelId").and_then(|v| v.as_str())
                        != Some(crate::lexical::CHALLENGER_ID)
                })
        );
    }

    #[tokio::test]
    async fn models_lists_bake_off_challenger_without_promoting() {
        let dir = tempfile::tempdir().unwrap();
        let value = handle(dir.path(), "models", &json!({}), None)
            .await
            .unwrap();
        assert_eq!(value["models"], json!([]));
        if crate::runtime_built() && crate::bakeoff::challenger_admitted() {
            assert_eq!(value["bakeOffChallenger"], crate::lexical::CHALLENGER_ID);
        } else {
            assert!(value.get("admissionNote").is_some());
        }
        assert!(value["status"].as_str().unwrap_or("").len() > 5);
    }

    #[tokio::test]
    async fn graph_is_off_until_enabled_and_then_needs_edges_or_a_database() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        assert_eq!(
            handle(root, "graph", &json!({}), None).await.unwrap(),
            json!({ "applied": false, "suggestions": [], "gnn": "not_in_binary" })
        );
        std::fs::write(
            root.join("ax.json"),
            r#"{"neural":{"graph":{"enabled":true}}}"#,
        )
        .unwrap();
        let error = handle(root, "graph", &json!({}), None).await.unwrap_err();
        assert_eq!(error, "no database");
    }

    #[tokio::test]
    async fn feedback_with_source_text_is_refused_and_not_stored() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("ax.json"),
            r#"{"neural":{"learning":{"feedback_collection":true}}}"#,
        )
        .unwrap();
        let database = open(dir.path()).await;
        let args = json!({
            "executionId": "e1",
            "taskCategory": "parser",
            "outcome": "success",
            "sourceText": "fn parse() {}"
        });
        let error = handle(dir.path(), "feedback", &args, Some(database.pool()))
            .await
            .unwrap_err();
        assert_eq!(error, "source text is not stored");
        let rows = count(database.pool(), "SELECT COUNT(*) FROM neural_feedback").await;
        assert_eq!(rows, 0);
    }

    #[tokio::test]
    async fn share_without_a_database_has_no_models_or_counts() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("ax.json"),
            r#"{"neural":{"distribution":{"enabled":true,"share_learning_data":true}}}"#,
        )
        .unwrap();
        let value = handle(dir.path(), "share", &json!({}), None).await.unwrap();
        assert_eq!(value["sent"], false);
        assert_eq!(value["models"], json!([]));
        assert_eq!(value["aggregates"], json!([]));
    }

    #[tokio::test]
    async fn model_status_without_a_model_id_names_no_checksum() {
        let dir = tempfile::tempdir().unwrap();
        let database = open(dir.path()).await;
        let value = handle(
            dir.path(),
            "model_status",
            &json!({}),
            Some(database.pool()),
        )
        .await
        .unwrap();
        assert_eq!(
            value,
            json!({ "memoryModel": "hash-v1", "modelId": "", "promotedChecksum": null })
        );
    }
}
