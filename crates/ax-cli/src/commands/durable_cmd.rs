//! `ax durable` — transcript / task persistence (same engine as `ax_durable`).

use std::io::Read;

use serde_json::{json, Value};

use crate::commands::resolve_project_root;

#[derive(Debug, Clone)]
pub enum DurableAction {
    Append { kind: String },
    Read,
    Search { query: String },
    Compact,
    Fork,
    Handoff,
    DocPut { kind: String },
    DocGet { kind: String },
    TaskStart,
    TaskCheckpoint,
    TaskResume,
    TaskFinish,
}

pub struct DurableArgs {
    pub action: DurableAction,
    pub session: String,
    pub path: Option<String>,
    pub body_stdin: bool,
    pub input_stdin: bool,
    pub json: bool,
}

pub async fn run(args: DurableArgs) -> Result<(), String> {
    let root = resolve_project_root(args.path);
    let mut request = if args.input_stdin || args.body_stdin {
        let raw = read_stdin()?;
        if args.body_stdin {
            json!({"body": raw})
        } else if raw.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&raw).map_err(|e| format!("stdin JSON: {e}"))?
        }
    } else {
        json!({})
    };

    let action_name = match &args.action {
        DurableAction::Append { kind } => {
            if let Some(obj) = request.as_object_mut() {
                obj.insert("kind".into(), json!(kind));
                obj.entry("action").or_insert(json!("append"));
            }
            "append"
        }
        DurableAction::Read => {
            merge_action(&mut request, "read");
            "read"
        }
        DurableAction::Search { query } => {
            if let Some(obj) = request.as_object_mut() {
                obj.insert("action".into(), json!("search"));
                obj.insert("query".into(), json!(query));
            }
            "search"
        }
        DurableAction::Compact => {
            merge_action(&mut request, "compact");
            "compact"
        }
        DurableAction::Fork => {
            merge_action(&mut request, "fork");
            "fork"
        }
        DurableAction::Handoff => {
            merge_action(&mut request, "handoff");
            "handoff"
        }
        DurableAction::DocPut { kind } => {
            if let Some(obj) = request.as_object_mut() {
                obj.insert("action".into(), json!("doc_put"));
                obj.insert("kind".into(), json!(kind));
            }
            "doc_put"
        }
        DurableAction::DocGet { kind } => {
            if let Some(obj) = request.as_object_mut() {
                obj.insert("action".into(), json!("doc_get"));
                obj.insert("kind".into(), json!(kind));
            }
            "doc_get"
        }
        DurableAction::TaskStart => {
            merge_action(&mut request, "task_start");
            "task_start"
        }
        DurableAction::TaskCheckpoint => {
            merge_action(&mut request, "task_checkpoint");
            "task_checkpoint"
        }
        DurableAction::TaskResume => {
            merge_action(&mut request, "task_resume");
            "task_resume"
        }
        DurableAction::TaskFinish => {
            merge_action(&mut request, "task_finish");
            "task_finish"
        }
    };

    let text = ax_usage::durable_project_apply(&root, &args.session, &request).await?;
    let _ = ax_usage::record_activity(
        &root,
        ax_usage::ActivityRecord {
            idempotency_key: format!(
                "cli.durable.{action_name}:{}:{}",
                args.session,
                chrono_stamp()
            ),
            source: Some(ax_usage::ActivitySource::Cli),
            session_id: Some(args.session.clone()),
            operation: format!("durable.{action_name}"),
            status: Some(ax_usage::ActivityStatus::Success),
            summary: format!("ax durable {action_name}"),
            ..ax_usage::ActivityRecord::default()
        },
    )
    .await;

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "session": args.session,
                "action": action_name,
                "text": text,
            }))
            .map_err(|e| e.to_string())?
        );
    } else {
        println!("{text}");
    }
    Ok(())
}

fn merge_action(request: &mut Value, action: &str) {
    if let Some(obj) = request.as_object_mut() {
        obj.entry("action").or_insert(json!(action));
    }
}

fn read_stdin() -> Result<String, String> {
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| e.to_string())?;
    Ok(buf)
}

