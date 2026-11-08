//! `ax turn` — explicit agent turn lifecycle (no IDE hooks required).

use std::io::Read;

use serde_json::{json, Value};

use crate::commands::preflight::invoke_shared_tool;
use crate::commands::resolve_path;

#[derive(Debug, Clone)]
pub enum TurnAction {
    Begin,
    Event,
    Checkpoint,
    Finish,
    Fail,
    Status,
}

pub struct TurnArgs {
    pub action: TurnAction,
    pub session: String,
    pub turn: Option<String>,
    pub path: Option<String>,
    pub prompt_stdin: bool,
    pub input_stdin: bool,
    pub outcome_stdin: bool,
    pub reason_stdin: bool,
    pub agent: Option<String>,
    pub json: bool,
}

pub async fn run(args: TurnArgs) -> Result<(), String> {
    let root = resolve_path(args.path.clone());
    match args.action {
        TurnAction::Begin => {
            let prompt = if args.prompt_stdin {
                read_stdin()?
            } else {
                String::new()
            };
            let snap =
                ax_usage::turn_begin(&root, &args.session, &prompt, args.agent.as_deref()).await?;

            // Shared preflight after turn begin.
            let preflight = invoke_shared_tool(
                &root,
                "ax_preflight",
                json!({
                    "prompt": prompt,
                    "session": args.session,
                }),
            )
            .await
            .unwrap_or_else(|e| json!({"error": e}));

            let _ = ax_usage::record_activity(
                &root,
                ax_usage::ActivityRecord {
                    idempotency_key: format!("cli.context.loaded:{}", snap.turn_id),
                    source: Some(ax_usage::ActivitySource::Cli),
                    session_id: Some(args.session.clone()),
                    turn_id: Some(snap.turn_id.clone()),
                    operation: "context.loaded".into(),
                    status: Some(ax_usage::ActivityStatus::Success),
                    summary: "preflight after turn begin".into(),
                    ..ax_usage::ActivityRecord::default()
                },
            )
            .await;

            print_json_or(
                args.json,
                &json!({
                    "turnId": snap.turn_id,
                    "sessionId": snap.session_id,
                    "state": "open",
                    "preflight": preflight,
                }),
                &format!("turn={} session={} state=open", snap.turn_id, snap.session_id),
            )?;
        }
        TurnAction::Event => {
            let turn_id = require_turn(&args)?;
            let input = parse_event_input(args.input_stdin)?;
            let snap = ax_usage::turn_event(&root, &args.session, &turn_id, input).await?;
            print_json_or(
                args.json,
                &serde_json::to_value(&snap).map_err(|e| e.to_string())?,
                &format!("turn={} events={}", snap.turn_id, snap.events.len()),
            )?;
        }
        TurnAction::Checkpoint => {
            let turn_id = require_turn(&args)?;
            let note = if args.input_stdin {
                read_stdin()?
            } else {
                String::new()
            };
            let snap = ax_usage::turn_checkpoint(&root, &args.session, &turn_id, &note).await?;
            print_json_or(
                args.json,
                &serde_json::to_value(&snap).map_err(|e| e.to_string())?,
                &format!("turn={} checkpointed", snap.turn_id),
            )?;
        }
        TurnAction::Finish => {
            let turn_id = require_turn(&args)?;
            let finish = parse_finish_input(args.outcome_stdin || args.input_stdin)?;
            let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
            let receipt =
                ax_usage::turn_finish(&root, ax.db_pool(), &args.session, &turn_id, finish).await?;
            print_json_or(
                args.json,
                &serde_json::to_value(&receipt).map_err(|e| e.to_string())?,
                &format!(
                    "turn={} finished memory={:?} idempotent={}",
                    receipt.turn_id, receipt.memory_id, receipt.idempotent
                ),
            )?;
        }
        TurnAction::Fail => {
            let turn_id = require_turn(&args)?;
            let reason = if args.reason_stdin || args.input_stdin {
                read_stdin()?
            } else {
                "failed".into()
            };
            let snap = ax_usage::turn_fail(&root, &args.session, &turn_id, &reason).await?;
            print_json_or(
                args.json,
                &serde_json::to_value(&snap).map_err(|e| e.to_string())?,
                &format!("turn={} failed", snap.turn_id),
            )?;
        }
        TurnAction::Status => {
            let status = ax_usage::turn_status(&root, &args.session, args.turn.as_deref())?;
            print_json_or(args.json, &status, &status.to_string())?;
        }
    }
    Ok(())
}

fn require_turn(args: &TurnArgs) -> Result<String, String> {
    args.turn
        .clone()
        .ok_or_else(|| "--turn <id> is required".into())
}

fn parse_event_input(from_stdin: bool) -> Result<ax_usage::TurnEventInput, String> {
    if !from_stdin {
        return Ok(ax_usage::TurnEventInput::default());
    }
    let raw = read_stdin()?;
    if let Ok(v) = serde_json::from_str::<Value>(&raw) {
        Ok(ax_usage::TurnEventInput {
            kind: v
                .get("kind")
                .and_then(|x| x.as_str())
                .unwrap_or("note")
                .to_string(),
            body: v
                .get("body")
                .and_then(|x| x.as_str())
                .unwrap_or(raw.as_str())
                .to_string(),
            files: string_array(v.get("files")),
            symbols: string_array(v.get("symbols")),
            decisions: string_array(v.get("decisions")),
        })
    } else {
        Ok(ax_usage::TurnEventInput {
            kind: "note".into(),
            body: raw,
            ..ax_usage::TurnEventInput::default()
        })
    }
}

fn parse_finish_input(from_stdin: bool) -> Result<ax_usage::TurnFinishInput, String> {
    if !from_stdin {
        return Ok(ax_usage::TurnFinishInput::default());
    }
    let raw = read_stdin()?;
    if let Ok(v) = serde_json::from_str::<Value>(&raw) {
        let semantic = v
            .get("memories")
            .and_then(|m| m.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        Some(ax_usage::SemanticMemoryInput {
                            kind: item.get("kind")?.as_str()?.to_string(),
                            title: item
                                .get("title")
                                .and_then(|t| t.as_str())
                                .unwrap_or("")
                                .to_string(),
                            body: item.get("body")?.as_str()?.to_string(),
                            files: string_array(item.get("files")),
                            tags: string_array(item.get("tags")),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(ax_usage::TurnFinishInput {
            outcome: v
                .get("outcome")
                .and_then(|o| o.as_str())
                .unwrap_or(raw.as_str())
                .to_string(),
            decisions: string_array(v.get("decisions")),
            open_questions: string_array(v.get("openQuestions")),
            semantic_memories: semantic,
        })
    } else {
        Ok(ax_usage::TurnFinishInput {
            outcome: raw,
            ..ax_usage::TurnFinishInput::default()
        })
    }
}

fn string_array(v: Option<&Value>) -> Vec<String> {
    v.and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|i| i.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn print_json_or(json_mode: bool, value: &Value, plain: &str) -> Result<(), String> {
    if json_mode {
        println!(
            "{}",
            serde_json::to_string_pretty(value).map_err(|e| e.to_string())?
        );
    } else {
        println!("{plain}");
    }
    Ok(())
}

fn read_stdin() -> Result<String, String> {
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| e.to_string())?;
    Ok(buf)
}
