//! Provider-agnostic cost quote.
//!
//! Token counts times a catalog rate are always [`CostConfidence::Estimated`].
//! [`CostConfidence::Actual`] is only for a billed amount a provider returned,
//! which this function does not invent. A missing count or a missing rate is
//! [`CostConfidence::Unknown`] for that component, and the total stays unknown
//! so a partial sum is never shown as the bill.

use serde::{Deserialize, Serialize};

use crate::pricing::ModelPricing;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostConfidence {
    Actual,
    Estimated,
    Unknown,
}

/// One observed slice of model usage. `None` token fields were not reported.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostUsage {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
    pub cache_write_tokens: Option<i64>,
    pub timestamp_ms: Option<i64>,
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    /// Confidence of the token counts. Dollars computed here are still estimates.
    pub confidence: CostConfidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CostComponent {
    pub usd: Option<f64>,
    pub confidence: CostConfidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Cost {
    pub input: CostComponent,
    pub output: CostComponent,
    pub cache_read: CostComponent,
    pub cache_write: CostComponent,
    /// Present only when every component is known. Otherwise `None`.
    pub total_usd: Option<f64>,
    pub total_confidence: CostConfidence,
}

/// Price `usage` with an explicit catalog row.
///
/// `pricing == None` means the model is not in the user override table or the
/// synced catalog. Built-in savings defaults are not consulted.
pub fn calculate(usage: &CostUsage, pricing: Option<ModelPricing>) -> Cost {
    let _ = usage.confidence;
    let input = price_component(usage.input_tokens, pricing.map(|p| p.input_per_mtok));
    let output = price_component(usage.output_tokens, pricing.map(|p| p.output_per_mtok));
    let cache_read = price_component(
        usage.cache_read_tokens,
        pricing.and_then(|p| p.cache_read_per_mtok),
    );
    let cache_write = price_component(
        usage.cache_write_tokens,
        pricing.and_then(|p| p.cache_write_per_mtok),
    );
    let components = [input, output, cache_read, cache_write];
    let (total_usd, total_confidence) = if components
        .iter()
        .any(|c| c.confidence == CostConfidence::Unknown)
    {
        (None, CostConfidence::Unknown)
    } else {
        let sum = components.iter().filter_map(|c| c.usd).sum();
        (Some(sum), CostConfidence::Estimated)
    };
    Cost {
        input,
        output,
        cache_read,
        cache_write,
        total_usd,
        total_confidence,
    }
}

fn price_component(tokens: Option<i64>, rate_per_mtok: Option<f64>) -> CostComponent {
    let Some(tokens) = tokens else {
        return unknown();
    };
    if tokens < 0 {
        return unknown();
    }
    if tokens == 0 {
        return CostComponent {
            usd: Some(0.0),
            confidence: CostConfidence::Estimated,
        };
    }
    match rate_per_mtok {
        Some(rate) if rate.is_finite() && rate >= 0.0 => CostComponent {
            usd: Some(tokens as f64 / 1_000_000.0 * rate),
            confidence: CostConfidence::Estimated,
        },
        _ => unknown(),
    }
}

fn unknown() -> CostComponent {
    CostComponent {
        usd: None,
        confidence: CostConfidence::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn priced() -> ModelPricing {
        ModelPricing {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
            cache_read_per_mtok: Some(0.30),
            cache_write_per_mtok: Some(3.75),
        }
    }

    fn usage(
        input: Option<i64>,
        output: Option<i64>,
        cache_read: Option<i64>,
        cache_write: Option<i64>,
    ) -> CostUsage {
        CostUsage {
            provider: Some("anthropic".into()),
            model: Some("example".into()),
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: cache_read,
            cache_write_tokens: cache_write,
            timestamp_ms: Some(0),
            session_id: None,
            turn_id: None,
            confidence: CostConfidence::Estimated,
        }
    }

    #[test]
    fn prices_each_token_class_from_the_catalog_rate() {
        let cost = calculate(
            &usage(
                Some(1_000_000),
                Some(1_000_000),
                Some(1_000_000),
                Some(1_000_000),
            ),
            Some(priced()),
        );
        assert_eq!(cost.input.usd, Some(3.0));
        assert_eq!(cost.output.usd, Some(15.0));
        assert_eq!(cost.cache_read.usd, Some(0.30));
        assert_eq!(cost.cache_write.usd, Some(3.75));
        assert_eq!(cost.input.confidence, CostConfidence::Estimated);
        assert_eq!(cost.total_confidence, CostConfidence::Estimated);
        assert!((cost.total_usd.unwrap() - 22.05).abs() < 1e-9);
    }

    #[test]
    fn missing_rate_marks_that_component_and_the_total_unknown() {
        let mut pricing = priced();
        pricing.cache_write_per_mtok = None;
        let cost = calculate(
            &usage(Some(1_000), Some(0), Some(0), Some(50)),
            Some(pricing),
        );
        assert_eq!(cost.cache_write.confidence, CostConfidence::Unknown);
        assert_eq!(cost.cache_write.usd, None);
        assert_eq!(cost.input.confidence, CostConfidence::Estimated);
        assert_eq!(cost.total_usd, None);
        assert_eq!(cost.total_confidence, CostConfidence::Unknown);
    }

    #[test]
    fn missing_tokens_are_unknown() {
        let cost = calculate(&usage(None, Some(0), Some(0), Some(0)), Some(priced()));
        assert_eq!(cost.input.confidence, CostConfidence::Unknown);
        assert_eq!(cost.total_confidence, CostConfidence::Unknown);
    }

    #[test]
    fn zero_tokens_are_a_zero_estimate_without_a_rate() {
        let cost = calculate(&usage(Some(0), Some(0), Some(0), Some(0)), None);
        assert_eq!(cost.input.usd, Some(0.0));
        assert_eq!(cost.total_usd, Some(0.0));
        assert_eq!(cost.total_confidence, CostConfidence::Estimated);
    }

    #[test]
    fn negative_tokens_or_rate_are_unknown() {
        let mut pricing = priced();
        pricing.output_per_mtok = -1.0;
        let cost = calculate(&usage(Some(-5), Some(10), Some(0), Some(0)), Some(pricing));
        assert_eq!(cost.input.confidence, CostConfidence::Unknown);
        assert_eq!(cost.output.confidence, CostConfidence::Unknown);
        assert_eq!(cost.total_confidence, CostConfidence::Unknown);
    }

    #[test]
    fn unknown_model_does_not_invent_a_rate() {
        let cost = calculate(
            &usage(Some(1_000), Some(1_000), Some(1_000), Some(1_000)),
            None,
        );
        assert_eq!(cost.input.confidence, CostConfidence::Unknown);
        assert_eq!(cost.output.confidence, CostConfidence::Unknown);
        assert_eq!(cost.cache_read.confidence, CostConfidence::Unknown);
        assert_eq!(cost.cache_write.confidence, CostConfidence::Unknown);
        assert_eq!(cost.total_usd, None);
    }
}
