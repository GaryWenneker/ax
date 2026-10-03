//! Golden scenarios from the Pi integration plan.

use std::time::Instant;

use ax_pi::{
    create_pi_integration, ContextRequest, ContextSection, OptimizationMode, PiOptions,
    SectionType, ToolCallObservation,
};
use ax_usage::ModelPricing;
use serde_json::json;

fn priced(mode: OptimizationMode) -> PiOptions {
    PiOptions {
        mode,
        pricing: Some(ModelPricing {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
            cache_read_per_mtok: Some(0.3),
            cache_write_per_mtok: Some(0.3),
        }),
        session_id: Some("abc123".into()),
        monthly_budget: Some(60.0),
        currency: "USD".into(),
        days_elapsed: 10,
        days_remaining: 20,
        ..PiOptions::default()
    }
}

#[test]
fn scenario_rg_userservice_advises_ax_explore() {
    let pi = create_pi_integration(priced(OptimizationMode::Advise));
    pi.set_graph_entities(vec!["UserService".into()]).unwrap();
    let advice = pi
        .advise_tool_call(&ToolCallObservation {
            session_id: "abc123".into(),
            turn_id: "t".into(),
            call_id: "c".into(),
            tool_name: "rg".into(),
            arguments: json!({"pattern": "UserService", "path": "src/"}),
            repository_state: "head".into(),
            known_graph_entities: vec!["UserService".into()],
            indexed_paths: Vec::new(),
            output_text_for_estimate: None,
            output_token_estimate: Some(7_423),
            output_bytes: None,
            duration_ms: 3,
            success: true,
        })
        .unwrap();
    assert_eq!(advice.alternatives[0].tool_name, "ax_explore");
    assert!((advice.alternatives[0].estimated_reduction_percent - 91.4).abs() < 0.05);
}

#[test]
fn scenario_repeated_read_marks_the_second_call() {
    let pi = create_pi_integration(priced(OptimizationMode::Advise));
    pi.set_repository_state("head").unwrap();
    let observation = ToolCallObservation {
        session_id: "abc123".into(),
        turn_id: "t".into(),
        call_id: "first".into(),
        tool_name: "read".into(),
        arguments: json!("src/UserService.cs"),
        repository_state: "head".into(),
        known_graph_entities: Vec::new(),
        indexed_paths: vec!["src/UserService.cs".into()],
        output_text_for_estimate: Some("class UserService {}".into()),
        output_token_estimate: Some(20),
        output_bytes: Some(20),
        duration_ms: 1,
        success: true,
    };
    pi.record_tool_result(&observation).unwrap();
    let mut second = observation.clone();
    second.call_id = "second".into();
    pi.record_tool_result(&second).unwrap();
    let report = pi.economics(Some("abc123")).unwrap();
    assert_eq!(report.repeated_output_tokens, 20);
}

#[test]
fn scenario_huge_output_generates_advice() {
    let pi = create_pi_integration(priced(OptimizationMode::Advise));
    let advice = pi
        .advise_tool_call(&ToolCallObservation {
            session_id: "abc123".into(),
            turn_id: "t".into(),
            call_id: "big".into(),
            tool_name: "bash".into(),
            arguments: json!({"cmd": "yes"}),
            repository_state: "head".into(),
            known_graph_entities: Vec::new(),
            indexed_paths: Vec::new(),
            output_text_for_estimate: None,
            output_token_estimate: Some(9_000),
            output_bytes: None,
            duration_ms: 1,
            success: true,
        })
        .unwrap();
    assert_eq!(advice.alternatives[0].tool_name, "filtered-output");
}

#[test]
fn scenario_ax_failure_does_not_fail_pi() {
    let pi = create_pi_integration(priced(OptimizationMode::Observe));
    pi.fail_next();
    assert!(pi.ingest(&json!({"type": "agent_start"})).is_ok());
    pi.fail_next();
    let advice = pi
        .advise_tool_call(&ToolCallObservation {
            session_id: "abc123".into(),
            turn_id: "t".into(),
            call_id: "c".into(),
            tool_name: "rg".into(),
            arguments: json!({}),
            repository_state: "head".into(),
            known_graph_entities: Vec::new(),
            indexed_paths: Vec::new(),
            output_text_for_estimate: None,
            output_token_estimate: Some(1),
            output_bytes: None,
            duration_ms: 1,
            success: true,
        })
        .unwrap();
    assert!(advice.degraded);
}

