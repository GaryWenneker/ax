-- ============================================
-- GLOBAL MULTI-TENANT DATABASE SCHEMA
-- Location: ~/.ax/global.db (override: AX_GLOBAL_DB)
-- ============================================

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS workspaces (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    path TEXT NOT NULL UNIQUE,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    last_sync TIMESTAMP
);

CREATE TABLE IF NOT EXISTS projects (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    path TEXT NOT NULL UNIQUE,
    workspace_id INTEGER,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    last_sync TIMESTAMP,
    node_count INTEGER DEFAULT 0,
    doc_count INTEGER DEFAULT 0,
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id)
);

CREATE TABLE IF NOT EXISTS sync_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_project TEXT NOT NULL,
    target_type TEXT NOT NULL CHECK(target_type IN ('global', 'project')),
    record_count INTEGER NOT NULL,
    node_types TEXT,
    duration_ms INTEGER,
    errors TEXT,
    synced_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS global_nodes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id INTEGER NOT NULL,
    source_id TEXT NOT NULL,
    node_type TEXT NOT NULL,
    name TEXT NOT NULL,
    file_path TEXT,
    content_hash TEXT NOT NULL,
    source_db_version INTEGER,
    synced_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(project_id, source_id),
    FOREIGN KEY (project_id) REFERENCES projects(id)
);

CREATE TABLE IF NOT EXISTS global_documents (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id INTEGER NOT NULL,
    file_path TEXT NOT NULL,
    doc_type TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    word_count INTEGER DEFAULT 0,
    link_count INTEGER DEFAULT 0,
    synced_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(project_id, file_path),
    FOREIGN KEY (project_id) REFERENCES projects(id)
);

CREATE TABLE IF NOT EXISTS global_edges (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id INTEGER NOT NULL,
    source_node TEXT NOT NULL,
    target_node TEXT NOT NULL,
    edge_type TEXT NOT NULL,
    content_hash_source TEXT,
    content_hash_target TEXT,
    synced_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (project_id) REFERENCES projects(id)
);

CREATE TABLE IF NOT EXISTS cross_project_refs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_project TEXT NOT NULL,
    target_project TEXT NOT NULL,
    source_node_name TEXT NOT NULL,
    target_node_name TEXT NOT NULL,
    ref_type TEXT NOT NULL CHECK(ref_type IN ('import', 'reference', 'duplicate')),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(source_project, target_project, source_node_name, target_node_name)
);

CREATE TABLE IF NOT EXISTS shared_knowledge (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    content_hash TEXT NOT NULL UNIQUE,
    node_type TEXT NOT NULL,
    name TEXT NOT NULL,
    projects TEXT NOT NULL,
    first_seen_project TEXT,
    last_synced TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_global_nodes_project_type ON global_nodes(project_id, node_type);
CREATE INDEX IF NOT EXISTS idx_global_nodes_hash ON global_nodes(content_hash);
CREATE INDEX IF NOT EXISTS idx_global_documents_project_path ON global_documents(project_id, file_path);
CREATE TABLE IF NOT EXISTS global_policy_rules (
    project_id INTEGER NOT NULL,
    item_id TEXT NOT NULL,
    payload TEXT NOT NULL,
    synced_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (project_id, item_id),
    FOREIGN KEY (project_id) REFERENCES projects(id)
);

CREATE TABLE IF NOT EXISTS global_policy_skills (
    project_id INTEGER NOT NULL,
    item_id TEXT NOT NULL,
    payload TEXT NOT NULL,
    synced_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (project_id, item_id),
    FOREIGN KEY (project_id) REFERENCES projects(id)
);

CREATE INDEX IF NOT EXISTS idx_sync_log_timestamp ON sync_log(synced_at DESC);
CREATE INDEX IF NOT EXISTS idx_cross_project_refs_source ON cross_project_refs(source_project, source_node_name);

CREATE VIEW IF NOT EXISTS v_global_nodes_with_project AS
SELECT
    g.id as global_id,
    g.project_id,
    p.name as project_name,
    p.path as project_path,
    g.node_type,
    g.name as node_name,
    g.file_path,
    g.content_hash,
    g.synced_at
FROM global_nodes g
JOIN projects p ON g.project_id = p.id;

CREATE VIEW IF NOT EXISTS v_shared_knowledge_summary AS
SELECT
    sk.content_hash,
    sk.node_type,
    sk.name as node_name,
    sk.projects as project_names
FROM shared_knowledge sk;
