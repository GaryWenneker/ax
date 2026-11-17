//! Rows a hook records for the session and turn it already named.

use sqlx::SqlitePool;

use ax_utils::errors::AxError;

pub struct HookTurn {
    pub project_id: String,
    pub session_id: String,
    pub turn_id: String,
    pub prompt: String,
}

pub async fn record_hook_turn(pool: &SqlitePool, turn: &HookTurn) -> Result<(), AxError> {
    let session_id = turn.session_id.trim();
    let turn_id = turn.turn_id.trim();
    if session_id.is_empty() || turn_id.is_empty() {
        return Ok(());
    }
    let prompt: String = turn.prompt.chars().take(PROMPT_CAP).collect();
    let now = now_ms();
    let mut tx = pool.begin().await.map_err(db_err)?;
    sqlx::query(
        "INSERT INTO agent_sessions (id, project_id, created_at, updated_at, cache_namespace)
         VALUES (?, ?, ?, ?, '')
         ON CONFLICT(id) DO UPDATE SET updated_at = excluded.updated_at",
    )
    .bind(session_id)
    .bind(&turn.project_id)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(db_err)?;

    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM agent_turns WHERE id = ?")
        .bind(turn_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;
    if exists.is_some() {
        tx.commit().await.map_err(db_err)?;
        return Ok(());
    }

    let sequence: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(sequence), 0) + 1 FROM agent_turns WHERE session_id = ?",
    )
    .bind(session_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(db_err)?;
    sqlx::query(
        "INSERT INTO agent_turns (id, session_id, sequence, started_at, prompt, files, commits)
         VALUES (?, ?, ?, ?, ?, '[]', '[]')",
    )
    .bind(turn_id)
    .bind(session_id)
    .bind(sequence)
    .bind(now)
    .bind(prompt)
    .execute(&mut *tx)
    .await
    .map_err(db_err)?;
    tx.commit().await.map_err(db_err)?;
    Ok(())
}

const PROMPT_CAP: usize = 8_000;

pub struct HookTurnEnd {
    pub turn_id: String,
    pub outcome: Option<String>,
    pub files: Vec<String>,
    pub commits: Vec<String>,
}

pub async fn finish_hook_turn(pool: &SqlitePool, end: &HookTurnEnd) -> Result<bool, AxError> {
    let turn_id = end.turn_id.trim();
    if turn_id.is_empty() {
        return Ok(false);
    }
    let outcome = end
        .outcome
        .as_ref()
        .map(|text| text.chars().take(OUTCOME_CAP).collect::<String>());
    let files = serde_json::to_string(&end.files).unwrap_or_else(|_| "[]".to_string());
    let commits = serde_json::to_string(&end.commits).unwrap_or_else(|_| "[]".to_string());
    let result = sqlx::query(
        "UPDATE agent_turns
         SET ended_at = ?,
             outcome = COALESCE(?, outcome),
             files = ?,
             commits = ?
         WHERE id = ?",
    )
    .bind(now_ms())
    .bind(outcome)
    .bind(files)
    .bind(commits)
    .bind(turn_id)
    .execute(pool)
    .await
    .map_err(db_err)?;
    Ok(result.rows_affected() == 1)
}

const OUTCOME_CAP: usize = 20_000;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

fn db_err(err: sqlx::Error) -> AxError {
    AxError::Database(ax_utils::errors::DatabaseError::new(err.to_string()))
}
