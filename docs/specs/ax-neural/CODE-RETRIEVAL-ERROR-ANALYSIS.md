# Code retrieval error analysis

The 15 rows below are the doc-comment queries whose reciprocal rank changed when `AX_MEMORY_BENCH_IDENTIFIER_AFFIX` was set inside the measurement only. Memory ids were the stable `path:line` ids. Production MRR was 0.17747420956497137 and the affix pass was 0.182903707133853. Twelve queries gained reciprocal rank and three lost it. The flag stays off. The affix rule was not edited after this inspection. These rows are a diagnosis, not a validation set and not a source of thresholds.

Ranks are 1-based positions in the top 10 returned by `recall_for_prompt`. An empty rank means the documented function was outside that list.

| Direction | Production rank | Affix rank | Function | What moved it |
|---|---:|---:|---|---|
| Gain |  | 3 | `watch_and_sync` | `watch`, `watcher`, and `sync` share a prefix with the function name. `get_ready_files` also entered, through `Debounced`/`debounce_ms` and `files`. |
| Gain |  | 3 | `consume_sse_blocking` | `Blocking` and `consumer` share a prefix with the function name. The doc does not name the function. |
| Gain |  | 1 | `rotate_client_credential` | `rotation` and `credential` share a prefix with the function name. The doc does not name the function. It became the only hit. |
| Gain |  | 5 | `flatten_list_item` | `PolicyRuleDoc` and `PolicySkillDoc` share the prefix `Policy` with the parameter type `PolicyKind`. `policy_rows` entered on the same prefix. |
| Gain |  | 4 | `get_impact_subgraph` | The doc names `getImpactSubgraph`. Removing case and underscores makes that the same identifier as `get_impact_subgraph`. |
| Gain |  | 6 | `server_key` | The gold signature shares no affix with the query. `publish_for_test` entered on `publishers` and `project`. |
| Gain |  | 5 | `read_guard_ids` | `read` is a prefix of the function name. `decision` shares a suffix with `fallback_session`. |
| Gain |  | 3 | `policy_sync_enabled` | `policySync` is a prefix of the function name. `project` also matches `project_root`, and eight other policy functions entered with it. |
| Gain |  | 5 | `read_project_stack_detect` | `stackDetect` is a suffix of the function name. |
| Gain |  | 6 | `write_project_agents_dir` | The gold matches through `project`/`project_root`. `agentsDir` matches the different function `agents_dir_name`. |
| Gain |  | 2 | `sonar_ping_fast` | `Fast` and `SonarQube` share a prefix with the function name. |
| Gain |  | 7 | `parse_composer_model_config` | `modelConfig` is a suffix of the function name. `model` also matches `normalize_cursor_model`. |
| Loss | 6 |  | `wait_for_any_daemon` | The gold itself matches `Waits` and `daemon`, and the affix pass removed it from the top 10. The five functions above it stayed. |
| Loss | 2 | 3 | `ensure_policy_dirs` | `dedup_policy` and the `seed_*` functions entered above it through `policy` and `seeding`. |
| Loss | 7 |  | `remove_pending_origin` | `pending` matches the different function `pending_origin`. `item` matches `LinkItem` and `GlobalItem`. The gold left the top 10. |

## Causes

**Correct symbol identification.** One gain. `getImpactSubgraph` and `get_impact_subgraph` are the same identifier once case and underscores are removed. That is the documented name of the function, not a shared fragment.

**Identifier substring overlap.** Eleven gains. The query does not contain the function name. A prefix or suffix of at least 4 characters, on an identifier-shaped token, is enough. `rotation`/`rotate_client_credential`, `consumer`/`consume_sse_blocking`, `watch`/`watch_and_sync`, `read`/`read_guard_ids`, `policySync`/`policy_sync_enabled`, `stackDetect`/`read_project_stack_detect`, and `modelConfig`/`parse_composer_model_config` are this class. Some of those fragments are the words a reader would use. The matcher does not know which fragments are the symbol.

**Incorrect symbol association.** All three losses, and the extra hits on several gains. Shared pieces pull a different function: `Policy` pulls `policy_rows`, `project` pulls every `project_root` parameter, `publishers` pulls `publish_for_test`, `seeding` pulls `seed_global_policy`, `pending` pulls `pending_origin`, and `item` pulls `LinkItem`. `server_key` rose beside `publish_for_test` even though the gold signature itself shares no affix with the query. `write_project_agents_dir` rose through `project_root` while the documented name `agentsDir` points at `agents_dir_name`.

**Ranking tie behavior.** Not what moved these 15. The ids were stable, and the reciprocal of the documented function changed. The earlier random-id runs changed the size of the MRR delta across the whole 617; they are not the cause of these rank changes.

**Missing graph relationships.** The losses and the crowded gains are sibling functions, often in the same area (`policy_*`, `agents_dir`, `pending_origin` next to `remove_pending_origin`). A call, definition, or file edge would say which symbol the doc names. A shared prefix cannot.

**Missing semantic information.** Not measured on these rows. Several docs describe the function without naming it (`rotation`, `consumer`, `watch loop`). The affix pass scored them only where a fragment overlapped an identifier. That is not evidence that a semantic model would have ranked them, and it is not evidence that it would have avoided the three losses.

## What this does not justify

The affix flag stays disabled. Production ranking defaults stay as they are. Nothing here is a new threshold, a new synonym, or a change to `SEMANTIC_WEIGHT`, the coverage floor, or `ax_text_rank`. A later graph-aware ranker has to be scored on a frozen set that was not built from these 15 rows.
