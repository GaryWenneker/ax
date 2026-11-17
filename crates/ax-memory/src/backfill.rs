//! Stamp legacy memories with session/turn ids.

use sqlx::SqlitePool;

use ax_utils::errors::{AxError, DatabaseError};

use crate::turns::conversation_from_body;

fn db_err(pool: &SqlitePool, e: impl std::fmt::Display) -> AxError {
    AxError::Database(DatabaseError::new(format!(
        "{} ({})",
        e,
        pool.connect_options().get_filename().display()
    )))
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BackfillReport {
    pub updated: usize,
    pub linked: usize,
    pub unlinked: usize,
}

/// Fill blank `session_id` / `turn_id` from the conversation line when present,
/// otherwise mark the row `Unlinked`.
pub async fn backfill_session_ids(pool: &SqlitePool) -> Result<BackfillReport, AxError> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT id, body FROM memories WHERE session_id = '' OR turn_id = ''")
            .fetch_all(pool)
            .await
            .map_err(|e| db_err(pool, e))?;

    let mut report = BackfillReport::default();
    for (id, body) in rows {
        let session = conversation_from_body(&body)
            .map(str::to_string)
            .unwrap_or_else(|| "Unlinked".into());
        let turn = if session == "Unlinked" {
            "Unlinked".to_string()
        } else {
            // Keep an existing turn id when we only filled the session.
            let existing: Option<String> =
                sqlx::query_scalar("SELECT turn_id FROM memories WHERE id = ?")
                    .bind(&id)
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| db_err(pool, e))?;
            let existing = existing.unwrap_or_default();
            if existing.trim().is_empty() {
                "Unlinked".into()
            } else {
                existing
            }
        };
        let result = sqlx::query(
            "UPDATE memories SET session_id = ?, turn_id = ?
             WHERE id = ? AND (session_id = '' OR turn_id = '')",
        )
        .bind(&session)
        .bind(&turn)
        .bind(&id)
        .execute(pool)
        .await
        .map_err(|e| db_err(pool, e))?;
        if result.rows_affected() == 0 {
            continue;
        }
        report.updated += 1;
        if session == "Unlinked" {
            report.unlinked += 1;
        } else {
            report.linked += 1;
        }
    }
    Ok(report)
}
