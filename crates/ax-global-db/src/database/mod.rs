//! Apply the global.db schema (embedded SQL, no checkout path).

use anyhow::{Context, Result};
use sqlx::SqlitePool;

use ax_db::schema::split_statements;

const SCHEMA_SQL: &str = include_str!("../../../../database/schema/global.db.schema.sql");

pub async fn initialize_schema(pool: &SqlitePool) -> Result<()> {
    for stmt in split_statements(SCHEMA_SQL) {
        sqlx::query(&stmt).execute(pool).await.with_context(|| {
            format!(
                "schema statement failed: {}",
                stmt.chars().take(80).collect::<String>()
            )
        })?;
    }
    // One-time backfill for existing single-role installs; never re-add a
    // deliberately removed role when init runs again.
    sqlx::query(
        "INSERT INTO member_roles(member_id, role)
         SELECT id, role FROM team_members AS m
         WHERE NOT EXISTS (SELECT 1 FROM member_roles AS r WHERE r.member_id = m.id)",
    )
    .execute(pool)
    .await?;
    for table in ["global_policy_rules", "global_policy_skills"] {
        add_level_column(pool, table).await?;
    }
    add_column_if_missing(pool, "team_members", "avatar_image", "TEXT").await?;
    Ok(())
}

/// Tables created before the `level` column existed: every row they hold is global.
async fn add_level_column(pool: &SqlitePool, table: &str) -> Result<()> {
    add_column_if_missing(pool, table, "level", "TEXT NOT NULL DEFAULT 'global'").await
}

async fn add_column_if_missing(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<()> {
    let has: Option<(String,)> = sqlx::query_as(&format!(
        "SELECT name FROM pragma_table_info('{table}') WHERE name = '{column}'"
    ))
    .fetch_optional(pool)
    .await?;
    if has.is_none() {
        sqlx::query(&format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"))
            .execute(pool)
            .await
            .with_context(|| format!("add {column} column to {table}"))?;
    }
    Ok(())
}
