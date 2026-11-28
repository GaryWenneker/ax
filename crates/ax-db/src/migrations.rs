//! Schema migrations v2-v6.

use sqlx::{Connection, SqlitePool};

use ax_utils::errors::{AxError, DatabaseError};

pub const CURRENT_SCHEMA_VERSION: i32 = 46;

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
    Migration {
        version: 14,
        description: "Memory vault enable flag (exclude disabled from recall/inject)",
        sql: "ALTER TABLE memories ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1;",
    },
    Migration {
        version: 15,
        description: "Hybrid policy storage: per-item storage, source, root_id, stub_path",
        sql: "
            ALTER TABLE policy_rules ADD COLUMN storage TEXT;
            ALTER TABLE policy_rules ADD COLUMN source TEXT;
            ALTER TABLE policy_rules ADD COLUMN root_id TEXT;
            ALTER TABLE policy_rules ADD COLUMN stub_path TEXT;
            ALTER TABLE policy_skills ADD COLUMN storage TEXT;
            ALTER TABLE policy_skills ADD COLUMN source TEXT;
            ALTER TABLE policy_skills ADD COLUMN root_id TEXT;
            ALTER TABLE policy_skills ADD COLUMN stub_path TEXT;
        ",
    },
    Migration {
        version: 16,
        description: "Skills always_apply — match even on empty prompts",
        sql: "ALTER TABLE policy_skills ADD COLUMN always_apply INTEGER NOT NULL DEFAULT 0;",
    },
    // Query-time snippets are served from here instead of the working tree, so a
    // graph read never touches the filesystem. Separate table on purpose: file
    // listings do `SELECT * FROM files`, and a body column there would pull every
    // file's text into memory on every listing.
    Migration {
        version: 17,
        description: "Source store: file_contents for graph-only snippet reads",
        sql: "
            CREATE TABLE IF NOT EXISTS file_contents (
              path TEXT PRIMARY KEY,
              content_hash TEXT NOT NULL,
              content TEXT NOT NULL,
              byte_len INTEGER NOT NULL,
              updated_at INTEGER NOT NULL
            );
        ",
    },
    Migration {
        version: 18,
        description: "Policy skill catalog group (Command Center grouping)",
        sql: "ALTER TABLE policy_skills ADD COLUMN skill_group TEXT;",
    },
    Migration {
        version: 19,
        description: "Policy rule catalog group (Command Center grouping)",
        sql: "ALTER TABLE policy_rules ADD COLUMN skill_group TEXT;",
    },
    Migration {
        version: 20,
        description: "Capped hash-on-change policy revisions",
        sql: "
            CREATE TABLE IF NOT EXISTS policy_revisions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                kind TEXT NOT NULL,
                item_id TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                body TEXT NOT NULL,
                source TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_policy_revisions_item
              ON policy_revisions(kind, item_id, created_at DESC);
        ",
    },
    Migration {
        version: 21,
        description: "WebDAV vault files and drafts (Obsidian)",
        sql: "
            CREATE TABLE IF NOT EXISTS dav_files (
                path TEXT PRIMARY KEY,
                is_dir INTEGER NOT NULL DEFAULT 0,
                content BLOB NOT NULL DEFAULT x'',
                draft_error TEXT,
                updated_at INTEGER NOT NULL
            );
        ",
    },
    Migration {
        version: 22,
        description: "Extra frontmatter properties on rules and skills",
        sql: "
            ALTER TABLE policy_rules ADD COLUMN properties TEXT NOT NULL DEFAULT '{}';
            ALTER TABLE policy_skills ADD COLUMN properties TEXT NOT NULL DEFAULT '{}';
        ",
    },
    Migration {
        version: 23,
        description: "Pi agent economics (derived metrics only)",
        sql: "
            CREATE TABLE IF NOT EXISTS agent_sessions (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                repository_root TEXT,
                active_branch TEXT,
                cache_namespace TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS agent_turns (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                sequence INTEGER NOT NULL,
                started_at INTEGER,
                ended_at INTEGER,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                estimated_cost REAL
            );
            CREATE TABLE IF NOT EXISTS agent_tool_calls (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                turn_id TEXT NOT NULL,
                tool_name TEXT NOT NULL,
                input_hash TEXT NOT NULL,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                duration_ms INTEGER NOT NULL DEFAULT 0,
                cache_hit INTEGER NOT NULL DEFAULT 0,
                repeated INTEGER NOT NULL DEFAULT 0,
                estimated_cost REAL
            );
            CREATE TABLE IF NOT EXISTS agent_model_calls (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                turn_id TEXT,
                provider TEXT,
                model TEXT,
                input_tokens INTEGER,
                output_tokens INTEGER,
                cached_input_tokens INTEGER,
                estimated_cost REAL,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS agent_optimizations (
                id TEXT PRIMARY KEY,
                tool_call_id TEXT NOT NULL,
                alternative_tool TEXT NOT NULL,
                current_tokens INTEGER NOT NULL,
                alternative_tokens INTEGER NOT NULL,
                reduction_percent REAL NOT NULL,
                confidence TEXT NOT NULL,
                reason TEXT NOT NULL,
                accepted INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS agent_context_cache (
                key TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                estimated_tokens INTEGER NOT NULL,
                source_hashes TEXT NOT NULL,
                git_revision TEXT,
                policy_version TEXT,
                skill_version TEXT,
                graph_version TEXT
            );
        ",
    },
    Migration {
        version: 24,
        description: "Memory tiers, named cues, embedding model, and graph episode state",
        sql: "
            ALTER TABLE memories ADD COLUMN tier TEXT NOT NULL DEFAULT 't2';
            ALTER TABLE memories ADD COLUMN embedding_model TEXT NOT NULL DEFAULT 'hash-v1';
            ALTER TABLE memories ADD COLUMN status TEXT NOT NULL DEFAULT 'active';
            CREATE INDEX IF NOT EXISTS idx_memories_tier ON memories(tier);
            CREATE TABLE IF NOT EXISTS memory_cues (
                cue TEXT NOT NULL,
                memory_id TEXT NOT NULL,
                weight REAL NOT NULL DEFAULT 1.0,
                source TEXT NOT NULL,
                PRIMARY KEY (cue, memory_id)
            );
            CREATE INDEX IF NOT EXISTS idx_memory_cues_cue ON memory_cues(cue);
            CREATE TABLE IF NOT EXISTS memory_links (
                from_id TEXT NOT NULL,
                to_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                PRIMARY KEY (from_id, to_id, kind)
            );
            CREATE TABLE IF NOT EXISTS graph_symbol_state (
                symbol_id TEXT PRIMARY KEY,
                fingerprint TEXT NOT NULL
            );
        ",
    },
    Migration {
        version: 25,
        description: "Turns left on the column default are recent, not long-term",
        sql: "UPDATE memories SET tier = 't1' WHERE kind = 'turn' AND tier = 't2';",
    },
    Migration {
        version: 26,
        description: "Git captures and folder docs belong in the archive",
        sql: "UPDATE memories SET tier = 't3' WHERE kind IN ('git', 'doc') AND tier = 't2';",
    },
    Migration {
        version: 27,
        description: "Rebuild memory search after the tier columns changed the content table",
        sql: "INSERT INTO memories_fts(memories_fts) VALUES('rebuild');",
    },
    Migration {
        version: 28,
        description: "Append-only history of rules, skills, and IDE files",
        sql: "
            CREATE TABLE IF NOT EXISTS policy_history (
                kind TEXT NOT NULL,
                item_id TEXT NOT NULL,
                action TEXT NOT NULL,
                catalog_version INTEGER NOT NULL,
                at INTEGER NOT NULL,
                PRIMARY KEY (kind, item_id, action, at)
            );
        ",
    },
    Migration {
        version: 29,
        description: "Rebuild the archive vector index; earlier rows could map to the wrong memory",
        sql: "
            DROP TABLE IF EXISTS memory_vec_map;
            DROP TABLE IF EXISTS memory_vec;
        ",
    },
    Migration {
        version: 30,
        description: "Drop automatic cues that are noise words or already shared by 32 memories",
        sql: "
            DELETE FROM memory_cues
            WHERE source != 'trigger'
              AND cue IN (
                'add','added','adding','any','array','bool','boolean','change','changed',
                'changing','char','class','const','create','created','delete','deleted',
                'deleting','enum','f32','f64','field','fix','fixed','float','fn','function',
                'i32','i64','impl','int','integer','interface','method','modify','modified',
                'modifying','module','number','object','parameter','refactor','refactored',
                'remove','removed','removing','rename','renamed','renaming','str','string',
                'struct','trait','type','u32','u64','update','updated','updating','usize',
                'variable','void'
              );
            DELETE FROM memory_cues
            WHERE source != 'trigger'
              AND cue IN (
                SELECT cue FROM (
                  SELECT cue FROM memory_cues
                  WHERE source != 'trigger'
                  GROUP BY cue
                  HAVING COUNT(DISTINCT memory_id) >= 32
                )
              );
        ",
    },
    Migration {
        version: 31,
        description: "Reject heuristic consolidation candidates and revoke similarity supersedes links",
        sql: "
            UPDATE memories
            SET status = 'rejected'
            WHERE status = 'candidate' AND COALESCE(source, '') = 'consolidation';
            UPDATE memory_links
            SET kind = 'revoked_supersedes'
            WHERE kind = 'supersedes';
        ",
    },
    Migration {
        version: 32,
        description: "Register running builds so an older build can skip background writers",
        sql: "
            CREATE TABLE IF NOT EXISTS ax_builds (
                role TEXT NOT NULL,
                pid INTEGER NOT NULL,
                version TEXT NOT NULL,
                mtime_ms INTEGER NOT NULL,
                len INTEGER NOT NULL,
                seen_at INTEGER NOT NULL,
                PRIMARY KEY (role, pid)
            );
        ",
    },
    Migration {
        version: 33,
        description: "Flag Blossom notes as foreign so injection skips them",
        sql: "
            UPDATE memories SET status = 'foreign'
            WHERE status = 'active'
              AND (
                lower(COALESCE(source, '')) IN ('blossom', 'foreign')
                OR lower(COALESCE(tags, '')) LIKE '%\"blossom\"%'
                OR lower(COALESCE(files, '')) LIKE '%/blossom/%'
                OR lower(COALESCE(files, '')) LIKE '%\\blossom\\%'
                OR lower(COALESCE(title, '')) LIKE '%blossom%'
                OR lower(COALESCE(body, '')) LIKE '%blossom%'
              );
        ",
    },
    Migration {
        version: 34,
        description: "Demote build-output code changes to noise",
        sql: r#"
            UPDATE memories
            SET status = 'noise', tier = 't3', enabled = 0
            WHERE COALESCE(status, 'active') = 'active'
              AND (
                files LIKE '%/target-dev/%' OR files LIKE '%"target-dev/%'
                OR files LIKE '%\target-dev\%' OR body LIKE '% in target-dev/%'
                OR body LIKE '% in target-dev\%'
                OR files LIKE '%/target/%' OR files LIKE '%"target/%'
                OR body LIKE '% in target/%' OR body LIKE '% in target\%'
                OR files LIKE '%/node_modules/%' OR files LIKE '%"node_modules/%'
                OR body LIKE '% in node_modules/%'
                OR files LIKE '%/dist/%' OR files LIKE '%"dist/%'
                OR body LIKE '% in dist/%' OR body LIKE '% in dist\%'
              );
        "#,
    },
    Migration {
        version: 35,
        description: "Prompt text on hook turns",
        sql: "
            ALTER TABLE agent_turns ADD COLUMN prompt TEXT NOT NULL DEFAULT '';
            ALTER TABLE agent_turns ADD COLUMN outcome TEXT;
            ALTER TABLE agent_turns ADD COLUMN files TEXT NOT NULL DEFAULT '[]';
            ALTER TABLE agent_turns ADD COLUMN commits TEXT NOT NULL DEFAULT '[]';
        ",
    },
    Migration {
        version: 36,
        description: "Session and turn on memories",
        sql: "
            ALTER TABLE memories ADD COLUMN session_id TEXT NOT NULL DEFAULT '';
            ALTER TABLE memories ADD COLUMN turn_id TEXT NOT NULL DEFAULT '';
        ",
    },
    Migration {
        version: 37,
        description: "Graph node and edge version history",
        sql: "
            CREATE TABLE IF NOT EXISTS graph_node_versions (
              node_key TEXT NOT NULL,
              version INTEGER NOT NULL,
              body_hash TEXT NOT NULL,
              signature TEXT NOT NULL DEFAULT '',
              kind TEXT NOT NULL DEFAULT '',
              file_path TEXT NOT NULL DEFAULT '',
              start_line INTEGER NOT NULL DEFAULT 0,
              end_line INTEGER NOT NULL DEFAULT 0,
              valid_from INTEGER NOT NULL,
              valid_to INTEGER,
              commit_id TEXT NOT NULL DEFAULT '',
              session_id TEXT NOT NULL DEFAULT '',
              turn_id TEXT NOT NULL DEFAULT '',
              PRIMARY KEY (node_key, version)
            );
            CREATE TABLE IF NOT EXISTS graph_edge_versions (
              source_key TEXT NOT NULL,
              target_key TEXT NOT NULL,
              kind TEXT NOT NULL,
              version INTEGER NOT NULL,
              valid_from INTEGER NOT NULL,
              valid_to INTEGER,
              commit_id TEXT NOT NULL DEFAULT '',
              session_id TEXT NOT NULL DEFAULT '',
              turn_id TEXT NOT NULL DEFAULT '',
              PRIMARY KEY (source_key, target_key, kind, version)
            );
            CREATE TABLE IF NOT EXISTS graph_body_blobs (
              body_hash TEXT PRIMARY KEY,
              body BLOB NOT NULL
            );
        ",
    },
    Migration {
        version: 38,
        description: "Memory purge tombstones",
        sql: "
            CREATE TABLE IF NOT EXISTS memory_tombstones (
              memory_id TEXT PRIMARY KEY,
              purged_at INTEGER NOT NULL
            );
        ",
    },
    Migration {
        version: 39,
        description: "Memory provenance and trust",
        sql: "
            ALTER TABLE memories ADD COLUMN origin TEXT NOT NULL DEFAULT '';
            ALTER TABLE memories ADD COLUMN author TEXT NOT NULL DEFAULT '';
            ALTER TABLE memories ADD COLUMN trust_level TEXT NOT NULL DEFAULT 'trusted';
            ALTER TABLE memories ADD COLUMN commit_id TEXT NOT NULL DEFAULT '';
            ALTER TABLE memories ADD COLUMN instruction_like INTEGER NOT NULL DEFAULT 0;
        ",
    },
    Migration {
        version: 40,
        description: "Normalized search_text on memories",
        sql: "
            ALTER TABLE memories ADD COLUMN search_text TEXT NOT NULL DEFAULT '';
        ",
    },
    Migration {
        version: 41,
        description: "Memory anchors to graph versions",
        sql: "
            CREATE TABLE IF NOT EXISTS memory_anchors (
              memory_id TEXT NOT NULL,
              node_key TEXT NOT NULL,
              version INTEGER NOT NULL DEFAULT 0,
              stale INTEGER NOT NULL DEFAULT 0,
              orphan INTEGER NOT NULL DEFAULT 0,
              PRIMARY KEY (memory_id, node_key, version)
            );
        ",
    },
    Migration {
        version: 42,
        description: "Memory access log",
        sql: "
            CREATE TABLE IF NOT EXISTS memory_access (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              memory_id TEXT NOT NULL,
              kind TEXT NOT NULL,
              accessed_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_memory_access_memory ON memory_access(memory_id, accessed_at);
        ",
    },
    Migration {
        version: 43,
        description: "Deterministic learning episodes",
        sql: "
            CREATE TABLE IF NOT EXISTS memory_episodes (
              id TEXT PRIMARY KEY,
              session_id TEXT NOT NULL DEFAULT '',
              turn_id TEXT NOT NULL DEFAULT '',
              goal TEXT NOT NULL DEFAULT '',
              outcome TEXT NOT NULL DEFAULT '',
              created_at INTEGER NOT NULL
            );
        ",
    },
    Migration {
        version: 44,
        description: "FTS5 includes search_text with BM25F column weights",
        sql: "
            DROP TRIGGER IF EXISTS memories_ai;
            DROP TRIGGER IF EXISTS memories_ad;
            DROP TRIGGER IF EXISTS memories_au;
            DROP TABLE IF EXISTS memories_fts;
            CREATE VIRTUAL TABLE memories_fts USING fts5(
                id,
                title,
                body,
                tags,
                search_text,
                content='memories',
                content_rowid='rowid'
            );
            CREATE TRIGGER memories_ai AFTER INSERT ON memories BEGIN
                INSERT INTO memories_fts(rowid, id, title, body, tags, search_text)
                VALUES (NEW.rowid, NEW.id, NEW.title, NEW.body, NEW.tags, NEW.search_text);
            END;
            CREATE TRIGGER memories_ad AFTER DELETE ON memories BEGIN
                INSERT INTO memories_fts(memories_fts, rowid, id, title, body, tags, search_text)
                VALUES ('delete', OLD.rowid, OLD.id, OLD.title, OLD.body, OLD.tags, OLD.search_text);
            END;
            CREATE TRIGGER memories_au AFTER UPDATE ON memories BEGIN
                INSERT INTO memories_fts(memories_fts, rowid, id, title, body, tags, search_text)
                VALUES ('delete', OLD.rowid, OLD.id, OLD.title, OLD.body, OLD.tags, OLD.search_text);
                INSERT INTO memories_fts(rowid, id, title, body, tags, search_text)
                VALUES (NEW.rowid, NEW.id, NEW.title, NEW.body, NEW.tags, NEW.search_text);
            END;
            INSERT INTO memories_fts(memories_fts) VALUES('rebuild');
        ",
    },
    Migration {
        version: 45,
        description: "Neural tables, separate from hash-v1 memory embeddings",
        sql: include_str!("../../ax-neural/schema.sql"),
    },
    Migration {
        version: 46,
        description: "Neural rank totals per project: preflights, duplicate tokens, ranking time",
        sql: include_str!("../../ax-neural/schema_rank.sql"),
    },
];

pub async fn get_current_version(pool: &SqlitePool) -> Result<i32, AxError> {
    let result = sqlx::query_scalar::<_, Option<i32>>("SELECT MAX(version) FROM schema_versions")
        .fetch_one(pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(result.unwrap_or(0))
}

/// Applies every migration newer than `from_version`. Each migration runs in
/// its own `BEGIN IMMEDIATE` transaction on one connection: the version is
/// checked again under the write lock (another process may have applied it
/// since `from_version` was read), and the statements and the version row
/// commit together, so a crash never leaves a migration half applied.
pub async fn run_migrations(pool: &SqlitePool, from_version: i32) -> Result<(), AxError> {
    run_migration_list(pool, from_version, MIGRATIONS).await
}

async fn run_migration_list(
    pool: &SqlitePool,
    from_version: i32,
    migrations: &[Migration],
) -> Result<(), AxError> {
    let db_err = |e: sqlx::Error| AxError::Database(DatabaseError::new(e.to_string()));
    let pending: Vec<&Migration> = migrations
        .iter()
        .filter(|m| m.version > from_version)
        .collect();
    if pending.is_empty() {
        return Ok(());
    }
    let mut conn = pool.acquire().await.map_err(db_err)?;
    for migration in pending {
        // A tracked transaction: on an error, a failed COMMIT, or a dropped
        // future it rolls back before the connection goes back to the pool,
        // so no later user inherits an open write transaction.
        let mut tx = conn.begin_with("BEGIN IMMEDIATE").await.map_err(db_err)?;
        apply_migration(&mut tx, migration).await?;
        tx.commit().await.map_err(db_err)?;
    }
    Ok(())
}

/// Runs one migration inside the caller's transaction, unless it is already
/// recorded.
async fn apply_migration(
    conn: &mut sqlx::SqliteConnection,
    migration: &Migration,
) -> Result<(), AxError> {
    let applied: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM schema_versions WHERE version = ?")
        .bind(migration.version)
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    if applied > 0 {
        return Ok(());
    }
    for trimmed in crate::schema::split_statements(migration.sql) {
        // Safe: statements come from the compiled-in migration list.
        let result = sqlx::query(sqlx::AssertSqlSafe(trimmed.as_str()))
            .execute(&mut *conn)
            .await;
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
    sqlx::query(
        "INSERT OR IGNORE INTO schema_versions (version, applied_at, description) VALUES (?, ?, ?)",
    )
    .bind(migration.version)
    .bind(chrono_now_ms())
    .bind(migration.description)
    .execute(&mut *conn)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_failing_migration_leaves_nothing_half_applied() {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::Database::open(&dir.path().join("ax.db"))
            .await
            .unwrap();
        let pool = db.pool();
        let from = get_current_version(pool).await.unwrap();
        let broken = [Migration {
            version: from + 1,
            description: "test: second statement fails",
            sql: "CREATE TABLE half_applied (x INTEGER);
                  INSERT INTO no_such_table VALUES (1);",
        }];
        assert!(run_migration_list(pool, from, &broken).await.is_err());
        let table: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE name = 'half_applied'")
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(table, 0, "the first statement was rolled back");
        assert_eq!(get_current_version(pool).await.unwrap(), from);
        db.close().await;
    }

    /// A one-connection pool, so the next `acquire` gets the very connection
    /// the migration used.
    async fn single_connection_pool(path: &std::path::Path) -> SqlitePool {
        let opts = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE schema_versions (version INTEGER PRIMARY KEY, applied_at INTEGER, description TEXT)",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    /// True when the pooled connection is outside any transaction: a nested
    /// `BEGIN` fails with "cannot start a transaction within a transaction".
    async fn pooled_connection_is_idle(pool: &SqlitePool) -> bool {
        let mut conn = pool.acquire().await.unwrap();
        let ok = sqlx::query("BEGIN").execute(&mut *conn).await.is_ok();
        if ok {
            sqlx::query("ROLLBACK").execute(&mut *conn).await.unwrap();
        }
        ok
    }

    #[tokio::test]
    async fn a_failing_migration_returns_its_connection_outside_a_transaction() {
        let dir = tempfile::tempdir().unwrap();
        let pool = single_connection_pool(&dir.path().join("m.db")).await;
        let broken = [Migration {
            version: 1,
            description: "test: fails",
            sql: "INSERT INTO no_such_table VALUES (1);",
        }];
        assert!(run_migration_list(&pool, 0, &broken).await.is_err());
        assert!(pooled_connection_is_idle(&pool).await);
        pool.close().await;
    }

    #[tokio::test]
    async fn a_cancelled_migration_returns_its_connection_outside_a_transaction() {
        let dir = tempfile::tempdir().unwrap();
        let pool = single_connection_pool(&dir.path().join("m.db")).await;
        // Seconds of work, so the future is dropped while the migration's
        // transaction is open.
        let slow = [Migration {
            version: 1,
            description: "test: slow",
            sql: "CREATE TABLE slow AS WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 30000000) SELECT count(*) AS n FROM c;",
        }];
        let cancelled = tokio::time::timeout(
            std::time::Duration::from_millis(200),
            run_migration_list(&pool, 0, &slow),
        )
        .await;
        assert!(cancelled.is_err(), "the migration should still be running");
        assert!(pooled_connection_is_idle(&pool).await);
        let recorded: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM schema_versions")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(recorded, 0, "the cancelled migration was rolled back");
        pool.close().await;
    }
}
