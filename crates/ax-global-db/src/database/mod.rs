//! Apply the global.db schema (embedded SQL, no checkout path).

use anyhow::{Context, Result};
use sqlx::SqlitePool;

use ax_db::schema::split_statements;

const SCHEMA_SQL: &str = include_str!("../../../../database/schema/global.db.schema.sql");

pub async fn initialize_schema(pool: &SqlitePool) -> Result<()> {
    for stmt in split_statements(SCHEMA_SQL) {
        sqlx::query(&stmt)
            .execute(pool)
            .await
            .with_context(|| format!("schema statement failed: {}", stmt.chars().take(80).collect::<String>()))?;
    }
    Ok(())
}
