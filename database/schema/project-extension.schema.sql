-- ============================================
-- PROJECT DATABASE EXTENSION SCHEMA
-- Adds global sync tracking columns to existing projects
-- Run this on each project database individually
-- ============================================

-- Add sync tracking columns to nodes table
ALTER TABLE nodes ADD COLUMN IF NOT EXISTS last_global_sync TIMESTAMP;

-- Add sync tracking columns to documents table  
ALTER TABLE documents ADD COLUMN IF NOT EXISTS last_global_sync TIMESTAMP;

-- Add project metadata table for global sync status
CREATE TABLE IF NOT EXISTS project_metadata (
    key TEXT PRIMARY KEY,
    value TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Insert default sync status
INSERT OR IGNORE INTO project_metadata (key, value) VALUES ('global_sync_enabled', 'true');
INSERT OR IGNORE INTO project_metadata (key, value) VALUES ('last_global_sync', NULL);