#[test]
fn scenario_repository_change_keeps_unrelated_context() {
    let pi = create_pi_integration(priced(OptimizationMode::Advise));
    pi.set_knowledge(vec![ContextSection {
        id: "src".into(),
        kind: SectionType::Source,
        priority: 1,
        token_estimate: 1,
        content: "class UserService".into(),
        mandatory: false,
        source_paths: vec!["src/UserService.cs".into()],
    }])
    .unwrap();
    let first = pi
        .get_session_context(&ContextRequest {
            session_id: "abc123".into(),
            turn_id: "t".into(),
            user_intent: Some("service".into()),
            repository_root: None,
            changed_files: Vec::new(),
            previous_tool_names: Vec::new(),
        })
        .unwrap();
    assert!(!first.cache_hit);
    pi.set_knowledge(vec![ContextSection {
        id: "docs".into(),
        kind: SectionType::Source,
        priority: 1,
        token_estimate: 1,
        content: "readme".into(),
        mandatory: false,
        source_paths: vec!["docs/readme.md".into()],
    }])
    .unwrap();
    let second = pi
        .get_session_context(&ContextRequest {
            session_id: "abc123".into(),
            turn_id: "t".into(),
            user_intent: Some("docs".into()),
            repository_root: None,
            changed_files: Vec::new(),
            previous_tool_names: Vec::new(),
        })
        .unwrap();
    assert!(!second.cache_hit);
    pi.invalidate_context_paths(&["src/UserService.cs".into()])
        .unwrap();
    let docs_again = pi
        .get_session_context(&ContextRequest {
            session_id: "abc123".into(),
            turn_id: "t".into(),
            user_intent: Some("docs".into()),
            repository_root: None,
            changed_files: Vec::new(),
            previous_tool_names: Vec::new(),
        })
        .unwrap();
    assert!(docs_again.cache_hit);
    let source_again = pi
        .get_session_context(&ContextRequest {
            session_id: "abc123".into(),
            turn_id: "t".into(),
            user_intent: Some("service".into()),
            repository_root: None,
            changed_files: Vec::new(),
            previous_tool_names: Vec::new(),
        })
        .unwrap();
    assert!(!source_again.cache_hit);
}

#[test]
fn scenario_mandatory_rule_is_kept() {
    let pi = create_pi_integration(PiOptions {
        context_max_tokens: 1,
        ..priced(OptimizationMode::Advise)
    });
    pi.set_knowledge(vec![
        ContextSection {
            id: "rule".into(),
            kind: SectionType::Rule,
            priority: 100,
            token_estimate: 1,
            content: "always apply this rule about secrets".into(),
            mandatory: true,
            source_paths: Vec::new(),
        },
        ContextSection {
            id: "bg".into(),
            kind: SectionType::Summary,
            priority: 1,
            token_estimate: 1,
            content: "background ".repeat(500),
            mandatory: false,
            source_paths: Vec::new(),
        },
    ])
    .unwrap();
    let result = pi
        .get_session_context(&ContextRequest {
            session_id: "abc123".into(),
            turn_id: "t".into(),
            user_intent: None,
            repository_root: None,
            changed_files: Vec::new(),
            previous_tool_names: Vec::new(),
        })
        .unwrap();
    assert!(result.sections.iter().any(|section| section.id == "rule"));
    assert!(result.sections.iter().all(|section| section.id != "bg"));
}

