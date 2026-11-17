# SPEC — Phase 2: Complete the retrieval benchmark

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/SPEC-PHASE2-BENCHMARK.md`  
**Depends on:** Phase 1 complete (`capabilities` honesty)  
**Tier:** 2  
**Spec approval:** goal-continuation autonomous (`spec approval: not obtained (autonomous run)` — human reviews this artifact)

## Arms (identical fixture / queries / limits / scoring)

| Arm | Adapter | Production impact |
|---|---|---|
| A | BM25 rankings from `ax_memory::bench::score_coding` (no Neural ranking) | None — benchmark-only |
| B | Ax locked fusion + hash-v1 (`score.ax`) | None — reads existing pipeline |
| C | Lexical-semantic-cpu when bake-off-admitted; else falls back to B | None — does not change `SEMANTIC_WEIGHT` or production ranking |

Fixture: built-in `benchmarks/memory/coding.json`. Locked families: `named_trigger`, `paraphrase`, `abstention`, `delivery_budget`. All other families are **held-out** (reported separately; must not retune `SEMANTIC_WEIGHT`).

## Metrics

Per arm, for locked and held-out:

- Precision@3, Precision@5
- Recall@3, Recall@5
- MRR
- NDCG@5, NDCG@10
- Query latency (`scoreCodingMillis`, lexical rank micros)
- Memory: `not_measured` with reason until a portable RSS reader exists
- Tokens: `not_measured` (no LLM arm in this retrieval benchmark)

## Scenarios

1. **Given** the coding fixture, **when** `run_abc` / `ax neural benchmark-abc` runs, **then** the report has three arms with locked + heldOut metric objects and `kind: measured`.
2. **Given** production Neural ranking is required, **when** the ABC adapters run, **then** `productionRankingUnchanged` is true and `NeuralConfig::required().ranking_active()` remains true.
3. **Given** `SEMANTIC_WEIGHT` is 0, **when** ABC runs, **then** the report echoes `semanticWeight: 0.0`.
4. **Given** arm C uses lexical ranking, **when** the report is emitted, **then** `claimsNeuralInference` is false.
5. **Given** abstention questions have empty relevant sets, **when** P/R/MRR are computed, **then** those samples are omitted (not fabricated as 0/1).

## Deliverables

- `crates/ax-neural/src/benchmark_abc.rs`
- `ax neural benchmark-abc` / dispatch `benchmark-abc`
- `scripts/neural-abc.sh` → `target-dev/neural-abc.json`
- Unit tests for metrics + adapter isolation

## Must not

- Disable production Neural ranking
- Raise `SEMANTIC_WEIGHT`
- Claim neural embedding inference for lexical/hash/BM25 arms
- Fabricate memory/token numbers
