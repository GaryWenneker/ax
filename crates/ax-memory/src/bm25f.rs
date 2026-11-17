//! BM25F field-weighted ranking constants (Phase 6).

/// Field weights over title, triggers, tags, anchors, body.
pub const W_TITLE: f64 = 3.0;
pub const W_TRIGGER: f64 = 2.5;
pub const W_TAG: f64 = 1.5;
pub const W_ANCHOR: f64 = 2.0;
pub const W_BODY: f64 = 1.0;

pub const K1: f64 = 1.2;
pub const B: f64 = 0.75;

/// Combine per-field BM25 scores with shipped weights.
pub fn bm25f_combine(title: f64, trigger: f64, tag: f64, anchor: f64, body: f64) -> f64 {
    W_TITLE * title + W_TRIGGER * trigger + W_TAG * tag + W_ANCHOR * anchor + W_BODY * body
}

/// Classic BM25 term score for one field.
pub fn bm25_term(tf: f64, df: f64, docs: f64, field_len: f64, avg_len: f64) -> f64 {
    if tf <= 0.0 || docs <= 0.0 || df <= 0.0 {
        return 0.0;
    }
    let idf = ((docs - df + 0.5) / (df + 0.5)).ln().max(0.0);
    let norm = 1.0 - B + B * (field_len / avg_len.max(1.0));
    idf * ((tf * (K1 + 1.0)) / (tf + K1 * norm))
}

#[cfg(test)]
mod tests {
    use super::{bm25_term, bm25f_combine, B, K1};

    #[test]
    fn title_outweighs_body_and_constants_match_plan() {
        assert!((K1 - 1.2).abs() < f64::EPSILON);
        assert!((B - 0.75).abs() < f64::EPSILON);
        let titled = bm25f_combine(1.0, 0.0, 0.0, 0.0, 0.0);
        let bodied = bm25f_combine(0.0, 0.0, 0.0, 0.0, 1.0);
        assert!(titled > bodied);
        assert!(bm25_term(2.0, 1.0, 100.0, 10.0, 10.0) > 0.0);
        assert_eq!(bm25_term(0.0, 1.0, 100.0, 10.0, 10.0), 0.0);
    }
}
