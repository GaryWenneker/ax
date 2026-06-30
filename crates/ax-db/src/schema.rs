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
        sqlx::query(&trimmed)
            .execute(pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(format!("schema: {e}: {trimmed}"))))?;
    }
    Ok(())
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
}