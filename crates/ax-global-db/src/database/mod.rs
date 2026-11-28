//! Apply the global.db schema (embedded SQL, no checkout path).

use anyhow::{Context, Result};
use sqlx::{Acquire, SqlitePool};

use ax_db::schema::split_statements;
use ax_db::with_busy_retry;

const SCHEMA_SQL: &str = include_str!("../../../../database/schema/global.db.schema.sql");

pub async fn initialize_schema(pool: &SqlitePool) -> Result<()> {
    // Every statement is idempotent, so one that lost a race for the write
    // lock with another process opening the same file is simply run again.
    for stmt in split_statements(SCHEMA_SQL) {
        with_busy_retry(|| sqlx::query(&stmt).execute(pool))
            .await
            .with_context(|| {
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
    widen_member_role_check(pool).await?;
    Ok(())
}

pub async fn widen_member_role_check(pool: &SqlitePool) -> Result<()> {
    let team_sql: Option<String> = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'team_members'",
    )
    .fetch_optional(pool)
    .await?;
    let Some(team_sql) = team_sql else {
        return Ok(());
    };
    let roles_sql: Option<String> = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'member_roles'",
    )
    .fetch_optional(pool)
    .await?;
    let team_ready = team_sql.contains("'approver'");
    let roles_ready = roles_sql
        .as_deref()
        .map(|sql| sql.contains("'approver'"))
        .unwrap_or(true);
    if team_ready && roles_ready {
        return Ok(());
    }
    let mut conn = pool.acquire().await?;
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *conn)
        .await?;
    let rebuilt = rebuild_member_role_tables(&mut conn).await;
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *conn)
        .await?;
    rebuilt
}

async fn rebuild_member_role_tables(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Sqlite>,
) -> Result<()> {
    let mut tx = conn.begin().await?;
    sqlx::query(
        "CREATE TABLE member_roles_copy (
            member_id TEXT NOT NULL,
            role TEXT NOT NULL,
            assigned_at TIMESTAMP NOT NULL,
            PRIMARY KEY (member_id, role)
        )",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO member_roles_copy (member_id, role, assigned_at)
         SELECT member_id, role, assigned_at FROM member_roles",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query("DROP TABLE member_roles")
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "CREATE TABLE team_members_next (
            id TEXT PRIMARY KEY,
            display_name TEXT NOT NULL,
            email TEXT,
            role TEXT NOT NULL CHECK(role IN ('owner', 'approver', 'admin', 'member', 'readonly')),
            status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active', 'disabled')),
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            last_active_at TIMESTAMP,
            avatar_image TEXT
        )",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO team_members_next
         (id, display_name, email, role, status, created_at, last_active_at, avatar_image)
         SELECT id, display_name, email, role, status, created_at, last_active_at, avatar_image
         FROM team_members",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query("DROP TABLE team_members")
        .execute(&mut *tx)
        .await?;
    sqlx::query("ALTER TABLE team_members_next RENAME TO team_members")
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "CREATE TABLE member_roles (
            member_id TEXT NOT NULL REFERENCES team_members(id) ON DELETE CASCADE,
            role TEXT NOT NULL CHECK(role IN ('owner', 'approver', 'admin', 'member', 'readonly')),
            assigned_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (member_id, role)
        )",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO member_roles (member_id, role, assigned_at)
         SELECT member_id, role, assigned_at FROM member_roles_copy",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query("DROP TABLE member_roles_copy")
        .execute(&mut *tx)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_member_roles_role ON member_roles(role)")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
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
        sqlx::query(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition}"
        ))
        .execute(pool)
        .await
        .with_context(|| format!("add {column} column to {table}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_old_role_check_gains_approver() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("global.db");
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&format!("sqlite://{}?mode=rwc", path.display()))
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE team_members (
                id TEXT PRIMARY KEY,
                display_name TEXT NOT NULL,
                email TEXT,
                role TEXT NOT NULL CHECK(role IN ('owner', 'admin', 'member', 'readonly')),
                status TEXT NOT NULL DEFAULT 'active',
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                last_active_at TIMESTAMP,
                avatar_image TEXT
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE member_roles (
                member_id TEXT NOT NULL,
                role TEXT NOT NULL CHECK(role IN ('owner', 'admin', 'member', 'readonly')),
                assigned_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (member_id, role)
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO team_members (id, display_name, role, status)
             VALUES ('owner', 'Owner', 'owner', 'active')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO member_roles (member_id, role) VALUES ('owner', 'owner')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE web_sessions (
                token_hash TEXT PRIMARY KEY,
                member_id TEXT NOT NULL REFERENCES team_members(id) ON DELETE CASCADE
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO web_sessions (token_hash, member_id) VALUES ('hash', 'owner')")
            .execute(&pool)
            .await
            .unwrap();
        widen_member_role_check(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO team_members (id, display_name, role, status)
             VALUES ('ada', 'Ada', 'approver', 'active')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO member_roles (member_id, role) VALUES ('ada', 'approver')")
            .execute(&pool)
            .await
            .unwrap();
        let owner: String = sqlx::query_scalar("SELECT role FROM team_members WHERE id = 'owner'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(owner, "owner");
        let session_member: String =
            sqlx::query_scalar("SELECT member_id FROM web_sessions WHERE token_hash = 'hash'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(session_member, "owner");
    }
}
