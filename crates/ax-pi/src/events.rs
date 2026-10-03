//! Translate Pi's public agent events and telemetry span names into [`AxAgentEvent`].
//!
//! Pi agent events (`agent_start`, `turn_start`, `tool_execution_*`, `message_end`)
//! come from `@earendil-works/pi-agent-core`. Span names under `pi.ai.*`,
//! `pi.harness.*`, and `pi.session.*` are accepted when a telemetry adapter
//! forwards them. Message bodies, tool arguments, and tool output text are
//! dropped here. Callers that need a token estimate pass the text through
//! [`crate::estimate`] and keep only the number.

use serde_json::Value;

use crate::types::{
    AxAgentEvent, AxEventBase, AxModelUsage, ModelEvent, ToolEndEvent, ToolOutputEvent,
    ToolStartEvent,
};

#[derive(Debug)]
pub struct TranslateError(pub String);

pub fn translate(
    event: &Value,
    fallback_session: &str,
    timestamp: u64,
) -> Result<Vec<AxAgentEvent>, TranslateError> {
    let name = event_name(event).ok_or_else(|| TranslateError("missing event name".into()))?;
    let session = string_attr(event, &["sessionId", "session_id", "session.id"])
        .unwrap_or_else(|| fallback_session.to_string());
    if session.is_empty() {
        return Err(TranslateError("missing session id".into()));
    }
    let mut base = AxEventBase::new(
        attr_u64(event, &["timestamp", "timeUnixNano"]).unwrap_or(timestamp),
        session,
    );
    base.turn_id = string_attr(event, &["turnId", "turn_id", "turn.id"]);
    base.operation_id = string_attr(
        event,
        &["operationId", "operation_id", "toolCallId", "operation.id"],
    );
    base.lane_name = string_attr(event, &["laneName", "lane", "lane.name"]);

    let mapped = match name.as_str() {
        "agent_start" | "pi.session.start" | "pi.session.started" => {
            vec![AxAgentEvent::SessionStarted(base)]
        }
        "agent_end" | "pi.session.end" | "pi.session.ended" | "pi.session.stop" => {
            vec![AxAgentEvent::SessionEnded(base)]
        }
        "turn_start" | "pi.harness.turn.start" | "pi.harness.turn.started" => {
            vec![AxAgentEvent::TurnStarted(base)]
        }
        "turn_end" | "pi.harness.turn.end" | "pi.harness.turn.ended" => {
            vec![AxAgentEvent::TurnEnded(base)]
        }
        "pi.session.compaction" | "pi.session.compact" | "compaction" => {
            vec![AxAgentEvent::Compaction(base)]
        }
        "pi.session.write" | "session_write" => vec![AxAgentEvent::SessionWrite(base)],
        "tool_execution_start" | "pi.harness.tool.start" | "pi.ai.tool.start" => {
            let tool_name = tool_name(event)?;
            vec![AxAgentEvent::ToolStarted(ToolStartEvent {
                base,
                tool_name,
            })]
        }
        "tool_execution_update" | "pi.harness.tool.output" => {
            let tool_name = tool_name(event)?;
            vec![AxAgentEvent::ToolOutput(ToolOutputEvent {
                base,
                tool_name,
                output_token_estimate: attr_u64(
                    event,
                    &["outputTokens", "output_tokens", "output.tokens"],
                )
                .map(|n| n as u32),
                output_bytes: attr_u64(event, &["outputBytes", "output_bytes", "output.bytes"]),
            })]
        }
        "tool_execution_end" | "pi.harness.tool.end" | "pi.ai.tool.end" => {
            let tool_name = tool_name(event)?;
            let success = match event.get("isError").and_then(Value::as_bool) {
                Some(is_error) => !is_error,
                None => attr_bool(event, &["success"]).unwrap_or(true),
            };
            vec![AxAgentEvent::ToolEnded(ToolEndEvent {
                base,
                tool_name,
                duration_ms: attr_u64(event, &["durationMs", "duration_ms"]).unwrap_or(0),
                success,
                output_token_estimate: attr_u64(
                    event,
                    &["outputTokens", "output_tokens", "output.tokens"],
                )
                .map(|n| n as u32),
                output_bytes: attr_u64(event, &["outputBytes", "output_bytes", "output.bytes"]),
            })]
        }
        "pi.ai.request" | "pi.ai.model.request" | "model_request" => {
            vec![AxAgentEvent::ModelRequest(model_event(base, event))]
        }
        "message_end" | "pi.ai.response" | "pi.ai.model.response" | "model_response" => {
            if name == "message_end" && message_role(event) != Some("assistant") {
                return Ok(Vec::new());
            }
            vec![AxAgentEvent::ModelResponse(model_event(base, event))]
        }
        _ => Vec::new(),
    };
    Ok(mapped)
}

fn model_event(base: AxEventBase, event: &Value) -> ModelEvent {
    let message = event.get("message").unwrap_or(event);
    let usage_src = message.get("usage").unwrap_or(message);
    ModelEvent {
        provider: string_attr(message, &["provider"]).or_else(|| string_attr(event, &["provider"])),
        model: string_attr(message, &["model", "modelId"])
            .or_else(|| string_attr(event, &["model"])),
        usage: AxModelUsage {
            input_tokens: usage_u64(usage_src, &["input", "inputTokens", "input_tokens"]),
            output_tokens: usage_u64(usage_src, &["output", "outputTokens", "output_tokens"]),
            cached_input_tokens: usage_u64(
                usage_src,
                &["cacheRead", "cachedInputTokens", "cached_input_tokens"],
            ),
            total_tokens: usage_u64(usage_src, &["totalTokens", "total_tokens", "total"]),
        },
        base,
    }
}

