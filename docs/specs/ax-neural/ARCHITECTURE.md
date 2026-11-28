# Ax Neural architecture

Status: recorded from the repository at the start of this work. No production behavior is described here that the later specs do not implement.

## Languages and workspace

The product is Rust. Command Center is TypeScript in `crates/ax-web/web-ui`. The public site is Astro. The Cargo workspace members live in the root `Cargo.toml`. Neural code is one new member, `crates/ax-neural`. It is not a second service process.

## Seams that stay authoritative

| Concern | Where it lives | Neural rule |
|---|---|---|
| CLI | `crates/ax-cli` | Add `ax neural`. Do not replace existing commands. |
| MCP | `crates/ax-mcp/src/tools.rs` | Add `ax_neural_*` tools. Existing tools stay. |
| Memory embeddings | `crates/ax-memory/src/embed.rs` | `hash-v1`, 256 dimensions, backend `hash`. `reembed_all_foreign` still rewrites any other model id on `memories.embedding`. |
| Vector index | `crates/ax-db/src/vec_ext.rs`, table `memory_vec` | sqlite-vec stays the hash index. Neural vectors use a different table. |
| Fusion | `crates/ax-memory/src/fusion.rs` | Weights stay FTS 0.45, vector 0.25, ACT-R 0.15, graph 0.10, usage 0.05. |
| Context budget | `crates/ax-mcp/src/context_budget.rs` | `HardRequired` blocks stay. On every preflight Neural reorders only the blocks the budget may drop, and drops repeated text. Its reasons go in `contextBudget.neural`, which the budget counts. |
| Source retrieval | `crates/ax-context/src/retrieval.rs` | Stays model-free. Neural is not called from that module. |
| Graph | `crates/ax-graph`, `petgraph_analysis.rs` | Edges stay authoritative. Suggestions are separate rows with provenance `predicted`. |
| Skills and rules | `crates/ax-policy` | Bodies stay in policy files. Neural stores ids, not copies. |
| Reasoning offload | `crates/ax-reasoning` | Optional LLM offload. Not a dependency of Neural. |
| Replication | `crates/ax-db/src/replication.rs` `Dataset` | Graph, history, and logs stay as they are. Neural share packets are a separate artifact and are off unless `share_learning_data` is true. |
| Config | project `ax.json` | A `neural` object, merged the same way as `context.budgetTokens`. A missing object keeps the defaults: ranking on; prediction, graph, feedback collection, and distribution off. |
| Benchmarks | `benchmarks/memory/coding.json`, `crates/ax-memory/src/bench.rs` | The fixture is built into the binary. The bake-off scores it with `score_coding` in a temporary database. It does not open `.ax/ax.db`. |

## Dependency choice

Candle, Burn, ONNX Runtime, and Tract are not workspace dependencies. The default `ax` binary does not link them. The Cargo feature `neural-runtime` exists and is empty so a later runtime can sit behind it. CPU hash-v1 remains the embedder in every build.

The retrieval experiments (`query_prior`, `code_aware`, `code_discovery`, `recovery`, `learn_validate`) have no caller in dispatch, the CLI, MCP or the web server. They compile only for tests and with the Cargo feature `neural-lab`, so `cargo test -p ax-neural` still runs their tests and release builds leave them out.

## Data added

Schema version 45 adds tables. They reference entities by id and content hash. They do not copy memory bodies, rule bodies, or source files.

### `SCHEMA_SQL` application

The migration SQL lives in the crate as executable DDL only (no comments in the SQL files). [`crates/ax-neural/schema.sql`](../../../crates/ax-neural/schema.sql) is migration **45**; [`crates/ax-neural/schema_rank.sql`](../../../crates/ax-neural/schema_rank.sql) is migration **46**.

