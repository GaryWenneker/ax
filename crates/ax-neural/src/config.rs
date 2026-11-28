use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct NeuralConfig {
    pub enabled: bool,
    pub mode: String,
    pub learning_enabled: bool,
    pub preferred_backend: String,
    pub fallback_backend: String,
    pub semantic_enabled: bool,
    pub similarity_threshold: f64,
    pub graph_enabled: bool,
    pub graph_learning_enabled: bool,
    pub ranking_enabled: bool,
    pub reranking_enabled: bool,
    pub prediction_enabled: bool,
    pub minimum_confidence: f64,
    pub optimizer_enabled: bool,
    pub respect_token_budget: bool,
    pub feedback_collection: bool,
    pub automatic_training: bool,
    pub automatic_model_promotion: bool,
    pub distribution_enabled: bool,
    pub share_learning_data: bool,
    pub config_error: Option<String>,
}

impl Default for NeuralConfig {
    fn default() -> Self {
        Self::required()
    }
}

impl NeuralConfig {
    /// Neural is required. `enabled: false` is not a supported state.
    pub fn required() -> Self {
        Self {
            enabled: true,
            mode: "local".into(),
            learning_enabled: false,
            preferred_backend: "auto".into(),
            fallback_backend: "cpu".into(),
            semantic_enabled: true,
            similarity_threshold: 0.70,
            graph_enabled: false,
            graph_learning_enabled: false,
            ranking_enabled: true,
            reranking_enabled: false,
            prediction_enabled: false,
            minimum_confidence: 0.75,
            optimizer_enabled: true,
            respect_token_budget: true,
            feedback_collection: false,
            automatic_training: false,
            automatic_model_promotion: false,
            distribution_enabled: false,
            share_learning_data: false,
            config_error: None,
        }
    }

    pub fn ranking_active(&self) -> bool {
        self.enabled && self.ranking_enabled
    }

    pub fn feedback_active(&self) -> bool {
        self.feedback_collection
    }

    pub fn prediction_active(&self) -> bool {
        self.prediction_enabled
    }
}

pub fn load(root: &std::path::Path) -> NeuralConfig {
    let path = root.join("ax.json");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return NeuralConfig::required();
        }
        Err(error) => {
            let mut cfg = NeuralConfig::required();
            cfg.config_error = Some(format!("could not read ax.json: {error}"));
            return cfg;
        }
    };
    let value: Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(error) => {
            let mut cfg = NeuralConfig::required();
            cfg.config_error = Some(format!("ax.json is not valid JSON: {error}"));
            return cfg;
        }
    };
    let Some(neural) = value.get("neural") else {
        return NeuralConfig::required();
    };
    lock_required(parse_neural(neural))
}

fn lock_required(mut cfg: NeuralConfig) -> NeuralConfig {
    if !cfg.enabled || !cfg.ranking_enabled {
        let disable = "neural cannot be disabled";
        cfg.config_error = Some(match cfg.config_error.take() {
            Some(existing) => format!("{disable}; {existing}"),
            None => disable.to_string(),
        });
    }
    cfg.enabled = true;
    cfg.ranking_enabled = true;
    cfg.automatic_training = false;
    cfg.automatic_model_promotion = false;
    cfg
}

const TOP_KEYS: &[&str] = &[
    "enabled",
    "mode",
    "learning_enabled",
    "runtime",
    "semantic",
    "graph",
    "ranking",
    "prediction",
    "optimizer",
    "learning",
    "distribution",
];

/// Collects every problem in the section. An invalid field keeps its default
/// and every valid field is still applied.
#[derive(Default)]
struct Problems(Vec<String>);

impl Problems {
    fn take<T>(&mut self, result: Result<Option<T>, String>) -> Option<T> {
        result.unwrap_or_else(|error| {
            self.0.push(error);
            None
        })
    }

    fn section<'a>(
        &mut self,
        obj: &'a Map<String, Value>,
        name: &str,
        known: &[&str],
    ) -> Option<&'a Map<String, Value>> {
        let value = obj.get(name)?;
        let Some(section) = value.as_object() else {
            self.0.push(format!("neural.{name} must be an object"));
            return None;
        };
        self.unknown(section, &format!("neural.{name}."), known);
        Some(section)
    }

    fn unknown(&mut self, obj: &Map<String, Value>, prefix: &str, known: &[&str]) {
        for key in obj.keys().filter(|key| !known.contains(&key.as_str())) {
            self.0.push(format!("{prefix}{key} is not a known setting"));
        }
    }

    fn bool(&mut self, obj: &Map<String, Value>, key: &str) -> Option<bool> {
        self.take(bool_field(obj, key))
    }

    fn str(&mut self, obj: &Map<String, Value>, key: &str) -> Option<String> {
        self.take(str_field(obj, key))
    }

    fn unit(&mut self, obj: &Map<String, Value>, key: &str, path: &str) -> Option<f64> {
        let value = self.take(f64_field(obj, key))?;
        if (0.0..=1.0).contains(&value) {
            Some(value)
        } else {
            self.0.push(format!("{path} must be between 0 and 1"));
            None
        }
    }
}

