//! Hidden `ax session-hook` — Cursor sessionStart and beforeSubmitPrompt hook.

use std::io::{self, IsTerminal, Read};

use ax_usage::{
    parse_cursor_hook_model, parse_cursor_hook_session_id, record_session_model_tag,
    write_active_cursor_session,
};

pub async fn run() -> Result<(), String> {
    if std::env::var("AX_NO_SESSION_HOOK").ok().as_deref() == Some("1") {
        return Ok(());
    }
    if io::stdin().is_terminal() {
        return Ok(());
    }

    let mut raw = String::new();
    if io::stdin().read_to_string(&mut raw).is_err() {
        return Ok(());
    }

    let input: serde_json::Value = serde_json::from_str(&raw).unwrap_or(serde_json::Value::Null);

    // Always tag the active session id when present — even if Cursor omits model.
    // Without this, verbose lines lack `session=` and audit correlation collapses.
    if let Some(session_id) = parse_cursor_hook_session_id(&input) {
        let root = input
            .get("workspace_roots")
            .and_then(|v| v.as_array())
            .and_then(|roots| (roots.len() == 1).then(|| roots[0].as_str()).flatten())
            .or_else(|| input.get("cwd").and_then(|v| v.as_str()));
        let window = input.get("window_id").and_then(|v| v.as_str());
        if let (Some(root), Some(window)) = (root, window) {
            // Missing bindings stay diagnostic only; MCP never borrows that chat.
            let path = std::path::Path::new(root);
            if let Some(project) = crate::commands::find_project_root(path) {
                let _ = ax_usage::write_bound_cursor_session(&project, window, &session_id);
            }
        } else {
            let _ = write_active_cursor_session(&session_id);
        }
    }

    let event = input
        .get("hook_event_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if event == "beforeSubmitPrompt" {
        let _ = sync_current_turn_from_hook(&input);
        if let Some(root) = hook_project_root(&input) {
            let _ = record_prompt_turn(&root, &input).await;
        }
        let prompt = input
            .get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if !prompt.is_empty() {
            let session = parse_cursor_hook_session_id(&input);
            if let (Some(session), Some(cwd)) = (session, input.get("cwd").and_then(|v| v.as_str()))
            {
                if let Some(root) = crate::commands::find_project_root(std::path::Path::new(cwd)) {
                    if ax_usage::session_from_args(&serde_json::json!({"session":session}))
                        .is_some()
                    {
                        let _ = ax_usage::note_project_tool_if_open(
                            &root,
                            &session,
                            "user_prompt",
                            &prompt,
                        )
                        .await;
                    }
                }
            }
        }
    }

    if let Some((session_id, model)) = parse_cursor_hook_model(&input) {
        record_session_model_tag("cursor", &session_id, &model).await?;
    }
    Ok(())
}

