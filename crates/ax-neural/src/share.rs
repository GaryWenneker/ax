use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct ModelManifest {
    pub model_id: String,
    pub version: String,
    pub checksum: String,
    pub platforms: Vec<String>,
}

/// Host-supplied dialer. The crate never opens sockets itself unless a dialer is injected.
pub trait Dialer {
    fn dial(&mut self, endpoint: &str, packet: &Value) -> Result<(), String>;
}

#[derive(Debug, Default)]
pub struct NoopDialer;

impl Dialer for NoopDialer {
    fn dial(&mut self, _endpoint: &str, _packet: &Value) -> Result<(), String> {
        Err("no dialer configured".into())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SharePolicy<'a> {
    pub mode: &'a str,
    pub models: bool,
    pub aggregates: bool,
    /// Optional central/hybrid endpoint. Dialed only when a Dialer is supplied and share is on.
    pub endpoint: Option<&'a str>,
}

/// Builds the share packet. `sent` stays false unless distribution is ready and `dialer` succeeds.
pub fn packet_for(
    policy: SharePolicy<'_>,
    models: &[ModelManifest],
    outcome_counts: &[(String, u64)],
) -> Value {
    packet_for_with_dialer(policy, models, outcome_counts, None)
}

pub fn packet_for_with_dialer(
    policy: SharePolicy<'_>,
    models: &[ModelManifest],
    outcome_counts: &[(String, u64)],
    dialer: Option<&mut dyn Dialer>,
) -> Value {
    let models: Vec<Value> = if policy.models {
        models
            .iter()
            .map(|model| {
                json!({
                    "modelId": model.model_id,
                    "version": model.version,
                    "checksum": model.checksum,
                    "platforms": model.platforms,
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    let aggregates: Vec<Value> = if policy.aggregates {
        outcome_counts
            .iter()
            .map(|(outcome, count)| json!({ "outcome": outcome, "count": count }))
            .collect()
    } else {
        Vec::new()
    };
    let remote = policy.mode == "hybrid" || policy.mode == "central";
    let would_send = policy.models || policy.aggregates;
    let sync_plan = if remote {
        json!({
            "endpoint": policy.endpoint,
            "wouldSend": would_send,
            "requiresDistribution": true,
            "offlineLocalOk": true
        })
    } else {
        Value::Null
    };
    let mut packet = json!({
        "shared": would_send,
        "mode": policy.mode,
        "sent": false,
        "models": models,
        "aggregates": aggregates,
        "rawFeedback": false,
        "sourceIncluded": false,
        "syncPlan": sync_plan
    });
    let ready = remote && would_send && policy.endpoint.is_some();
    if ready {
        if let (Some(endpoint), Some(dialer)) = (policy.endpoint, dialer) {
            match dialer.dial(endpoint, &packet) {
                Ok(()) => {
                    packet["sent"] = json!(true);
                    packet["dialError"] = Value::Null;
                }
                Err(error) => {
                    packet["sent"] = json!(false);
                    packet["dialError"] = json!(error);
                }
            }
        }
    }
    packet
}

pub fn packet(allow: bool, models: &[ModelManifest], outcome_counts: &[(String, u64)]) -> Value {
    packet_for(
        SharePolicy {
            mode: "local",
            models: allow,
            aggregates: allow,
            endpoint: None,
        },
        models,
        outcome_counts,
    )
}

pub fn checksum_matches(bytes: &[u8], expected_hex: &str) -> bool {
    let Ok(expected) = blake3::Hash::from_hex(expected_hex.trim()) else {
        return false;
    };
    // `blake3::Hash` compares in constant time and `from_hex` accepts either case.
    blake3::hash(bytes) == expected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sharing_off_exports_nothing() {
        let value = packet(
            false,
            &[ModelManifest {
                model_id: "candidate".into(),
                version: "1".into(),
                checksum: "abc".into(),
                platforms: vec!["macos-arm64".into()],
            }],
            &[("success".into(), 3)],
        );
        assert_eq!(value["shared"], false);
        assert_eq!(value["models"].as_array().unwrap().len(), 0);
        assert_eq!(value["rawFeedback"], false);
    }

    #[test]
    fn packet_respects_policy_and_never_includes_feedback_rows() {
        let value = packet_for(
            SharePolicy {
                mode: "central",
                models: true,
                aggregates: true,
                endpoint: None,
            },
            &[manifest()],
            &[("success".into(), 3)],
        );
        assert_eq!(value["rawFeedback"], false);
        assert_eq!(value["sourceIncluded"], false);
        assert!(value.get("rows").is_none());
        assert!(value.get("feedback").is_none());
        assert_eq!(value["models"].as_array().unwrap().len(), 1);
        assert_eq!(value["aggregates"][0]["count"], 3);
        let counts_only = packet_for(
            SharePolicy {
                mode: "local",
                models: false,
                aggregates: false,
                endpoint: None,
            },
            &[manifest()],
            &[("success".into(), 9)],
        );
        assert_eq!(counts_only["shared"], false);
        assert_eq!(counts_only["models"].as_array().unwrap().len(), 0);
        assert_eq!(counts_only["aggregates"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn sharing_on_keeps_manifests_and_counts_only() {
        let value = packet(
            true,
            &[ModelManifest {
                model_id: "candidate".into(),
                version: "1".into(),
                checksum: "abc".into(),
                platforms: vec!["macos-arm64".into()],
            }],
            &[("failure".into(), 2)],
        );
        let text = value.to_string();
        assert!(!text.contains("execution"));
        assert!(!text.contains("snippet"));
        assert_eq!(value["models"][0]["checksum"], "abc");
        assert_eq!(value["aggregates"][0]["count"], 2);
    }

    fn manifest() -> ModelManifest {
        ModelManifest {
            model_id: "candidate".into(),
            version: "1".into(),
            checksum: "abc".into(),
            platforms: vec!["macos-arm64".into()],
        }
    }

    #[test]
    fn packet_reports_the_configured_mode() {
        let value = packet_for(
            SharePolicy {
                mode: "hybrid",
                models: false,
                aggregates: false,
                endpoint: Some("https://example.invalid/neural"),
            },
            &[manifest()],
            &[("success".into(), 1)],
        );
        assert_eq!(value["mode"], "hybrid");
        assert_eq!(value["sent"], false);
        assert_eq!(value["shared"], false);
        assert_eq!(value["models"].as_array().unwrap().len(), 0);
        assert_eq!(value["aggregates"].as_array().unwrap().len(), 0);
        assert_eq!(
            value["syncPlan"]["endpoint"],
            "https://example.invalid/neural"
        );
        assert_eq!(value["syncPlan"]["offlineLocalOk"], true);
    }

    #[test]
    fn share_hybrid_plans_without_sending() {
        let value = packet_for(
            SharePolicy {
                mode: "hybrid",
                models: true,
                aggregates: true,
                endpoint: Some("https://example.invalid/neural"),
            },
            &[manifest()],
            &[("success".into(), 1)],
        );
        assert_eq!(value["sent"], false);
        assert_eq!(value["syncPlan"]["wouldSend"], true);
        assert_eq!(value["syncPlan"]["requiresDistribution"], true);
        assert!(value["syncPlan"]["endpoint"].is_string());
    }

    #[test]
    fn models_and_aggregates_follow_their_own_keys() {
        let models_only = packet_for(
            SharePolicy {
                mode: "central",
                models: true,
                aggregates: false,
                endpoint: None,
            },
            &[manifest()],
            &[("success".into(), 4)],
        );
        assert_eq!(models_only["shared"], true);
        assert_eq!(models_only["models"].as_array().unwrap().len(), 1);
        assert_eq!(models_only["aggregates"].as_array().unwrap().len(), 0);
        assert_eq!(models_only["syncPlan"]["wouldSend"], true);
        let counts_only = packet_for(
            SharePolicy {
                mode: "central",
                models: false,
                aggregates: true,
                endpoint: None,
            },
            &[manifest()],
            &[("success".into(), 4)],
        );
        assert_eq!(counts_only["shared"], true);
        assert_eq!(counts_only["models"].as_array().unwrap().len(), 0);
        assert_eq!(counts_only["aggregates"][0]["count"], 4);
        assert_eq!(counts_only["rawFeedback"], false);
        assert_eq!(counts_only["sent"], false);
    }

    #[test]
    fn checksum_rejects_a_changed_byte() {
        let digest = blake3::hash(b"model").to_hex().to_string();
        assert!(checksum_matches(b"model", &digest));
        assert!(!checksum_matches(b"modeL", &digest));
    }

    #[test]
    fn checksum_accepts_uppercase_hex_and_a_trailing_newline() {
        let digest = blake3::hash(b"model").to_hex().to_string();
        assert!(checksum_matches(b"model", &digest.to_ascii_uppercase()));
        assert!(checksum_matches(b"model", &format!("{digest}\n")));
        assert!(!checksum_matches(b"modeL", &digest.to_ascii_uppercase()));
        assert!(!checksum_matches(b"model", ""));
        assert!(!checksum_matches(b"model", "not hex"));
    }

    #[derive(Default)]
    struct RecordingDialer {
        calls: Vec<(String, bool)>,
        fail: bool,
    }

    impl Dialer for RecordingDialer {
        fn dial(&mut self, endpoint: &str, packet: &Value) -> Result<(), String> {
            self.calls.push((
                endpoint.to_string(),
                packet["shared"].as_bool().unwrap_or(false),
            ));
            if self.fail {
                Err("dial refused".into())
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn share_dials_when_distribution_ready() {
        let mut dialer = RecordingDialer::default();
        let value = packet_for_with_dialer(
            SharePolicy {
                mode: "central",
                models: true,
                aggregates: true,
                endpoint: Some("https://example.invalid/neural"),
            },
            &[manifest()],
            &[("success".into(), 1)],
            Some(&mut dialer),
        );
        assert_eq!(value["sent"], true);
        assert_eq!(dialer.calls.len(), 1);
        assert_eq!(dialer.calls[0].0, "https://example.invalid/neural");
        assert!(value["dialError"].is_null());
    }

    #[test]
    fn share_dial_failure_keeps_sent_false() {
        let mut dialer = RecordingDialer {
            fail: true,
            ..Default::default()
        };
        let value = packet_for_with_dialer(
            SharePolicy {
                mode: "hybrid",
                models: true,
                aggregates: false,
                endpoint: Some("https://example.invalid/neural"),
            },
            &[manifest()],
            &[],
            Some(&mut dialer),
        );
        assert_eq!(value["sent"], false);
        assert_eq!(value["dialError"], "dial refused");
        assert_eq!(value["syncPlan"]["offlineLocalOk"], true);
    }

    #[test]
    fn share_without_dialer_stays_unsent_even_when_ready() {
        let value = packet_for(
            SharePolicy {
                mode: "central",
                models: true,
                aggregates: true,
                endpoint: Some("https://example.invalid/neural"),
            },
            &[manifest()],
            &[("success".into(), 2)],
        );
        assert_eq!(value["sent"], false);
        assert_eq!(value["syncPlan"]["wouldSend"], true);
    }
}