fn chrono_stamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Command-line form of `ax durable`, mapped onto [`DurableArgs`].
#[derive(clap::Subcommand)]
pub enum DurableCliAction {
    Append {
        #[arg(long)]
        session: String,
        #[arg(long)]
        kind: String,
        #[arg(long)]
        path: Option<String>,
        #[arg(long = "body-stdin", action = clap::ArgAction::SetTrue)]
        body_stdin: bool,
        #[arg(long)]
        json: bool,
    },
    Read {
        #[arg(long)]
        session: String,
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Search {
        #[arg(long)]
        session: String,
        #[arg(long)]
        query: String,
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Compact {
        #[arg(long)]
        session: String,
        #[arg(long)]
        path: Option<String>,
        #[arg(long = "summary-stdin", action = clap::ArgAction::SetTrue)]
        summary_stdin: bool,
        #[arg(long)]
        json: bool,
    },
    #[command(name = "task-start")]
    TaskStart {
        #[arg(long)]
        session: String,
        #[arg(long)]
        path: Option<String>,
        #[arg(long = "input-stdin", action = clap::ArgAction::SetTrue)]
        input_stdin: bool,
        #[arg(long)]
        json: bool,
    },
    #[command(name = "task-checkpoint")]
    TaskCheckpoint {
        #[arg(long)]
        session: String,
        #[arg(long)]
        path: Option<String>,
        #[arg(long = "input-stdin", action = clap::ArgAction::SetTrue)]
        input_stdin: bool,
        #[arg(long)]
        json: bool,
    },
    #[command(name = "task-resume")]
    TaskResume {
        #[arg(long)]
        session: String,
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        json: bool,
    },
    #[command(name = "task-finish")]
    TaskFinish {
        #[arg(long)]
        session: String,
        #[arg(long)]
        path: Option<String>,
        #[arg(long = "input-stdin", action = clap::ArgAction::SetTrue)]
        input_stdin: bool,
        #[arg(long)]
        json: bool,
    },
    Fork {
        #[arg(long)]
        session: String,
        #[arg(long)]
        path: Option<String>,
        #[arg(long = "input-stdin", action = clap::ArgAction::SetTrue)]
        input_stdin: bool,
        #[arg(long)]
        json: bool,
    },
    Handoff {
        #[arg(long)]
        session: String,
        #[arg(long)]
        path: Option<String>,
        #[arg(long = "input-stdin", action = clap::ArgAction::SetTrue)]
        input_stdin: bool,
        #[arg(long)]
        json: bool,
    },
    #[command(name = "doc-put")]
    DocPut {
        #[arg(long)]
        session: String,
        #[arg(long)]
        kind: String,
        #[arg(long)]
        path: Option<String>,
        #[arg(long = "body-stdin", action = clap::ArgAction::SetTrue)]
        body_stdin: bool,
        #[arg(long)]
        json: bool,
    },
    #[command(name = "doc-get")]
    DocGet {
        #[arg(long)]
        session: String,
        #[arg(long)]
        kind: String,
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

impl From<DurableCliAction> for DurableArgs {
    fn from(action: DurableCliAction) -> Self {
        match action {
            DurableCliAction::Append {
                session,
                kind,
                path,
                body_stdin,
                json,
            } => DurableArgs {
                action: DurableAction::Append { kind },
                session,
                path,
                body_stdin,
                input_stdin: false,
                json,
            },
            DurableCliAction::Read {
                session,
                path,
                json,
            } => DurableArgs {
                action: DurableAction::Read,
                session,
                path,
                body_stdin: false,
                input_stdin: false,
                json,
            },
            DurableCliAction::Search {
                session,
                query,
                path,
                json,
            } => DurableArgs {
                action: DurableAction::Search { query },
                session,
                path,
                body_stdin: false,
                input_stdin: false,
                json,
            },
            DurableCliAction::Compact {
                session,
                path,
                summary_stdin,
                json,
            } => DurableArgs {
                action: DurableAction::Compact,
                session,
                path,
                body_stdin: summary_stdin,
                input_stdin: false,
                json,
            },
            DurableCliAction::TaskStart {
                session,
                path,
                input_stdin,
                json,
            } => DurableArgs {
                action: DurableAction::TaskStart,
                session,
                path,
                body_stdin: false,
                input_stdin,
                json,
            },
            DurableCliAction::TaskCheckpoint {
                session,
                path,
                input_stdin,
                json,
            } => DurableArgs {
                action: DurableAction::TaskCheckpoint,
                session,
                path,
                body_stdin: false,
                input_stdin,
                json,
            },
            DurableCliAction::TaskResume {
                session,
                path,
                json,
            } => DurableArgs {
                action: DurableAction::TaskResume,
                session,
                path,
                body_stdin: false,
                input_stdin: false,
                json,
            },
            DurableCliAction::TaskFinish {
                session,
                path,
                input_stdin,
                json,
            } => DurableArgs {
                action: DurableAction::TaskFinish,
                session,
                path,
                body_stdin: false,
                input_stdin,
                json,
            },
            DurableCliAction::Fork {
                session,
                path,
                input_stdin,
                json,
            } => DurableArgs {
                action: DurableAction::Fork,
                session,
                path,
                body_stdin: false,
                input_stdin,
                json,
            },
            DurableCliAction::Handoff {
                session,
                path,
                input_stdin,
                json,
            } => DurableArgs {
                action: DurableAction::Handoff,
                session,
                path,
                body_stdin: false,
                input_stdin,
                json,
            },
            DurableCliAction::DocPut {
                session,
                kind,
                path,
                body_stdin,
                json,
            } => DurableArgs {
                action: DurableAction::DocPut { kind },
                session,
                path,
                body_stdin,
                input_stdin: false,
                json,
            },
            DurableCliAction::DocGet {
                session,
                kind,
                path,
                json,
            } => DurableArgs {
                action: DurableAction::DocGet { kind },
                session,
                path,
                body_stdin: false,
                input_stdin: false,
                json,
            },
        }
    }
}
