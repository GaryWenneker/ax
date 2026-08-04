//! Initial schema and FTS5 setup.

use sqlx::SqlitePool;

use ax_utils::errors::{AxError, DatabaseError};

fn strip_line_comments(sql: &str) -> String {
    sql.lines()
        .filter(|line| !line.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn count_begin_end(sql: &str) -> (i32, i32) {
    let mut begins = 0i32;
    let mut ends = 0i32;
    for word in sql.split(|c: char| !c.is_alphanumeric() && c != '_') {
        match word.to_uppercase().as_str() {
            "BEGIN" => begins += 1,
            "END" => ends += 1,
            _ => {}
        }
    }
    (begins, ends)
}

pub fn split_statements(sql: &str) -> Vec<String> {
    let cleaned = strip_line_comments(sql);
    let mut statements = Vec::new();
    let mut current = String::new();

    for part in cleaned.split(';') {
        if current.is_empty() {
            current = part.to_string();
        } else {
            current.push(';');
            current.push_str(part);
        }

        let (begins, ends) = count_begin_end(&current);
        let depth = begins - ends;

        if depth <= 0 {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                statements.push(trimmed.to_string());
            }
            current.clear();
        }
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        statements.push(trimmed.to_string());
    }

    statements
}

pub async fn apply_initial_schema(pool: &SqlitePool) -> Result<(), AxError> {
    let schema = include_str!("schema.sql");
    for trimmed in split_statements(schema) {
        let result = execute_schema_statement(pool, &trimmed).await;
        if let Err(e) = result {
            // schema.sql reflects the current end-state and runs *before*
            // migrations. On an older database, a column a later migration adds
            // does not exist yet, so an index/table in schema.sql that references
            // it fails here — the owning migration creates it moments later.
            // Tolerate those recoverable cases, mirroring `run_migrations`.
            let msg = e.to_string();
            if msg.contains("no such column")
                || msg.contains("duplicate column")
                || msg.contains("already exists")
            {
                continue;
            }
            return Err(AxError::Database(DatabaseError::new(format!(
                "schema: {e}: {trimmed}"
            ))));
        }
    }
    Ok(())
}

async fn execute_schema_statement(
    pool: &SqlitePool,
    sql: &str,
) -> Result<sqlx::sqlite::SqliteQueryResult, sqlx::Error> {
    crate::with_busy_retry(|| async { sqlx::query(sql).execute(pool).await }).await
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_statements_are_complete() {
        let stmts = split_statements(include_str!("schema.sql"));
        for s in &stmts {
            if s.contains("CREATE TRIGGER") {
                assert!(s.contains("END"), "incomplete trigger: {}", s);
            }
        }
        assert!(stmts.len() > 10);
    }

    /// Regression: schema.sql runs before migrations. On a pre-v11 database the
    /// `edges` table has no `confidence` column yet, so the schema's
    /// `idx_edges_confidence` (and other forward-looking statements) must be
    /// tolerated rather than aborting the whole open.
    #[tokio::test]
    async fn apply_initial_schema_tolerates_pre_migration_columns() {
        use sqlx::sqlite::SqlitePoolOptions;
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        // Simulate an old edges table without the confidence/line/col columns.
        sqlx::query(
            "CREATE TABLE edges (id INTEGER PRIMARY KEY, source TEXT, target TEXT, kind TEXT, provenance TEXT)",
        )
        .execute(&pool)
        .await
        .unwrap();

        apply_initial_schema(&pool)
            .await
            .expect("schema apply must tolerate columns a later migration will add");
    }
}