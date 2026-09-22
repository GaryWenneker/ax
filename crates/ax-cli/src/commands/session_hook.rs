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
        let _ = write_active_cursor_session(&session_id);
    }

    let event = input
        .get("hook_event_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if event == "beforeSubmitPrompt" {
        let prompt = input
            .get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if !prompt.is_empty() {
            let session = parse_cursor_hook_session_id(&input);
            let _ = ax_usage::note_session_event(session.as_deref(), "user_prompt", &prompt, None)
                .await;
        }
    }

    if let Some((session_id, model)) = parse_cursor_hook_model(&input) {
        record_session_model_tag("cursor", &session_id, &model).await?;
    }
    Ok(())
}
