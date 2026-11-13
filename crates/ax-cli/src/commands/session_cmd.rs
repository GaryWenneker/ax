//! `ax session` — working context (same engine as `ax_session` MCP).

use std::io::Read;

use serde_json::{json, Value};

use crate::commands::preflight::invoke_shared_tool;
use crate::commands::resolve_path;

#[derive(Debug, Clone)]
pub enum SessionAction {
    Start,
    Get,
    Add,
    Update,
    Compact,
    Fork,
    Handoff,
    Resume,
    End,
}

pub struct SessionArgs {
    pub action: SessionAction,
    pub session: Option<String>,
    pub path: Option<String>,
    pub input_stdin: bool,
    pub json: bool,
}

pub async fn run(args: SessionArgs) -> Result<(), String> {
    let root = resolve_path(args.path);
    let session = match (&args.action, &args.session) {
        (SessionAction::Start, None) => mint_session_id(),
        (SessionAction::Start, Some(id)) => id.clone(),
        (_, Some(id)) => id.clone(),
        _ => return Err("--session is required (except for start without an id)".into()),
    };

    let mut params = if args.input_stdin {
        parse_stdin_json()?
    } else {
        json!({})
    };
    if let Some(obj) = params.as_object_mut() {
        obj.insert("session".into(), json!(session));
        let action = match args.action {
            SessionAction::Start | SessionAction::Get | SessionAction::Resume => "get",
            SessionAction::Add => "add",
            SessionAction::Update => "update",
            SessionAction::Compact => "compact",
            SessionAction::Fork => "fork",
            SessionAction::Handoff => "handoff",
            SessionAction::End => "clear",
        };
        obj.entry("action").or_insert(json!(action));
    }

    // Ensure durable scope exists for this session.
    let _ = ax_usage::durable_project_apply(&root, &session, &json!({"action": "read"})).await;

    let result = invoke_shared_tool(&root, "ax_session", params).await?;
    let _ = ax_usage::record_activity(
        &root,
        ax_usage::ActivityRecord {
            idempotency_key: format!("cli.session.{:?}:{session}", args.action),
            source: Some(ax_usage::ActivitySource::Cli),
            session_id: Some(session.clone()),
            operation: format!("session.{:?}", args.action).to_ascii_lowercase(),
            status: Some(ax_usage::ActivityStatus::Success),
            summary: format!("ax session {:?}", args.action),
            ..ax_usage::ActivityRecord::default()
        },
    )
    .await;

    let out = json!({
        "session": session,
        "result": result,
    });
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?
        );
    } else {
        println!("session={session}");
        println!("{}", ax_mcp::format_tool_result(&result));
    }
    Ok(())
}

fn mint_session_id() -> String {
    format!("axs_{}", &uuid_lite()[..16])
}

fn uuid_lite() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};
    let mut h = DefaultHasher::new();
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
        .hash(&mut h);
    std::process::id().hash(&mut h);
    format!("{:016x}{:016x}", h.finish(), h.finish().rotate_left(7))
}

fn parse_stdin_json() -> Result<Value, String> {
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| e.to_string())?;
    if buf.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(&buf).map_err(|e| format!("stdin JSON: {e}"))
}
