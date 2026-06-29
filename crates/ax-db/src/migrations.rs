//! Schema migrations v2-v6.

use sqlx::SqlitePool;

use ax_utils::errors::{AxError, DatabaseError};

pub const CURRENT_SCHEMA_VERSION: i32 = 6;

struct Migration {
    version: i32,
    description: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 2,
        description: "Add project metadata, provenance tracking, and unresolved ref context",
        sql: "
            CREATE TABLE IF NOT EXISTS project_metadata (
              key TEXT PRIMARY KEY,
              value TEXT NOT NULL,
              updated_at INTEGER NOT NULL
            );
            ALTER TABLE unresolved_refs ADD COLUMN file_path TEXT NOT NULL DEFAULT '';
            ALTER TABLE unresolved_refs ADD COLUMN language TEXT NOT NULL DEFAULT 'unknown';
            ALTER TABLE edges ADD COLUMN provenance TEXT DEFAULT NULL;
            CREATE INDEX IF NOT EXISTS idx_unresolved_file_path ON unresolved_refs(file_path);
            CREATE INDEX IF NOT EXISTS idx_edges_provenance ON edges(provenance);
        ",
    },
    Migration {
        version: 3,
        description: "Add lower(name) expression index",
        sql: "CREATE INDEX IF NOT EXISTS idx_nodes_lower_name ON nodes(lower(name));",
    },
    Migration {
        version: 4,
        description: "Drop redundant idx_edges_source / idx_edges_target",
        sql: "
            DROP INDEX IF EXISTS idx_edges_source;
            DROP INDEX IF EXISTS idx_edges_target;
        ",
    },
    Migration {
        version: 5,
        description: "Add nodes.return_type column",
        sql: "ALTER TABLE nodes ADD COLUMN return_type TEXT;",
    },
    Migration {
        version: 6,
        description: "Dedup duplicate edge rows and add UNIQUE identity index",
        sql: "
            DELETE FROM edges
            WHERE id NOT IN (
              SELECT MIN(id) FROM edges
              GROUP BY source, target, kind, IFNULL(line, -1), IFNULL(col, -1)
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_edges_identity
              ON edges(source, target, kind, IFNULL(line, -1), IFNULL(col, -1));
        ",
    },
];

pub async fn get_current_version(pool: &SqlitePool) -> Result<i32, AxError> {
    let result = sqlx::query_scalar::<_, Option<i32>>("SELECT MAX(version) FROM schema_versions")
        .fetch_one(pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(result.unwrap_or(0))
}

pub async fn run_migrations(pool: &SqlitePool, from_version: i32) -> Result<(), AxError> {
    for migration in MIGRATIONS {
        if migration.version <= from_version {
            continue;
        }
        for trimmed in crate::schema::split_statements(migration.sql) {
            let result = sqlx::query(&trimmed).execute(pool).await;
            if let Err(e) = result {
                let msg = e.to_string();
                if msg.contains("duplicate column") || msg.contains("already exists") {
                    continue;
                }
                return Err(AxError::Database(DatabaseError::new(format!(
                    "migration v{}: {e}",
                    migration.version
                ))));
            }
        }
        record_migration(pool, migration.version, migration.description).await?;
    }
    Ok(())
}

async fn record_migration(pool: &SqlitePool, version: i32, description: &str) -> Result<(), AxError> {
    let now = chrono_now_ms();
    sqlx::query("INSERT INTO schema_versions (version, applied_at, description) VALUES (?, ?, ?)")
        .bind(version)
        .bind(now)
        .bind(description)
        .execute(pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(())
}

fn chrono_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub async fn needs_migration(pool: &SqlitePool) -> Result<bool, AxError> {
    let current = get_current_version(pool).await?;
    Ok(current < CURRENT_SCHEMA_VERSION)
}
