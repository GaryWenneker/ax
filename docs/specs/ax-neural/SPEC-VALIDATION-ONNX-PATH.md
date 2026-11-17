# SPEC — ONNX production-path validation (§2)

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/SPEC-VALIDATION-ONNX-PATH.md`  
**Tier:** 2 (validation harness; production defaults unchanged)  
**Approval:** autonomous run — EVIDENCE will record `spec approval: not obtained (autonomous run)`.

## Goal

Measure the ONNX embedding path end-to-end in a **validation-only** harness and compare retrieval arms A–D **without** changing `SEMANTIC_WEIGHT`, the hash-v1 memory column, or production feature defaults.

## Arms

| Arm | Method |
|---|---|
| A | hash-v1 cosine retrieval (`ax_memory::embed::embed_text`) |
| B | lexical-semantic-cpu rank |
| C | ONNX MiniLM cosine retrieval (`neural-ort` + `~/.ax/models`) |
| D | Combined RRF (k=60) of A+B+C ranked lists |

## Stages (each must report `measured` \| `inactive_in_production` \| `not_built` \| `failed`)

1. Model loading  
2. Real embedding inference  
3. Vector persistence (`neural_embedding` table; not `memories.embedding`)  
4. Query embedding  
5. Similarity (cosine)  
6. Retrieval (ranked keys)  
7. Ranking fusion (validation RRF vs production `blend_semantic` / `SEMANTIC_WEIGHT`)  
8. Context delivery (`rank::plan` over retrieved texts)

## Invariants

- `semanticWeight` in report == 0.0  
- `neuralEmbeddingInferenceActive` == false  
- `productionDefaultsUnchanged` == true  
- Default `ax` binary without `neural-ort` still builds; ONNX stages report `not_built` when feature off  
- No new dependency beyond existing `neural-ort` / sqlx / fixture  

## Setup

- Files: `crates/ax-neural/src/path_validate.rs`, `scripts/neural-path-validate.sh`, result JSON under `docs/specs/ax-neural/` and `target-dev/`  
- Isolation: temp SQLite for persistence stage; coding fixture only  
- Git: no commit unless user asks  

## Acceptance

1. Harness emits JSON with stages + arms A–D locked metrics (P@3, R@3, MRR) when `neural-ort` + model present.  
2. Report documents which stages are inactive in production.  
3. Combined arm D measured; defaults unchanged.  
