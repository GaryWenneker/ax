CREATE TABLE IF NOT EXISTS neural_rank_total (
  project_id TEXT PRIMARY KEY,
  preflights INTEGER NOT NULL,
  duplicate_blocks INTEGER NOT NULL,
  duplicate_tokens INTEGER NOT NULL,
  rank_micros INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
