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
    for table in ["global_policy_rules", "global_policy_skills"] {
        add_level_column(pool, table).await?;
    }
    Ok(())
}

/// Tables created before the `level` column existed: every row they hold is global.
async fn add_level_column(pool: &SqlitePool, table: &str) -> Result<()> {
    let has: Option<(String,)> =
        sqlx::query_as(&format!("SELECT name FROM pragma_table_info('{table}') WHERE name = 'level'"))
            .fetch_optional(pool)
            .await?;
    if has.is_none() {
        sqlx::query(&format!("ALTER TABLE {table} ADD COLUMN level TEXT NOT NULL DEFAULT 'global'"))
            .execute(pool)
            .await
            .with_context(|| format!("add level column to {table}"))?;
    }
    Ok(())
}
