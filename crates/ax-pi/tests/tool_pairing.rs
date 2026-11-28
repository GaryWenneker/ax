//! Tool start and end events pair up even when the host sends no call id.

use ax_pi::{create_pi_integration, PiOptions};
use serde_json::{json, Value};

fn run(pi: &ax_pi::PiIntegration, events: &[Value]) {
    for event in events {
        pi.ingest(event).unwrap();
    }
}

#[test]
fn interleaved_tools_without_ids_keep_their_own_arguments() {
    let pi = create_pi_integration(PiOptions {
        session_id: Some("s".into()),
        ..PiOptions::default()
    });
    pi.set_repository_state("head").unwrap();
    run(
        &pi,
        &[
            json!({"type": "agent_start"}),
            json!({"type": "turn_start"}),
            json!({"type": "tool_execution_start", "toolName": "rg", "args": {"pattern": "alpha"}}),
            json!({"type": "tool_execution_start", "toolName": "cat", "args": {"path": "a.txt"}}),
            json!({"type": "tool_execution_end", "toolName": "cat", "outputTokens": 3}),
            json!({"type": "tool_execution_end", "toolName": "rg", "outputTokens": 50}),
            json!({"type": "tool_execution_start", "toolName": "rg", "args": {"pattern": "beta"}}),
            json!({"type": "tool_execution_start", "toolName": "cat", "args": {"path": "b.txt"}}),
            json!({"type": "tool_execution_end", "toolName": "cat", "outputTokens": 3}),
            json!({"type": "tool_execution_end", "toolName": "rg", "outputTokens": 50}),
        ],
    );
    let report = pi.economics(Some("s")).unwrap();
    assert_eq!(
        report.repeated_output_tokens, 0,
        "rg alpha and rg beta are different calls"
    );

    // The same arguments again is a real repeat.
    run(
        &pi,
        &[
            json!({"type": "tool_execution_start", "toolName": "rg", "args": {"pattern": "beta"}}),
            json!({"type": "tool_execution_end", "toolName": "rg", "outputTokens": 50}),
        ],
    );
    assert_eq!(pi.economics(Some("s")).unwrap().repeated_output_tokens, 50);
}