#[tokio::test]
async fn end_to_end_pi_events_produce_economics_without_storing_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("ax.db");
    let pi = create_pi_integration(PiOptions {
        db_path: Some(db.clone()),
        ..priced(OptimizationMode::Advise)
    });
    pi.set_graph_entities(vec!["UserService".into()]).unwrap();
    pi.start().unwrap();
    for event in [
        json!({"type": "agent_start", "sessionId": "abc123"}),
        json!({"type": "turn_start", "sessionId": "abc123"}),
        json!({
            "type": "tool_execution_start",
            "sessionId": "abc123",
            "toolCallId": "call-1",
            "toolName": "rg",
            "args": {"pattern": "UserService", "authorization": "SECRET_PROMPT_SENTINEL"}
        }),
        json!({
            "type": "tool_execution_end",
            "sessionId": "abc123",
            "toolCallId": "call-1",
            "toolName": "rg",
            "isError": false,
            "outputTokens": 7423,
            "durationMs": 4,
            "result": "SECRET_PROMPT_SENTINEL",
            "args": {"pattern": "UserService"}
        }),
        json!({
            "type": "message_end",
            "sessionId": "abc123",
            "message": {
                "role": "assistant",
                "content": [{"type": "text", "text": "SECRET_PROMPT_SENTINEL"}],
                "provider": "anthropic",
                "model": "claude",
                "usage": {"input": 1000, "output": 50, "cacheRead": 10, "totalTokens": 1060}
            }
        }),
        json!({"type": "turn_end", "sessionId": "abc123"}),
        json!({"type": "agent_end", "sessionId": "abc123"}),
    ] {
        pi.ingest(&event).unwrap();
    }
    let report = pi.economics(Some("abc123")).unwrap();
    assert_eq!(report.session_id.as_deref(), Some("abc123"));
    assert_eq!(report.turns, 1);
    assert_eq!(report.input_tokens, 1000);
    assert_eq!(report.output_tokens, 50);
    assert_eq!(report.cached_input_tokens, 10);
    assert!(report.estimated_model_cost.unwrap() > 0.0);
    assert_eq!(report.tool_output_tokens.get("rg").copied(), Some(7423));
    let text = pi.format_optimization_text().unwrap();
    assert!(text.contains("ax_explore"));
    assert!(text.contains("estimate"));
    pi.stop().await.unwrap();
    let stored = std::fs::read_to_string(&db).unwrap_or_default();
    let bytes = std::fs::read(&db).unwrap();
    let blob = String::from_utf8_lossy(&bytes);
    assert!(
        !blob.contains("SECRET_PROMPT_SENTINEL"),
        "database persisted a secret: {stored}"
    );
    let (loaded, json) = ax_pi::read_economics(&db, Some("abc123"), "USD", None)
        .await
        .unwrap();
    assert!(loaded.contains("abc123"));
    assert!(json["input_tokens"].as_u64().unwrap() >= 1000);
}

#[test]
fn disabled_integration_accepts_events() {
    let pi = create_pi_integration(PiOptions {
        enabled: false,
        ..PiOptions::default()
    });
    assert!(pi.ingest(&json!({"type": "not-an-event"})).is_ok());
    assert_eq!(pi.economics(None).unwrap().turns, 0);
}

#[test]
fn event_processing_stays_under_five_milliseconds_average() {
    let pi = create_pi_integration(priced(OptimizationMode::Observe));
    let event = json!({"type": "turn_start", "sessionId": "abc123"});
    let started = Instant::now();
    let count = 200u32;
    for _ in 0..count {
        pi.ingest(&event).unwrap();
    }
    let average_ms = started.elapsed().as_secs_f64() * 1000.0 / f64::from(count);
    assert!(average_ms < 5.0, "average event processing {average_ms}ms");
}

#[test]
fn optimize_mode_approves_explore_and_can_replay_a_repeat_from_memory() {
    let pi = create_pi_integration(priced(OptimizationMode::Optimize));
    pi.set_graph_entities(vec!["UserService".into()]).unwrap();
    let advice = pi
        .advise_tool_call(&ToolCallObservation {
            session_id: "abc123".into(),
            turn_id: "t".into(),
            call_id: "c".into(),
            tool_name: "rg".into(),
            arguments: json!({"pattern": "UserService"}),
            repository_state: "head".into(),
            known_graph_entities: vec!["UserService".into()],
            indexed_paths: Vec::new(),
            output_text_for_estimate: None,
            output_token_estimate: Some(7_423),
            output_bytes: None,
            duration_ms: 1,
            success: true,
        })
        .unwrap();
    assert_eq!(advice.apply.unwrap().tool_name, "ax_explore");

    let observation = ToolCallObservation {
        session_id: "abc123".into(),
        turn_id: "t".into(),
        call_id: "r1".into(),
        tool_name: "bash".into(),
        arguments: json!({"cmd": "same"}),
        repository_state: "head".into(),
        known_graph_entities: Vec::new(),
        indexed_paths: Vec::new(),
        output_text_for_estimate: Some("cached-body".into()),
        output_token_estimate: Some(3),
        output_bytes: None,
        duration_ms: 1,
        success: true,
    };
    pi.record_tool_result(&observation).unwrap();
    let mut again = observation.clone();
    again.call_id = "r2".into();
    again.output_token_estimate = Some(3_000);
    let advice = pi.advise_tool_call(&again).unwrap();
    assert_eq!(advice.cached_output.as_deref(), Some("cached-body"));
    assert_eq!(
        advice.apply.as_ref().map(|item| item.rule.as_str()),
        Some("exact-repeat")
    );
}

#[test]
fn config_defaults_to_observe() {
    let options = PiOptions::from_config_value(&json!({
        "pi": { "integration": true, "optimization": { "mode": "observe" } },
        "budget": { "monthly": 60, "currency": "EUR" }
    }));
    assert!(options.enabled);
    assert_eq!(options.mode, OptimizationMode::Observe);
    assert_eq!(options.monthly_budget, Some(60.0));
    assert_eq!(options.currency, "EUR");
}
