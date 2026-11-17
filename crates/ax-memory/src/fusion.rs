//! Calibrated fusion helpers and why-recalled signals (Phase 6).

use serde::{Deserialize, Serialize};

/// Shipped offline-tuned weights for normalized recall signals.
pub const W_FTS: f64 = 0.45;
pub const W_VECTOR: f64 = 0.25;
pub const W_ACT_R: f64 = 0.15;
pub const W_GRAPH: f64 = 0.10;
pub const W_USAGE: f64 = 0.05;

/// Abstain when the fused score is below this threshold (0..1 scale).
pub const ABSTAIN_THRESHOLD: f64 = 0.08;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WhyRecalled {
    pub signals: Vec<String>,
    pub matched_terms: Vec<String>,
    pub anchors: Vec<String>,
    pub graph_path: Vec<String>,
}

pub fn fuse_scores(fts: f64, vector: f64, act_r: f64, graph: f64, usage: f64) -> f64 {
    W_FTS * fts + W_VECTOR * vector + W_ACT_R * act_r + W_GRAPH * graph + W_USAGE * usage
}

pub fn should_abstain(fused: f64) -> bool {
    fused < ABSTAIN_THRESHOLD
}

pub fn why_from_hits(
    matched_terms: &[String],
    file_hit: bool,
    symbol_hit: bool,
    act_r: f64,
) -> WhyRecalled {
    let mut signals = Vec::new();
    if !matched_terms.is_empty() {
        signals.push("fts".into());
    }
    if file_hit || symbol_hit {
        signals.push("graph".into());
    }
    if act_r > 0.0 {
        signals.push("act_r".into());
    }
    WhyRecalled {
        signals,
        matched_terms: matched_terms.to_vec(),
        anchors: Vec::new(),
        graph_path: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{fuse_scores, should_abstain, W_FTS, W_VECTOR};

    #[test]
    fn fts_outweighs_vector_and_abstains_low() {
        assert!(W_FTS > W_VECTOR);
        assert!(fuse_scores(1.0, 0.0, 0.0, 0.0, 0.0) > fuse_scores(0.0, 1.0, 0.0, 0.0, 0.0));
        assert!(should_abstain(0.01));
        assert!(!should_abstain(0.5));
    }
}