fn event_name(event: &Value) -> Option<String> {
    string_attr(event, &["type", "name"]).map(|name| name.trim().to_string())
}

fn tool_name(event: &Value) -> Result<String, TranslateError> {
    string_attr(event, &["toolName", "tool_name", "tool.name"])
        .ok_or_else(|| TranslateError("missing tool name".into()))
}

fn message_role(event: &Value) -> Option<&str> {
    event
        .get("message")
        .and_then(|m| m.get("role"))
        .and_then(Value::as_str)
}

fn string_attr(event: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = event.get(*key).and_then(Value::as_str) {
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
        if let Some(value) = event
            .get("attributes")
            .and_then(|attrs| attrs.get(*key))
            .and_then(Value::as_str)
        {
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn attr_u64(event: &Value, keys: &[&str]) -> Option<u64> {
    for key in keys {
        for source in [event, event.get("attributes").unwrap_or(&Value::Null)] {
            if let Some(n) = source.get(*key).and_then(Value::as_u64) {
                return Some(n);
            }
            if let Some(n) = source.get(*key).and_then(Value::as_i64) {
                if n >= 0 {
                    return Some(n as u64);
                }
            }
        }
    }
    None
}

fn attr_bool(event: &Value, keys: &[&str]) -> Option<bool> {
    for key in keys {
        for source in [event, event.get("attributes").unwrap_or(&Value::Null)] {
            if let Some(v) = source.get(*key).and_then(Value::as_bool) {
                return Some(v);
            }
        }
    }
    None
}

fn usage_u64(event: &Value, keys: &[&str]) -> Option<u64> {
    attr_u64(event, keys)
}

/// Text used only to estimate tokens. The caller must not store the returned string.
pub fn output_text_for_estimate(event: &Value) -> Option<String> {
    if let Some(text) = event.get("outputText").and_then(Value::as_str) {
        return Some(text.to_string());
    }
    let result = event.get("result")?;
    if let Some(text) = result.as_str() {
        return Some(text.to_string());
    }
    if let Some(text) = result.get("content").and_then(|c| match c {
        Value::String(s) => Some(s.clone()),
        Value::Array(items) => Some(
            items
                .iter()
                .filter_map(|item| item.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        _ => None,
    }) {
        if !text.is_empty() {
            return Some(text);
        }
    }
    None
}

pub fn arguments_for_hash(event: &Value) -> Value {
    event
        .get("args")
        .cloned()
        .or_else(|| event.get("arguments").cloned())
        .unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pi_agent_tool_event_drops_arguments_and_output() {
        let event = json!({
            "type": "tool_execution_end",
            "toolCallId": "c1",
            "toolName": "rg",
            "args": { "pattern": "SECRET_PROMPT_SENTINEL" },
            "result": "SECRET_PROMPT_SENTINEL output",
            "isError": false,
            "outputTokens": 12
        });
        let translated = translate(&event, "sess", 10).unwrap();
        let rendered = serde_json::to_string(&translated).unwrap();
        assert!(!rendered.contains("SECRET_PROMPT_SENTINEL"));
        match &translated[0] {
            AxAgentEvent::ToolEnded(end) => {
                assert_eq!(end.tool_name, "rg");
                assert!(end.success);
                assert_eq!(end.output_token_estimate, Some(12));
                assert_eq!(end.base.operation_id.as_deref(), Some("c1"));
                assert_eq!(end.base.source, "pi");
                assert_eq!(end.base.schema_version, 1);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn assistant_message_keeps_usage_only() {
        let event = json!({
            "type": "message_end",
            "message": {
                "role": "assistant",
                "content": [{ "type": "text", "text": "SECRET_PROMPT_SENTINEL" }],
                "provider": "anthropic",
                "model": "claude",
                "usage": { "input": 10, "output": 4, "cacheRead": 2, "totalTokens": 16 }
            }
        });
        let translated = translate(&event, "sess", 1).unwrap();
        let rendered = serde_json::to_string(&translated).unwrap();
        assert!(!rendered.contains("SECRET_PROMPT_SENTINEL"));
        match &translated[0] {
            AxAgentEvent::ModelResponse(model) => {
                assert_eq!(model.usage.input_tokens, Some(10));
                assert_eq!(model.usage.cached_input_tokens, Some(2));
                assert_eq!(model.provider.as_deref(), Some("anthropic"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn user_message_is_ignored() {
        let event =
            json!({ "type": "message_end", "message": { "role": "user", "content": "hello" } });
        assert!(translate(&event, "sess", 1).unwrap().is_empty());
    }

    #[test]
    fn span_name_session_start() {
        let event = json!({
            "name": "pi.session.started",
            "timestamp": 5,
            "attributes": { "session.id": "abc", "turn.id": "t" }
        });
        let translated = translate(&event, "", 1).unwrap();
        match &translated[0] {
            AxAgentEvent::SessionStarted(base) => {
                assert_eq!(base.session_id, "abc");
                assert_eq!(base.turn_id.as_deref(), Some("t"));
                assert_eq!(base.timestamp, 5);
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