| Step | Location | What runs |
|---|---|---|
| Project DB open | `crates/ax-db/src/migrations.rs` | Migration 45 executes `schema.sql` via `include_str!` and `schema::split_statements` |
| Same DDL in tests | `crates/ax-neural/src/lib.rs` `SCHEMA_SQL` | `include_str!("../schema.sql")` so harnesses read the exact migration body without a cargo cycle (`ax-db` include-str's the file; `ax-neural` must not depend on migration structs) |

See migration tests `crates/ax-db/tests/migration_v45.rs` and `migration_v46.rs` for the expected table set after open.

### Schema version 45 tables

- [`neural_embedding`](../../../crates/ax-neural/schema.sql) — optional vectors for a model id other than `hash-v1`
- [`neural_feedback`](../../../crates/ax-neural/schema.sql) — append-only execution feedback
- [`neural_model`](../../../crates/ax-neural/schema.sql) — registry rows and an explicit promoted flag
- [`neural_graph_suggestion`](../../../crates/ax-neural/schema.sql) — predicted links, not graph edges
- [`neural_audit`](../../../crates/ax-neural/schema.sql) — promotion and rollback, and each stored evaluation

### Schema version 46 table

- [`neural_rank_total`](../../../crates/ax-neural/schema_rank.sql) — per project, the number of ranked preflights, the duplicate blocks and tokens dropped, and the time spent ranking. It holds counts only. It is not in `project_metadata`, because that table is replicated.

## Hot path

Neural ranking runs on every preflight. When `prediction.enabled` is true, a `neural:predicted` block is added before ranking. It is optional, labeled predicted, and the budget may drop it. Optional and high-value blocks are then reordered, and repeated text is dropped. Hard-required blocks stay first. Preflight then calls `context_budget::finish_with_neural`, which is `finish_with_memory_reserve` with `contextBudget.neural` inside the counted reply. The rank totals are written after the reply is built. Preflight waits for that write for at most 250 ms, because `ax preflight` and `ax turn begin` end their process as soon as preflight returns and a background write would be lost. An error or timeout is logged and the reply is unchanged. `neural.enabled: false` does not turn the layer off.

## Predictions and evaluation

Predictions come from this project's stored feedback, not from a model. The feedback rows whose task category contains the task, or is contained in it, are the matches. Each skill, rule, and context id in them gets a confidence: the share of matching rows that name it. Ids under `prediction.minimum_confidence` (default 0.75, inclusive) are dropped, and every prediction is labeled `predicted`. A `memory:` context id is predicted as a memory. `ax neural predict` and the `neural:predicted` preflight block both use this, and both stay off until `prediction.enabled` is true.

`ax neural evaluate` (MCP `ax_neural_evaluate`) scores the predictor leave-one-out on the built-in coding fixture. Each of its 19 questions is predicted from the other 18, with the question's family as the task category and its relevant memories as context ids. The reply gives questions, predictions, true and false positives, precision, the false-positive rate, and coverage. Each evaluation is stored in `neural_audit`. Status shows the last precision as measured, and `unavailable` before the first run.

## Status honesty (Phase 1)

`ax neural status` exposes a `capabilities` object. Lexical retrieval and deterministic ranking must not be reported as neural embedding inference. See `CAPABILITIES.md` and `SPEC-PHASE1-STATUS.md`.

## Retrieval A/B/C benchmark (Phase 2)

`ax neural benchmark-abc` (MCP `ax_neural_benchmark_abc`, script `scripts/neural-abc.sh`) measures three **benchmark-only** ranking adapters on the same coding fixture:

| Arm | Ranking list |
|---|---|
| A | BM25 (`score.bm25`) — Ax retrieval without Neural ranking |
| B | Ax locked fusion + hash-v1 (`score.ax`) — original Ax ranking |
| C | Lexical-semantic-cpu when bake-off-admitted (else B) — not neural embedding inference |

Metrics: Precision@3/5, Recall@3/5, MRR, NDCG@5/10, latency; memory/tokens reported `not_measured` with reasons until portable. Locked vs held-out families are split; held-out must not retune `SEMANTIC_WEIGHT`. Production Neural ranking stays required. See `SPEC-PHASE2-BENCHMARK.md` and `abc-results.json`.

## Embedding-runtime bake-off (Phase 3)

`ax neural embed-bakeoff` (MCP `ax_neural_embed_bakeoff`, `scripts/neural-embed-bakeoff.sh` / `neural-embed-bakeoff-ort.sh`) measures cosine/lexical/ONNX retrieval on the coding fixture. With feature `neural-ort` and checksummed `~/.ax/models/all-MiniLM-L6-v2.onnx`, `onnx-runtime-cpu` is **measured** and gate-admitted (`neuralEmbeddingAdmitted=true`); Candle remains `not_built`. Production keeps `neuralEmbeddingInferenceActive=false`, `SEMANTIC_WEIGHT=0`, memory column `hash-v1`. See `SPEC-PHASE3-EMBED.md` and `embed-bakeoff-results.json`.

## Agent evaluation (Phase 6)

`ax neural agent-eval` / `scripts/neural-agent-eval.sh` measures fixture context-selection arms A–D (BM25, Ax fusion, lexical Neural ranking, prediction). Live identical-LLM task success / tokens / retries stay `not_measured` until controlled agent runs exist. See `SPEC-PHASE6-AGENT.md` and `agent-eval-results.json`.

## Predictive fields (Phase 4)

`Prediction` carries `explanation`, `evidence`, and `relationship` (`inferred` from feedback co-occurrence, `verified` when a project graph edge confirms a `graph` prediction). `predict_with_edges` is used by `ax neural predict` when a database is open. Still frequency priors — not neural inference. See `SPEC-PHASE4-PREDICT.md`.