fn parse_neural(value: &Value) -> NeuralConfig {
    let mut cfg = NeuralConfig::required();
    let Some(obj) = value.as_object() else {
        cfg.config_error = Some("neural must be an object".into());
        return cfg;
    };
    let mut p = Problems::default();
    p.unknown(obj, "neural.", TOP_KEYS);
    if let Some(enabled) = p.bool(obj, "enabled") {
        cfg.enabled = enabled;
    }
    if let Some(mode) = p.str(obj, "mode") {
        if matches!(mode.as_str(), "local" | "central" | "hybrid") {
            cfg.mode = mode;
        } else {
            p.0.push(format!(
                "neural.mode {mode} is not local, central, or hybrid"
            ));
        }
    }
    if let Some(value) = p.bool(obj, "learning_enabled") {
        cfg.learning_enabled = value;
    }
    if let Some(runtime) = p.section(obj, "runtime", &["preferred_backend", "fallback_backend"]) {
        if let Some(value) = p.str(runtime, "preferred_backend") {
            cfg.preferred_backend = value;
        }
        if let Some(value) = p.str(runtime, "fallback_backend") {
            if value != "cpu" && value != "hash" {
                p.0.push("neural.runtime.fallback_backend must be cpu or hash".into());
            }
            cfg.fallback_backend = "cpu".into();
        }
    }
    if let Some(semantic) = p.section(obj, "semantic", &["enabled", "similarity_threshold"]) {
        if let Some(value) = p.bool(semantic, "enabled") {
            cfg.semantic_enabled = value;
        }
        let path = "neural.semantic.similarity_threshold";
        if let Some(value) = p.unit(semantic, "similarity_threshold", path) {
            cfg.similarity_threshold = value;
        }
    }
    if let Some(graph) = p.section(obj, "graph", &["enabled", "learning_enabled"]) {
        if let Some(value) = p.bool(graph, "enabled") {
            cfg.graph_enabled = value;
        }
        if let Some(value) = p.bool(graph, "learning_enabled") {
            cfg.graph_learning_enabled = value;
        }
    }
    if let Some(ranking) = p.section(obj, "ranking", &["enabled", "reranking_enabled"]) {
        if let Some(value) = p.bool(ranking, "enabled") {
            cfg.ranking_enabled = value;
        }
        if let Some(value) = p.bool(ranking, "reranking_enabled") {
            cfg.reranking_enabled = value;
        }
    }
    if let Some(prediction) = p.section(obj, "prediction", &["enabled", "minimum_confidence"]) {
        if let Some(value) = p.bool(prediction, "enabled") {
            cfg.prediction_enabled = value;
        }
        let path = "neural.prediction.minimum_confidence";
        if let Some(value) = p.unit(prediction, "minimum_confidence", path) {
            cfg.minimum_confidence = value;
        }
    }
    if let Some(optimizer) = p.section(obj, "optimizer", &["enabled", "respect_token_budget"]) {
        if let Some(value) = p.bool(optimizer, "enabled") {
            cfg.optimizer_enabled = value;
        }
        if let Some(value) = p.bool(optimizer, "respect_token_budget") {
            cfg.respect_token_budget = value;
        }
    }
    let learning_keys = [
        "feedback_collection",
        "automatic_training",
        "automatic_model_promotion",
    ];
    if let Some(learning) = p.section(obj, "learning", &learning_keys) {
        if let Some(value) = p.bool(learning, "feedback_collection") {
            cfg.feedback_collection = value;
        }
        let training = p.bool(learning, "automatic_training").unwrap_or(false);
        let promotion = p
            .bool(learning, "automatic_model_promotion")
            .unwrap_or(false);
        if training || promotion {
            p.0.push("automatic_training and automatic_model_promotion must stay false".into());
        }
    }
    if let Some(distribution) = p.section(obj, "distribution", &["enabled", "share_learning_data"])
    {
        if let Some(value) = p.bool(distribution, "enabled") {
            cfg.distribution_enabled = value;
        }
        if let Some(value) = p.bool(distribution, "share_learning_data") {
            cfg.share_learning_data = value;
        }
    }
    if !p.0.is_empty() {
        cfg.config_error = Some(p.0.join("; "));
    }
    cfg
}

fn bool_field(obj: &Map<String, Value>, key: &str) -> Result<Option<bool>, String> {
    match obj.get(key) {
        None => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(format!("neural field {key} must be a boolean")),
    }
}

