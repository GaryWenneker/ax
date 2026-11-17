//! ACT-R base-level activation for memory ranking (Phase 6).

/// Base-level activation from past access times (ms since epoch).
/// `B = ln(sum((now - t_i)^(-d)))` with decay `d` (default 0.5).
pub fn base_level_activation(now_ms: i64, access_times_ms: &[i64], decay: f64) -> f64 {
    if access_times_ms.is_empty() || decay <= 0.0 {
        return f64::NEG_INFINITY;
    }
    let mut sum = 0.0;
    for &t in access_times_ms {
        let age_ms = (now_ms - t).max(1) as f64;
        let age_s = age_ms / 1000.0;
        sum += age_s.powf(-decay);
    }
    if sum <= 0.0 {
        return f64::NEG_INFINITY;
    }
    sum.ln()
}

#[cfg(test)]
mod tests {
    use super::base_level_activation;

    #[test]
    fn more_recent_access_raises_activation() {
        let now = 1_000_000_i64;
        let old = base_level_activation(now, &[now - 86_400_000], 0.5);
        let recent = base_level_activation(now, &[now - 1_000], 0.5);
        assert!(recent > old, "recent={recent} old={old}");
        assert_eq!(base_level_activation(now, &[], 0.5), f64::NEG_INFINITY);
    }
}
