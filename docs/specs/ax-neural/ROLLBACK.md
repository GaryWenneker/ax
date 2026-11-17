# Rollback

Neural stays on. Schemas 45 and 46 do not need a downgrade.

1. Neural cannot be turned off. Removing the `neural` object, or setting `neural.enabled` to `false`, leaves ranking on. Unknown keys elsewhere in the file stay put.
2. Preflight calls `context_budget::finish_with_neural` after ranking. It wraps `finish_with_memory_reserve`, so hard-required blocks stay. `contextBudget.neural` says that kept optional blocks were ranked, that `omitted` blocks were over budget, and which blocks were dropped as duplicates, with their tokens. The budget counts it.
3. `memories.embedding` and `memory_vec` are unchanged. They stay `hash-v1`.
4. Schema 45 tables (`neural_embedding`, `neural_feedback`, `neural_model`, `neural_graph_suggestion`, `neural_audit`) and the schema 46 table `neural_rank_total` are additive. Leaving them in place does not change hash search. Dropping them is a manual SQL step. Without `neural_rank_total`, status reports token delta and rank latency as unavailable, and preflight logs a warning when it cannot add to the totals. Ranking does not read those tables.
5. To stop the predicted block, set `neural.prediction.enabled` to `false`. To stop stored graph suggestions, set `neural.graph.enabled` to `false`. Rows already stored stay until removed by SQL.
6. No model file is installed under `~/.ax/models` by this change, so there is nothing to delete there.
7. A promoted registry row, if one is ever created, is moved back with `ax neural rollback <model-id>` (MCP `ax_neural_rollback`). That selects the previously **promoted** version from `neural_audit` (not a never-promoted neighbour). It does not promote automatically.
