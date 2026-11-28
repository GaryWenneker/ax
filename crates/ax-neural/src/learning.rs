use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct FeedbackInput {
    pub project_id: String,
    pub execution_id: String,
    pub task_category: String,
    pub context_ids: Vec<String>,
    pub skill_ids: Vec<String>,
    pub rule_ids: Vec<String>,
    pub outcome: String,
    pub retry_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub source_text: String,
}

#[derive(Debug, Clone)]
pub struct FeedbackRow {
    pub id: String,
    pub project_id: String,
    pub execution_id: String,
    pub task_category: String,
    pub context_ids: String,
    pub skill_ids: String,
    pub rule_ids: String,
    pub outcome: String,
    pub context_attribution: String,
    pub retry_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

/// Most ids accepted in each of the context, skill and rule lists of one feedback call.
pub const MAX_FEEDBACK_IDS: usize = 256;
/// Most bytes accepted in the execution id, the task category, and each single id.
pub const MAX_FEEDBACK_FIELD_BYTES: usize = 512;

pub fn validate_feedback(input: &FeedbackInput) -> Result<FeedbackRow, String> {
    if input.execution_id.trim().is_empty() {
        return Err("execution id is required".into());
    }
    if input.project_id.trim().is_empty() {
        return Err("project id is required".into());
    }
    if !matches!(
        input.outcome.as_str(),
        "success" | "partial" | "failure" | "unknown"
    ) {
        return Err("outcome must be success, partial, failure, or unknown".into());
    }
    if input.retry_count < 0 || input.input_tokens < 0 || input.output_tokens < 0 {
        return Err("counts must be zero or greater".into());
    }
    for (name, list) in [
        ("contextIds", &input.context_ids),
        ("skillIds", &input.skill_ids),
        ("ruleIds", &input.rule_ids),
    ] {
        if list.len() > MAX_FEEDBACK_IDS {
            return Err(format!("{name} holds at most {MAX_FEEDBACK_IDS} ids"));
        }
        if list.iter().any(|id| id.len() > MAX_FEEDBACK_FIELD_BYTES) {
            return Err(format!(
                "each id in {name} is at most {MAX_FEEDBACK_FIELD_BYTES} bytes"
            ));
        }
    }
    for (name, value) in [
        ("executionId", &input.execution_id),
        ("taskCategory", &input.task_category),
    ] {
        if value.len() > MAX_FEEDBACK_FIELD_BYTES {
            return Err(format!(
                "{name} is at most {MAX_FEEDBACK_FIELD_BYTES} bytes"
            ));
        }
    }
    let blob = format!(
        "{} {} {} {} {} {}",
        input.execution_id,
        input.task_category,
        input.context_ids.join(" "),
        input.skill_ids.join(" "),
        input.rule_ids.join(" "),
        input.source_text
    );
    if secret_shaped(&blob) {
        return Err("secret-shaped text is excluded from feedback".into());
    }
    if !input.source_text.is_empty() {
        return Err("source text is not stored".into());
    }
    Ok(FeedbackRow {
        id: uuid::Uuid::new_v4().to_string(),
        project_id: input.project_id.clone(),
        execution_id: input.execution_id.clone(),
        task_category: input.task_category.clone(),
        context_ids: json!(input.context_ids).to_string(),
        skill_ids: json!(input.skill_ids).to_string(),
        rule_ids: json!(input.rule_ids).to_string(),
        outcome: input.outcome.clone(),
        context_attribution: "unverified".into(),
        retry_count: input.retry_count,
        input_tokens: input.input_tokens,
        output_tokens: input.output_tokens,
    })
}

/// True when `text` looks like it holds a credential: a `key=value` or `key: value` pair for a
/// password, secret, token or API key (spaces around the separator allowed), a bearer header,
/// a private key block, or a token ax's shared redaction would mask (`sk-…`, GitHub and AWS
/// keys, long hex or base64 runs).
pub fn secret_shaped(text: &str) -> bool {
    let compact = text
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(" =", "=")
        .replace("= ", "=")
        .replace(" :", ":")
        .replace(": ", ":");
    let pattern_hit = [
        "api_key=",
        "api_key:",
        "api-key=",
        "api-key:",
        "apikey",
        "password=",
        "password:",
        "passwd=",
        "pwd=",
        "secret=",
        "secret:",
        "aws_secret",
        "client_secret",
        "token=",
        "access_token",
        "refresh_token",
        "auth_token",
        "bearer ",
        "begin private key",
        "begin rsa private key",
        "begin openssh private key",
    ]
    .iter()
    .any(|pattern| compact.contains(pattern));
    pattern_hit || ax_memory::redact_secrets(text) != text
}

/// Registered metrics that clear the promotion gate on the built-in fixture.
#[cfg(test)]
pub(crate) const PASSING_EVIDENCE: &str = r#"{"bakeoff":{"families":{"named_trigger":{"scored":1,"hits":1},"paraphrase":{"scored":1,"hits":1},"abstention":{"scored":1,"hits":1},"delivery_budget":{"scored":1,"hits":1}}}}"#;

/// Gates promotion of one registered model version on an explicit command and
/// on that version's own bake-off evidence: the `bakeoff.families` object in
/// the metrics it was registered with (`{family: {scored, hits}}`).
///
/// The evidence must score the same questions per family as the built-in
/// coding fixture, keep every locked family fully hit, and beat hash-v1 on
/// paraphrase. hash-v1 is scored on the fixture in this call, not read from a
/// stored constant.
pub async fn promotion_allowed(explicit: bool, metrics_json: &str) -> Result<(), String> {
    if !explicit {
        return Err("promotion requires an explicit command".into());
    }
    let metrics: serde_json::Value = serde_json::from_str(metrics_json).unwrap_or(Value::Null);
    let Some(candidate) = metrics["bakeoff"]["families"].as_object() else {
        return Err(
            "promotion refused: the model version has no bake-off evidence; register it with metrics.bakeoff.families".into(),
        );
    };
    let report = crate::bakeoff::baseline_report(crate::CODING_FIXTURE).await?;
    let baseline = &report["families"];
    for (family, _) in crate::bakeoff::FAMILIES {
        let expected = baseline[family]["scored"].as_u64().unwrap_or(0);
        let scored = candidate
            .get(family)
            .and_then(|value| value["scored"].as_u64());
        if scored != Some(expected) {
            return Err(format!(
                "promotion refused: the bake-off evidence does not score the built-in fixture ({family} scored {scored:?}, expected {expected})"
            ));
        }
    }
    if !locked_families_clear(&json!({ "families": candidate })) {
        return Err(
            "promotion refused: the candidate misses questions in the locked families".into(),
        );
    }
    let candidate: std::collections::HashMap<String, Value> = candidate
        .iter()
        .map(|(family, value)| (family.clone(), value.clone()))
        .collect();
    if !crate::lexical::beats_baseline(baseline, &candidate) {
        return Err(
            "promotion refused: the candidate does not beat hash-v1 on the built-in fixture".into(),
        );
    }
    Ok(())
}

/// Locked families must score and hit on every question (paraphrase may miss).
pub fn locked_families_clear(report: &serde_json::Value) -> bool {
    for family in ["named_trigger", "abstention", "delivery_budget"] {
        let scored = report["families"][family]["scored"].as_u64().unwrap_or(0);
        let hits = report["families"][family]["hits"].as_u64().unwrap_or(0);
        if scored == 0 || hits < scored {
            return false;
        }
    }
    true
}

/// Deterministic train/validation split. `validation_ratio` in (0,1); seed makes it reproducible.
pub fn split_dataset(
    rows: &[Value],
    validation_ratio: f64,
    seed: u64,
) -> Result<(Vec<Value>, Vec<Value>), String> {
    if !(0.0..1.0).contains(&validation_ratio) {
        return Err("validation_ratio must be in [0,1)".into());
    }
    if rows.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let mut indexed: Vec<(u64, usize)> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let key = row
                .get("taskCategory")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
                + "|"
                + &i.to_string();
            (stable_hash(seed, &key), i)
        })
        .collect();
    indexed.sort_by_key(|(h, i)| (*h, *i));
    let val_count = ((rows.len() as f64) * validation_ratio).floor() as usize;
    let mut train = Vec::new();
    let mut validation = Vec::new();
    for (pos, (_h, i)) in indexed.into_iter().enumerate() {
        if pos < val_count {
            validation.push(rows[i].clone());
        } else {
            train.push(rows[i].clone());
        }
    }
    Ok((train, validation))
}