fn str_field(obj: &Map<String, Value>, key: &str) -> Result<Option<String>, String> {
    match obj.get(key) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(format!("neural field {key} must be a string")),
    }
}

fn f64_field(obj: &Map<String, Value>, key: &str) -> Result<Option<f64>, String> {
    match obj.get(key) {
        None => Ok(None),
        Some(Value::Number(value)) => value
            .as_f64()
            .map(Some)
            .ok_or_else(|| format!("neural field {key} must be a number")),
        Some(_) => Err(format!("neural field {key} must be a number")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_stays_required() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = load(dir.path());
        assert!(cfg.enabled);
        assert!(cfg.config_error.is_none());
        assert!(cfg.ranking_active());
    }

    #[test]
    fn invalid_json_stays_required_and_keeps_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ax.json");
        std::fs::write(&path, "{").unwrap();
        let cfg = load(dir.path());
        assert!(cfg.enabled);
        assert!(cfg.ranking_active());
        assert!(cfg.config_error.is_some());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "{");
    }

    #[test]
    fn other_keys_are_left_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ax.json");
        std::fs::write(
            &path,
            r#"{"context":{"budgetTokens":3000},"neural":{"enabled":false,"extra":1}}"#,
        )
        .unwrap();
        let cfg = load(dir.path());
        assert!(cfg.enabled);
        assert!(cfg.ranking_active());
        assert_eq!(
            cfg.config_error.as_deref(),
            Some("neural cannot be disabled; neural.extra is not a known setting")
        );
        assert!(std::fs::read_to_string(path)
            .unwrap()
            .contains("budgetTokens"));
    }

    #[test]
    fn bad_threshold_stays_required() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("ax.json"),
            r#"{"neural":{"enabled":true,"semantic":{"similarity_threshold":1.5}}}"#,
        )
        .unwrap();
        let cfg = load(dir.path());
        assert!(cfg.enabled);
        assert!(cfg.ranking_active());
        assert!((cfg.similarity_threshold - 0.70).abs() < f64::EPSILON);
        assert!(cfg.config_error.unwrap().contains("similarity_threshold"));
    }

    #[test]
    fn enabled_turns_ranking_on_by_default() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ax.json"), r#"{"neural":{"enabled":true}}"#).unwrap();
        let cfg = load(dir.path());
        assert!(cfg.ranking_active());
        assert!(!cfg.prediction_active());
        assert!(!cfg.feedback_active());
        assert!(!cfg.automatic_model_promotion);
    }

    #[test]
    fn unreadable_ax_json_stays_required_and_names_the_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("ax.json")).unwrap();
        let cfg = load(dir.path());
        assert!(cfg.enabled);
        assert!(cfg.ranking_active());
        let error = cfg.config_error.expect("read failure is reported");
        assert!(error.contains("could not read ax.json"), "{error}");
    }

    #[test]
    fn disabled_request_keeps_the_automatic_promotion_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("ax.json"),
            r#"{"neural":{"enabled":false,"learning":{"automatic_model_promotion":true}}}"#,
        )
        .unwrap();
        let cfg = load(dir.path());
        assert!(cfg.enabled);
        assert!(cfg.ranking_active());
        assert!(!cfg.automatic_model_promotion);
        let error = cfg.config_error.expect("both problems are reported");
        assert!(error.contains("neural cannot be disabled"), "{error}");
        assert!(error.contains("automatic_model_promotion"), "{error}");
    }

    #[test]
    fn automatic_promotion_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("ax.json"),
            r#"{"neural":{"enabled":true,"learning":{"automatic_model_promotion":true}}}"#,
        )
        .unwrap();
        let cfg = load(dir.path());
        assert!(cfg.enabled);
        assert!(cfg.ranking_active());
        assert!(!cfg.automatic_model_promotion);
        assert!(cfg.config_error.is_some());
    }

    fn load_neural(neural: Value) -> NeuralConfig {
        let dir = tempfile::tempdir().unwrap();
        let text = serde_json::json!({ "neural": neural }).to_string();
        std::fs::write(dir.path().join("ax.json"), text).unwrap();
        load(dir.path())
    }

    #[test]
    fn ranking_turned_off_stays_on_and_says_so() {
        let cfg = load_neural(serde_json::json!({ "ranking": { "enabled": false } }));
        assert!(cfg.ranking_active());
        assert_eq!(
            cfg.config_error.as_deref(),
            Some("neural cannot be disabled")
        );
    }

    #[test]
    fn fallback_backend_takes_cpu_or_hash_as_cpu_and_nothing_else() {
        for (backend, error) in [
            ("cpu", None),
            ("hash", None),
            (
                "gpu",
                Some("neural.runtime.fallback_backend must be cpu or hash"),
            ),
        ] {
            let cfg =
                load_neural(serde_json::json!({ "runtime": { "fallback_backend": backend } }));
            assert_eq!(cfg.config_error.as_deref(), error, "{backend}");
            assert_eq!(cfg.fallback_backend, "cpu", "{backend}");
        }
    }

    #[test]
    fn minimum_confidence_is_set_between_0_and_1_and_refused_outside() {
        let cfg = load_neural(serde_json::json!({
            "prediction": { "enabled": true, "minimum_confidence": 0.6 }
        }));
        assert_eq!(cfg.config_error, None);
        assert_eq!(cfg.minimum_confidence, 0.6);
        assert!(cfg.prediction_active());
        let cfg = load_neural(serde_json::json!({ "prediction": { "minimum_confidence": 1.5 } }));
        assert_eq!(cfg.minimum_confidence, 0.75);
        let error = cfg.config_error.expect("out of range is reported");
        assert!(error.contains("minimum_confidence"), "{error}");
    }

    #[test]
    fn a_malformed_neural_value_is_reported_and_neural_stays_required() {
        let cases = [
            (serde_json::json!(true), "neural must be an object"),
            (
                serde_json::json!({ "mode": "remote" }),
                "neural.mode remote is not local, central, or hybrid",
            ),
            (
                serde_json::json!({ "mode": 1 }),
                "neural field mode must be a string",
            ),
            (
                serde_json::json!({ "enabled": "yes" }),
                "neural field enabled must be a boolean",
            ),
            (
                serde_json::json!({ "semantic": { "similarity_threshold": "high" } }),
                "neural field similarity_threshold must be a number",
            ),
        ];
        let sections = [
            "runtime",
            "semantic",
            "graph",
            "ranking",
            "prediction",
            "optimizer",
            "learning",
            "distribution",
        ];
        let not_objects = sections.iter().map(|section| {
            let error = format!("neural.{section} must be an object");
            (serde_json::json!({ *section: 1 }), error)
        });
        let cases = cases
            .into_iter()
            .map(|(neural, error)| (neural, error.to_string()))
            .chain(not_objects);
        for (neural, error) in cases {
            let mut expected = NeuralConfig::required();
            expected.config_error = Some(error);
            assert_eq!(load_neural(neural.clone()), expected, "{neural}");
        }
    }

    #[test]
    fn runtime_semantic_and_optimizer_values_are_used() {
        let cfg = load_neural(serde_json::json!({
            "runtime": { "preferred_backend": "cpu" },
            "semantic": { "similarity_threshold": 0.8 },
            "optimizer": { "enabled": false, "respect_token_budget": false }
        }));
        assert_eq!(cfg.config_error, None);
        assert_eq!(cfg.preferred_backend, "cpu");
        assert_eq!(cfg.similarity_threshold, 0.8);
        assert!(!cfg.optimizer_enabled);
        assert!(!cfg.respect_token_budget);
    }

    #[test]
    fn one_invalid_field_keeps_the_other_valid_fields() {
        let cfg = load_neural(serde_json::json!({
            "learning": { "feedback_collection": true },
            "semantic": { "similarity_threshold": 1.5 },
            "prediction": { "enabled": true, "minimum_confidence": "high" }
        }));
        assert!(cfg.feedback_active());
        assert!(cfg.prediction_active());
        assert_eq!(cfg.similarity_threshold, 0.70);
        assert_eq!(cfg.minimum_confidence, 0.75);
        let error = cfg.config_error.expect("both problems are reported");
        assert!(error.contains("similarity_threshold"), "{error}");
        assert!(error.contains("minimum_confidence"), "{error}");
    }

    #[test]
    fn unknown_keys_are_reported_with_their_path() {
        let cfg = load_neural(serde_json::json!({
            "prediciton": { "enabled": true },
            "learning": { "feedback_colection": true }
        }));
        let error = cfg.config_error.clone().expect("unknown keys are reported");
        assert!(
            error.contains("neural.prediciton is not a known setting"),
            "{error}"
        );
        assert!(
            error.contains("neural.learning.feedback_colection is not a known setting"),
            "{error}"
        );
        assert!(!cfg.prediction_active());
        assert!(!cfg.feedback_active());
    }

    #[test]
    fn invalid_json_names_the_line_and_column() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ax.json"), "{\n  \"neural\": ,\n}").unwrap();
        let error = load(dir.path())
            .config_error
            .expect("parse error is reported");
        assert!(error.starts_with("ax.json is not valid JSON"), "{error}");
        assert!(error.contains("line 2"), "{error}");
        assert!(error.contains("column"), "{error}");
    }

    #[test]
    fn default_is_the_required_config() {
        assert_eq!(NeuralConfig::default(), NeuralConfig::required());
    }
}
