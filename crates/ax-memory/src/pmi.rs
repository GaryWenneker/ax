//! Pointwise mutual information term associations mined from the vault (Phase 6).

use std::collections::HashMap;

use sqlx::SqlitePool;

use ax_utils::errors::{AxError, DatabaseError};

fn db_err(pool: &SqlitePool, e: impl std::fmt::Display) -> AxError {
    AxError::Database(DatabaseError::new(format!(
        "{} ({})",
        e,
        pool.connect_options().get_filename().display()
    )))
}

fn tokens(text: &str) -> Vec<String> {
    let mut out: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() >= 3)
        .map(str::to_lowercase)
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Expand query tokens with high-PMI co-occurring terms from memory titles/bodies.
pub async fn expand_with_pmi(
    pool: &SqlitePool,
    query: &str,
    limit: usize,
) -> Result<Vec<String>, AxError> {
    let seeds = tokens(query);
    if seeds.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT title, body FROM memories WHERE enabled = 1 LIMIT 2000")
            .fetch_all(pool)
            .await
            .map_err(|e| db_err(pool, e))?;
    let mut df: HashMap<String, f64> = HashMap::new();
    let mut pair: HashMap<(String, String), f64> = HashMap::new();
    let mut docs = 0.0;
    for (title, body) in rows {
        let terms = tokens(&format!("{title} {body}"));
        if terms.is_empty() {
            continue;
        }
        docs += 1.0;
        for t in &terms {
            *df.entry(t.clone()).or_default() += 1.0;
        }
        for seed in &seeds {
            if !terms.iter().any(|t| t == seed) {
                continue;
            }
            for t in &terms {
                if t == seed {
                    continue;
                }
                *pair.entry((seed.clone(), t.clone())).or_default() += 1.0;
            }
        }
    }
    if docs <= 0.0 {
        return Ok(Vec::new());
    }
    let mut scored: Vec<(String, f64)> = Vec::new();
    for ((seed, term), joint) in pair {
        let p_xy = joint / docs;
        let p_x = df.get(&seed).copied().unwrap_or(0.0) / docs;
        let p_y = df.get(&term).copied().unwrap_or(0.0) / docs;
        if p_x <= 0.0 || p_y <= 0.0 || p_xy <= 0.0 {
            continue;
        }
        let score = (p_xy / (p_x * p_y)).ln();
        if score > 0.0 {
            scored.push((term, score));
        }
    }
    scored.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    scored.dedup_by(|a, b| a.0 == b.0);
    Ok(scored.into_iter().take(limit).map(|(t, _)| t).collect())
}

#[cfg(test)]
mod tests {
    use super::tokens;

    #[test]
    fn tokens_dedup_and_lower() {
        let t = tokens("Fix FIX parseHttp");
        assert!(t.contains(&"fix".to_string()));
        assert!(t.contains(&"parsehttp".to_string()));
        assert_eq!(t.iter().filter(|x| *x == "fix").count(), 1);
    }
}