fn stable_hash(seed: u64, text: &str) -> u64 {
    let mut h = seed ^ 0x9e3779b97f4a7c15;
    for b in text.as_bytes() {
        h = h.wrapping_mul(0x100000001b3).wrapping_add(*b as u64);
    }
    h
}

fn outcome_weight(outcome: &str) -> f64 {
    match outcome {
        "success" => 1.0,
        "partial" => 0.5,
        "failure" => -0.5,
        _ => 0.0,
    }
}

/// Controlled ranking-learning: frequency priors from train outcomes; scored on validation.
/// Never promotes. Not neural inference.
pub fn train_ranking_prior(
    rows: &[Value],
    validation_ratio: f64,
    seed: u64,
    dataset_version: &str,
    model_version: &str,
) -> Result<Value, String> {
    let (train, validation) = split_dataset(rows, validation_ratio, seed)?;
    let mut weights: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    for row in &train {
        let outcome = row
            .get("outcome")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let w = outcome_weight(outcome);
        if w == 0.0 {
            continue;
        }
        for key in ["contextIds", "skillIds", "ruleIds"] {
            if let Some(arr) = row.get(key).and_then(|v| v.as_array()) {
                for id in arr.iter().filter_map(|v| v.as_str()) {
                    if id.is_empty() {
                        continue;
                    }
                    *weights.entry(format!("{key}:{id}")).or_insert(0.0) += w;
                }
            }
        }
    }
    let metrics = evaluate_prior(&weights, &validation);
    Ok(json!({
        "kind": "measured",
        "method": "ranking-prior-frequency",
        "claimsNeuralInference": false,
        "promoted": false,
        "autoPromote": false,
        "datasetVersion": dataset_version,
        "modelVersion": model_version,
        "seed": seed,
        "validationRatio": validation_ratio,
        "trainSize": train.len(),
        "validationSize": validation.len(),
        "weightCount": weights.len(),
        "weights": weights,
        "metrics": metrics,
        "inferenceSeparated": true,
        "note": "Artifact only — register/promote explicitly; production ranking unchanged"
    }))
}

