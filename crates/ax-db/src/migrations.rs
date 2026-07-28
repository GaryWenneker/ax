//! Schema migrations v2-v6.

use sqlx::SqlitePool;

use ax_utils::errors::{AxError, DatabaseError};

pub const CURRENT_SCHEMA_VERSION: i32 = 12;

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
    Migration {
        version: 7,
        description: "Policy engine tables for rules and skills",
        sql: "
            CREATE TABLE IF NOT EXISTS policy_rules (
                id TEXT PRIMARY KEY,
                level TEXT NOT NULL,
                always_apply INTEGER NOT NULL DEFAULT 0,
                globs TEXT NOT NULL DEFAULT '[]',
                triggers TEXT NOT NULL DEFAULT '[]',
                tags TEXT NOT NULL DEFAULT '[]',
                priority INTEGER NOT NULL DEFAULT 50,
                body TEXT NOT NULL,
                source_path TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS policy_skills (
                name TEXT PRIMARY KEY,
                description TEXT NOT NULL,
                triggers TEXT NOT NULL DEFAULT '[]',
                tags TEXT NOT NULL DEFAULT '[]',
                priority INTEGER NOT NULL DEFAULT 50,
                context_task TEXT,
                body TEXT NOT NULL,
                source_path TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_policy_rules_level ON policy_rules(level);
            CREATE INDEX IF NOT EXISTS idx_policy_skills_priority ON policy_skills(priority);
        ",
    },
    Migration {
        version: 8,
        description: "Business rules and ship state for Command Center",
        sql: "
            CREATE TABLE IF NOT EXISTS business_rules (
                id TEXT PRIMARY KEY,
                node_id TEXT NOT NULL,
                rule_text TEXT NOT NULL,
                severity TEXT NOT NULL DEFAULT 'warning',
                file_path TEXT NOT NULL,
                line INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                FOREIGN KEY (node_id) REFERENCES nodes(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_business_rules_node ON business_rules(node_id);
            CREATE INDEX IF NOT EXISTS idx_business_rules_file ON business_rules(file_path);
            CREATE TABLE IF NOT EXISTS ship_state (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
        ",
    },
    Migration {
        version: 9,
        description: "Memory vault: memories table + FTS5 index",
        sql: "
            CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL DEFAULT 'note',
                title TEXT NOT NULL,
                body TEXT NOT NULL,
                tags TEXT NOT NULL DEFAULT '[]',
                files TEXT NOT NULL DEFAULT '[]',
                confidence REAL NOT NULL DEFAULT 1.0,
                source TEXT NOT NULL DEFAULT 'manual',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_memories_kind ON memories(kind);
            CREATE INDEX IF NOT EXISTS idx_memories_updated ON memories(updated_at);
            CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                id,
                title,
                body,
                tags,
                content='memories',
                content_rowid='rowid'
            );
            CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
                INSERT INTO memories_fts(rowid, id, title, body, tags)
                VALUES (NEW.rowid, NEW.id, NEW.title, NEW.body, NEW.tags);
            END;
            CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
                INSERT INTO memories_fts(memories_fts, rowid, id, title, body, tags)
                VALUES ('delete', OLD.rowid, OLD.id, OLD.title, OLD.body, OLD.tags);
            END;
            CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
                INSERT INTO memories_fts(memories_fts, rowid, id, title, body, tags)
                VALUES ('delete', OLD.rowid, OLD.id, OLD.title, OLD.body, OLD.tags);
                INSERT INTO memories_fts(rowid, id, title, body, tags)
                VALUES (NEW.rowid, NEW.id, NEW.title, NEW.body, NEW.tags);
            END;
        ",
    },
    Migration {
        version: 10,
        description: "Memory embeddings for hybrid (FTS + vector) recall",
        sql: "ALTER TABLE memories ADD COLUMN embedding BLOB;",
    },
    Migration {
        version: 11,
        description: "Edge confidence taxonomy + community detection storage",
        sql: "
            ALTER TABLE edges ADD COLUMN confidence TEXT DEFAULT NULL;
            CREATE INDEX IF NOT EXISTS idx_edges_confidence ON edges(confidence);
            CREATE TABLE IF NOT EXISTS node_communities (
                node_id TEXT PRIMARY KEY,
                community_id INTEGER NOT NULL,
                community_label TEXT,
                computed_at INTEGER NOT NULL,
                FOREIGN KEY (node_id) REFERENCES nodes(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_node_communities_community ON node_communities(community_id);
        ",
    },
    Migration {
        version: 12,
        description: "Policy rule/skill enable + review status for pack sync",
        sql: "
            ALTER TABLE policy_rules ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1;
            ALTER TABLE policy_rules ADD COLUMN status TEXT NOT NULL DEFAULT 'approved';
            ALTER TABLE policy_skills ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1;
            ALTER TABLE policy_skills ADD COLUMN status TEXT NOT NULL DEFAULT 'approved';
            CREATE INDEX IF NOT EXISTS idx_policy_rules_status ON policy_rules(status);
            CREATE INDEX IF NOT EXISTS idx_policy_skills_status ON policy_skills(status);
        ",
    },
    Migration {
        version: 13,
        description: "Policy hierarchy scope (company/workspace/project/private)",
        sql: "
            ALTER TABLE policy_rules ADD COLUMN scope TEXT NOT NULL DEFAULT 'project';
            ALTER TABLE policy_skills ADD COLUMN scope TEXT NOT NULL DEFAULT 'project';
            CREATE INDEX IF NOT EXISTS idx_policy_rules_scope ON policy_rules(scope);
            CREATE INDEX IF NOT EXISTS idx_policy_skills_scope ON policy_skills(scope);
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