pub(crate) async fn record_prompt_turn(
    project: &std::path::Path,
    input: &serde_json::Value,
) -> Result<(), String> {
    let event = input
        .get("hook_event_name")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if event != "beforeSubmitPrompt" {
        return Ok(());
    }
    let Some(session) = parse_cursor_hook_session_id(input) else {
        return Ok(());
    };
    let Some(turn) = input
        .get("generation_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|id| !id.is_empty())
    else {
        return Ok(());
    };
    let prompt = input
        .get("prompt")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let db = ax_db::Database::open(&project.join(".ax").join("ax.db"))
        .await
        .map_err(|err| err.to_string())?;
    let result = ax_db::hook_turns::record_hook_turn(
        db.pool(),
        &ax_db::hook_turns::HookTurn {
            project_id: project.display().to_string(),
            session_id: session,
            turn_id: turn.to_string(),
            prompt: ax_memory::turns::redact_secrets(prompt),
        },
    )
    .await
    .map_err(|err| err.to_string());
    db.close().await;
    result
}

pub(crate) fn write_current_turn(
    project: &std::path::Path,
    input: &serde_json::Value,
) -> Result<(), String> {
    let event = input
        .get("hook_event_name")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if event != "beforeSubmitPrompt" {
        return Ok(());
    }
    let Some(session) = parse_cursor_hook_session_id(input) else {
        return Ok(());
    };
    let Some(turn) = input
        .get("generation_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|id| !id.is_empty())
    else {
        return Ok(());
    };
    let dir = project.join(".ax").join("turns");
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let body = serde_json::json!({
        "session_id": session,
        "turn_id": turn,
    });
    let text = serde_json::to_string(&body).map_err(|err| err.to_string())?;
    std::fs::write(dir.join("current.json"), text).map_err(|err| err.to_string())
}

fn hook_project_root(input: &serde_json::Value) -> Option<std::path::PathBuf> {
    let raw = input
        .get("workspace_roots")
        .and_then(|value| value.as_array())
        .and_then(|roots| roots.first())
        .and_then(|value| value.as_str())
        .or_else(|| input.get("cwd").and_then(|value| value.as_str()))?;
    crate::commands::find_project_root(std::path::Path::new(raw))
}

pub(crate) fn sync_current_turn_from_hook(input: &serde_json::Value) -> Result<(), String> {
    let root = hook_project_root(input).ok_or("no ax project")?;
    write_current_turn(&root, input)
}

#[cfg(test)]
mod tests {
    use super::{record_prompt_turn, sync_current_turn_from_hook, write_current_turn};

    #[test]
    fn before_submit_writes_the_session_and_the_turn() {
        let dir = tempfile::tempdir().unwrap();
        let cursor = serde_json::json!({
            "hook_event_name": "beforeSubmitPrompt",
            "conversation_id": "conv-1",
            "generation_id": "gen-1",
        });
        write_current_turn(dir.path(), &cursor).unwrap();
        let written = std::fs::read_to_string(dir.path().join(".ax/turns/current.json")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&written).unwrap();
        assert_eq!(value["session_id"], "conv-1");
        assert_eq!(value["turn_id"], "gen-1");

        let preferred = serde_json::json!({
            "hook_event_name": "beforeSubmitPrompt",
            "session_id": "sess-1",
            "conversation_id": "conv-1",
            "generation_id": " gen-2 ",
        });
        write_current_turn(dir.path(), &preferred).unwrap();
        let written = std::fs::read_to_string(dir.path().join(".ax/turns/current.json")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&written).unwrap();
        assert_eq!(value["session_id"], "sess-1");
        assert_eq!(value["turn_id"], "gen-2");

        let missing = dir.path().join("missing");
        std::fs::create_dir_all(&missing).unwrap();
        write_current_turn(
            &missing,
            &serde_json::json!({
                "hook_event_name": "beforeSubmitPrompt",
                "conversation_id": "conv-1",
                "generation_id": "  ",
            }),
        )
        .unwrap();
        assert!(!missing.join(".ax/turns/current.json").exists());

        write_current_turn(
            &missing,
            &serde_json::json!({
                "hook_event_name": "sessionStart",
                "conversation_id": "conv-1",
                "generation_id": "gen-1",
            }),
        )
        .unwrap();
        assert!(!missing.join(".ax/turns/current.json").exists());
    }

    #[test]
    fn before_submit_uses_workspace_root_when_cwd_is_absent() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".ax")).unwrap();
        std::fs::write(dir.path().join(".ax/ax.db"), []).unwrap();
        let input = serde_json::json!({
            "hook_event_name": "beforeSubmitPrompt",
            "conversation_id": "conv-9",
            "generation_id": "gen-9",
            "workspace_roots": [dir.path().to_str().unwrap()],
        });
        sync_current_turn_from_hook(&input).unwrap();
        let written = std::fs::read_to_string(dir.path().join(".ax/turns/current.json")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&written).unwrap();
        assert_eq!(value["session_id"], "conv-9");
        assert_eq!(value["turn_id"], "gen-9");
    }

    #[tokio::test]
    async fn before_submit_stores_a_redacted_prompt() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".ax")).unwrap();
        record_prompt_turn(
            dir.path(),
            &serde_json::json!({
                "hook_event_name": "beforeSubmitPrompt",
                "conversation_id": "conv-1",
                "generation_id": "gen-1",
                "prompt": "use key sk-abcdefghijklmnopqrstuvwx now",
            }),
        )
        .await
        .unwrap();
        let db = ax_db::Database::open(&dir.path().join(".ax/ax.db"))
            .await
            .unwrap();
        let prompt: String =
            sqlx::query_scalar("SELECT prompt FROM agent_turns WHERE id = 'gen-1'")
                .fetch_one(db.pool())
                .await
                .unwrap();
        db.close().await;
        assert_eq!(prompt, "use key [redacted] now");
    }
}