/// Experimental weights keyed by project. Rows with no project id are skipped.
/// Production `train_ranking_prior` does not call this.
pub fn project_scoped_weights(rows: &[Value]) -> std::collections::BTreeMap<String, f64> {
    let mut weights = std::collections::BTreeMap::new();
    for row in rows {
        let Some(project) = row
            .get("projectId")
            .and_then(|v| v.as_str())
            .filter(|id| !id.is_empty())
        else {
            continue;
        };
        let outcome = row
            .get("outcome")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let weight = outcome_weight(outcome);
        if weight == 0.0 {
            continue;
        }
        for key in ["contextIds", "skillIds", "ruleIds"] {
            if let Some(ids) = row.get(key).and_then(|v| v.as_array()) {
                for id in ids.iter().filter_map(|v| v.as_str()) {
                    if id.is_empty() {
                        continue;
                    }
                    *weights
                        .entry(format!("{project}\t{key}:{id}"))
                        .or_insert(0.0) += weight;
                }
            }
        }
    }
    weights
}

fn evaluate_prior(
    weights: &std::collections::BTreeMap<String, f64>,
    validation: &[Value],
) -> Value {
    if validation.is_empty() {
        return json!({
            "kind": "not_measured",
            "reason": "empty validation set"
        });
    }
    let mut scored = 0u64;
    let mut agree = 0u64;
    for row in validation {
        let outcome = row
            .get("outcome")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let target = outcome_weight(outcome);
        if target == 0.0 {
            continue;
        }
        let mut score = 0.0;
        let mut n = 0u64;
        for key in ["contextIds", "skillIds", "ruleIds"] {
            if let Some(arr) = row.get(key).and_then(|v| v.as_array()) {
                for id in arr.iter().filter_map(|v| v.as_str()) {
                    if let Some(w) = weights.get(&format!("{key}:{id}")) {
                        score += *w;
                        n += 1;
                    }
                }
            }
        }
        if n == 0 {
            continue;
        }
        scored += 1;
        let pred: f64 = if score >= 0.0 { 1.0 } else { -1.0 };
        if pred.signum() == target.signum() {
            agree += 1;
        }
    }
    json!({
        "kind": "measured",
        "validationScored": scored,
        "signAgreement": if scored == 0 { Value::Null } else { json!(agree as f64 / scored as f64) }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> FeedbackInput {
        FeedbackInput {
            project_id: "p".into(),
            execution_id: "exec-1".into(),
            task_category: "memory".into(),
            context_ids: vec!["ctx".into()],
            skill_ids: vec!["recall".into()],
            rule_ids: vec![],
            outcome: "success".into(),
            retry_count: 0,
            input_tokens: 10,
            output_tokens: 4,
            source_text: String::new(),
        }
    }

    #[test]
    fn success_does_not_mark_context_useful() {
        let row = validate_feedback(&sample()).unwrap();
        assert_eq!(row.context_attribution, "unverified");
    }

    #[test]
    fn secret_and_source_are_rejected() {
        let mut secret = sample();
        secret.task_category = "api_key=abcd".into();
        assert!(validate_feedback(&secret).is_err());
        let mut source = sample();
        source.source_text = "fn main() {}".into();
        assert!(validate_feedback(&source).unwrap_err().contains("source"));
    }

    #[test]
    fn a_secret_in_the_execution_id_is_rejected() {
        let mut secret = sample();
        secret.execution_id = "password=hunter2".into();
        assert!(validate_feedback(&secret)
            .unwrap_err()
            .contains("secret-shaped"));
    }

    #[test]
    fn wider_secret_shapes_are_rejected() {
        for text in [
            "password = hunter2",
            "token=abc123",
            "Bearer eyJhbGciOiJIUzI1NiJ9",
            "apikey abc",
            "aws_secret_access_key",
            "sk-abcdefghijklmnopqrstuvwxyz",
            "ghp_abcdefghijklmnopqrstuvwxyz12",
        ] {
            let mut secret = sample();
            secret.context_ids = vec![text.into()];
            assert!(validate_feedback(&secret).is_err(), "accepted {text:?}");
        }
        // Ordinary ids and categories still pass.
        let mut plain = sample();
        plain.context_ids = vec!["mem:token-budget".into(), "rule:no-secrets".into()];
        plain.task_category = "parser".into();
        assert!(validate_feedback(&plain).is_ok());
    }

    #[test]
    fn oversized_feedback_is_rejected() {
        let mut many = sample();
        many.context_ids = (0..=MAX_FEEDBACK_IDS).map(|i| format!("m{i}")).collect();
        assert!(validate_feedback(&many).unwrap_err().contains("at most"));
        let mut long_id = sample();
        long_id.rule_ids = vec!["r".repeat(MAX_FEEDBACK_FIELD_BYTES + 1)];
        assert!(validate_feedback(&long_id).unwrap_err().contains("bytes"));
        let mut long_exec = sample();
        long_exec.execution_id = "e".repeat(MAX_FEEDBACK_FIELD_BYTES + 1);
        assert!(validate_feedback(&long_exec).unwrap_err().contains("bytes"));
        let mut at_limit = sample();
        at_limit.context_ids = (0..MAX_FEEDBACK_IDS).map(|i| format!("m{i}")).collect();
        // Not a hex digit: a long hex run is secret-shaped.
        at_limit.task_category = "x".repeat(MAX_FEEDBACK_FIELD_BYTES);
        assert!(validate_feedback(&at_limit).is_ok());
    }

    #[test]
    fn counts_may_be_zero_but_not_negative() {
        let with = |retry_count, input_tokens, output_tokens| FeedbackInput {
            retry_count,
            input_tokens,
            output_tokens,
            ..sample()
        };
        assert!(validate_feedback(&with(0, 0, 0)).is_ok());
        for (retry, input, output) in [(-1, 0, 0), (0, -1, 0), (0, 0, -1)] {
            let error = validate_feedback(&with(retry, input, output)).unwrap_err();
            assert_eq!(
                error, "counts must be zero or greater",
                "{retry} {input} {output}"
            );
        }
    }

    #[tokio::test]
    async fn automatic_promotion_flag_is_not_enough() {
        assert_eq!(
            promotion_allowed(false, PASSING_EVIDENCE)
                .await
                .unwrap_err(),
            "promotion requires an explicit command"
        );
        assert!(promotion_allowed(true, PASSING_EVIDENCE).await.is_ok());
    }

    #[tokio::test]
    async fn a_candidate_without_its_own_bakeoff_evidence_is_refused() {
        for metrics in ["{}", "not json", r#"{"bakeoff":{}}"#] {
            let error = promotion_allowed(true, metrics).await.unwrap_err();
            assert!(error.contains("no bake-off evidence"), "{metrics}: {error}");
        }
    }

    #[tokio::test]
    async fn a_candidate_must_beat_the_measured_baseline_and_keep_the_locked_families() {
        let tie = PASSING_EVIDENCE.replace(
            r#""paraphrase":{"scored":1,"hits":1}"#,
            r#""paraphrase":{"scored":1,"hits":0}"#,
        );
        assert_ne!(tie, PASSING_EVIDENCE);
        let error = promotion_allowed(true, &tie).await.unwrap_err();
        assert!(error.contains("does not beat"), "{error}");
        let regressed = PASSING_EVIDENCE.replace(
            r#""abstention":{"scored":1,"hits":1}"#,
            r#""abstention":{"scored":1,"hits":0}"#,
        );
        let error = promotion_allowed(true, &regressed).await.unwrap_err();
        assert!(error.contains("locked families"), "{error}");
        let other_fixture = PASSING_EVIDENCE.replace(
            r#""named_trigger":{"scored":1,"hits":1}"#,
            r#""named_trigger":{"scored":7,"hits":7}"#,
        );
        let error = promotion_allowed(true, &other_fixture).await.unwrap_err();
        assert!(error.contains("built-in fixture"), "{error}");
    }

    #[test]
    fn locked_families_clear_requires_hits_on_each_locked_family() {
        let ok = serde_json::json!({
            "families": {
                "named_trigger": { "scored": 1, "hits": 1 },
                "abstention": { "scored": 1, "hits": 1 },
                "delivery_budget": { "scored": 1, "hits": 1 },
                "paraphrase": { "scored": 1, "hits": 0 }
            }
        });
        assert!(locked_families_clear(&ok));
        let bad = serde_json::json!({
            "families": {
                "named_trigger": { "scored": 1, "hits": 0 },
                "abstention": { "scored": 1, "hits": 1 },
                "delivery_budget": { "scored": 1, "hits": 1 }
            }
        });
        assert!(!locked_families_clear(&bad));
    }

    #[test]
    fn feedback_needs_an_execution_id_a_project_and_a_known_outcome() {
        let with = |execution_id: &str, project_id: &str, outcome: &str| FeedbackInput {
            execution_id: execution_id.into(),
            project_id: project_id.into(),
            outcome: outcome.into(),
            ..sample()
        };
        for outcome in ["success", "partial", "failure", "unknown"] {
            assert!(
                validate_feedback(&with("e1", "p", outcome)).is_ok(),
                "{outcome}"
            );
        }
        for (execution_id, project_id, outcome, error) in [
            (" ", "p", "success", "execution id is required"),
            ("e1", "", "success", "project id is required"),
            (
                "e1",
                "p",
                "done",
                "outcome must be success, partial, failure, or unknown",
            ),
        ] {
            let refused = validate_feedback(&with(execution_id, project_id, outcome));
            assert_eq!(refused.unwrap_err(), error);
        }
    }
}

#[cfg(test)]
mod phase5_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn split_dataset_is_reproducible_and_disjoint() {
        let rows: Vec<Value> = (0..10)
            .map(|i| json!({ "taskCategory": format!("t{i}"), "outcome": "success", "contextIds": [format!("c{i}")] }))
            .collect();
        let (a_train, a_val) = split_dataset(&rows, 0.3, 7).unwrap();
        let (b_train, b_val) = split_dataset(&rows, 0.3, 7).unwrap();
        assert_eq!(a_train, b_train);
        assert_eq!(a_val, b_val);
        assert_eq!(a_train.len() + a_val.len(), 10);
        let train_cats: std::collections::BTreeSet<_> = a_train
            .iter()
            .map(|r| r["taskCategory"].as_str().unwrap())
            .collect();
        let val_cats: std::collections::BTreeSet<_> = a_val
            .iter()
            .map(|r| r["taskCategory"].as_str().unwrap())
            .collect();
        assert!(train_cats.is_disjoint(&val_cats));
    }

    #[test]
    fn project_scoped_weights_do_not_cross_projects() {
        let rows = vec![
            json!({"projectId":"a","outcome":"success","contextIds":["memory:m1"]}),
            json!({"projectId":"b","outcome":"failure","contextIds":["memory:m1"]}),
            json!({"outcome":"success","contextIds":["memory:m1"]}),
        ];
        let weights = project_scoped_weights(&rows);
        assert_eq!(weights.get("a\tcontextIds:memory:m1").copied(), Some(1.0));
        assert_eq!(weights.get("b\tcontextIds:memory:m1").copied(), Some(-0.5));
        assert!(weights
            .keys()
            .all(|key| key.starts_with("a\t") || key.starts_with("b\t")));
    }

    #[test]
    fn train_ranking_prior_does_not_auto_promote_and_skips_neural_claim() {
        let rows = vec![
            json!({"taskCategory":"a","outcome":"success","contextIds":["memory:m1"],"skillIds":["s1"],"ruleIds":[]}),
            json!({"taskCategory":"a","outcome":"failure","contextIds":["memory:m2"],"skillIds":[],"ruleIds":[]}),
            json!({"taskCategory":"b","outcome":"success","contextIds":["memory:m1"],"skillIds":["s1"],"ruleIds":["r1"]}),
            json!({"taskCategory":"b","outcome":"partial","contextIds":["memory:m3"],"skillIds":[],"ruleIds":[]}),
        ];
        let report = train_ranking_prior(&rows, 0.25, 1, "ds-1", "rank-prior-1").unwrap();
        assert_eq!(report["promoted"], false);
        assert_eq!(report["autoPromote"], false);
        assert_eq!(report["claimsNeuralInference"], false);
        assert_eq!(report["datasetVersion"], "ds-1");
        assert_eq!(report["modelVersion"], "rank-prior-1");
        assert!(report["weightCount"].as_u64().unwrap() >= 1);
        assert_eq!(report["inferenceSeparated"], true);
    }

    #[test]
    fn split_rejects_bad_ratio() {
        assert!(split_dataset(&[], 1.0, 0).is_err());
        assert!(split_dataset(&[], -0.1, 0).is_err());
    }
}
