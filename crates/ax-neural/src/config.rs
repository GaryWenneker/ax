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
        Err(_) => {
            let mut cfg = NeuralConfig::required();
            cfg.config_error = Some("ax.json is not valid JSON".into());
            return cfg;
        }
    };
    let Some(neural) = value.get("neural") else {
        return NeuralConfig::required();
    };
    match parse_neural(neural) {
        Ok(cfg) => lock_required(cfg),
        Err(error) => {
            let mut cfg = NeuralConfig::required();
            cfg.config_error = Some(error);
            cfg
        }
    }
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

fn parse_neural(value: &Value) -> Result<NeuralConfig, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| "neural must be an object".to_string())?;
    let mut cfg = NeuralConfig::required();
    cfg.enabled = bool_field(obj, "enabled")?.unwrap_or(true);
    if let Some(mode) = str_field(obj, "mode")? {
        if !matches!(mode.as_str(), "local" | "central" | "hybrid") {
            return Err(format!(
                "neural.mode {mode} is not local, central, or hybrid"
            ));
        }
        cfg.mode = mode;
    }
    cfg.learning_enabled = bool_field(obj, "learning_enabled")?.unwrap_or(false);
    if let Some(runtime) = obj.get("runtime") {
        let runtime = runtime
            .as_object()
            .ok_or_else(|| "neural.runtime must be an object".to_string())?;
        if let Some(value) = str_field(runtime, "preferred_backend")? {
            cfg.preferred_backend = value;
        }
        if let Some(value) = str_field(runtime, "fallback_backend")? {
            if value != "cpu" && value != "hash" {
                return Err("neural.runtime.fallback_backend must be cpu".into());
            }
            cfg.fallback_backend = "cpu".into();
        }
    }
    if let Some(semantic) = obj.get("semantic") {
        let semantic = semantic
            .as_object()
            .ok_or_else(|| "neural.semantic must be an object".to_string())?;
        cfg.semantic_enabled = bool_field(semantic, "enabled")?.unwrap_or(true);
        if let Some(threshold) = f64_field(semantic, "similarity_threshold")? {
            if !(0.0..=1.0).contains(&threshold) {
                return Err("neural.semantic.similarity_threshold must be between 0 and 1".into());
            }
            cfg.similarity_threshold = threshold;
        }
    }
    if let Some(graph) = obj.get("graph") {
        let graph = graph
            .as_object()
            .ok_or_else(|| "neural.graph must be an object".to_string())?;
        cfg.graph_enabled = bool_field(graph, "enabled")?.unwrap_or(false);
        cfg.graph_learning_enabled = bool_field(graph, "learning_enabled")?.unwrap_or(false);
    }
    if let Some(ranking) = obj.get("ranking") {
        let ranking = ranking
            .as_object()
            .ok_or_else(|| "neural.ranking must be an object".to_string())?;
        cfg.ranking_enabled = bool_field(ranking, "enabled")?.unwrap_or(true);
        cfg.reranking_enabled = bool_field(ranking, "reranking_enabled")?.unwrap_or(false);
    }
    if let Some(prediction) = obj.get("prediction") {
        let prediction = prediction
            .as_object()
            .ok_or_else(|| "neural.prediction must be an object".to_string())?;
        cfg.prediction_enabled = bool_field(prediction, "enabled")?.unwrap_or(false);
        if let Some(confidence) = f64_field(prediction, "minimum_confidence")? {
            if !(0.0..=1.0).contains(&confidence) {
                return Err("neural.prediction.minimum_confidence must be between 0 and 1".into());
            }
            cfg.minimum_confidence = confidence;
        }
    }
    if let Some(optimizer) = obj.get("optimizer") {
        let optimizer = optimizer
            .as_object()
            .ok_or_else(|| "neural.optimizer must be an object".to_string())?;
        cfg.optimizer_enabled = bool_field(optimizer, "enabled")?.unwrap_or(true);
        cfg.respect_token_budget = bool_field(optimizer, "respect_token_budget")?.unwrap_or(true);
    }
    if let Some(learning) = obj.get("learning") {
        let learning = learning
            .as_object()
            .ok_or_else(|| "neural.learning must be an object".to_string())?;
        cfg.feedback_collection = bool_field(learning, "feedback_collection")?.unwrap_or(false);
        cfg.automatic_training = bool_field(learning, "automatic_training")?.unwrap_or(false);
        cfg.automatic_model_promotion =
            bool_field(learning, "automatic_model_promotion")?.unwrap_or(false);
        if cfg.automatic_training || cfg.automatic_model_promotion {
            cfg.automatic_training = false;
            cfg.automatic_model_promotion = false;
            cfg.config_error =
                Some("automatic_training and automatic_model_promotion must stay false".into());
        }
    }
    if let Some(distribution) = obj.get("distribution") {
        let distribution = distribution
            .as_object()
            .ok_or_else(|| "neural.distribution must be an object".to_string())?;
        cfg.distribution_enabled = bool_field(distribution, "enabled")?.unwrap_or(false);
        cfg.share_learning_data = bool_field(distribution, "share_learning_data")?.unwrap_or(false);
    }
    Ok(cfg)
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
            Some("neural cannot be disabled")
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
            ("gpu", Some("neural.runtime.fallback_backend must be cpu")),
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
    fn default_is_the_required_config() {
        assert_eq!(NeuralConfig::default(), NeuralConfig::required());
    }
}
