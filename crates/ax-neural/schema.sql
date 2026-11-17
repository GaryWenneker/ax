CREATE TABLE IF NOT EXISTS neural_embedding (
  entity_kind TEXT NOT NULL,
  entity_id TEXT NOT NULL,
  content_hash TEXT NOT NULL,
  model_id TEXT NOT NULL,
  dim INTEGER NOT NULL,
  embedding BLOB NOT NULL,
  created_at INTEGER NOT NULL,
  PRIMARY KEY (entity_kind, entity_id, model_id)
);
CREATE TABLE IF NOT EXISTS neural_feedback (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  execution_id TEXT NOT NULL,
  task_category TEXT NOT NULL,
  context_ids TEXT NOT NULL,
  skill_ids TEXT NOT NULL,
  rule_ids TEXT NOT NULL,
  outcome TEXT NOT NULL,
  context_attribution TEXT NOT NULL,
  retry_count INTEGER NOT NULL,
  input_tokens INTEGER NOT NULL,
  output_tokens INTEGER NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS neural_model (
  model_id TEXT NOT NULL,
  version TEXT NOT NULL,
  model_type TEXT NOT NULL,
  dataset_version TEXT NOT NULL,
  checksum TEXT NOT NULL,
  platforms TEXT NOT NULL,
  metrics_json TEXT NOT NULL,
  promoted INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  PRIMARY KEY (model_id, version)
);
CREATE TABLE IF NOT EXISTS neural_graph_suggestion (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  source TEXT NOT NULL,
  target TEXT NOT NULL,
  score REAL NOT NULL,
  reason TEXT NOT NULL,
  provenance TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS neural_audit (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  project_id TEXT NOT NULL,
  action TEXT NOT NULL,
  detail TEXT NOT NULL,
  created_at INTEGER NOT NULL
);
