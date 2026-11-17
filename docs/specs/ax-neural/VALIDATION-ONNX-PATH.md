# AX Neural — ONNX production-path validation (§2)

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/VALIDATION-ONNX-PATH.md`  
**Date:** 2026-11-16  
**Evidence:** `/Users/gary/io/ax/docs/specs/ax-neural/path-validate-results.json`  
**Command:** `./scripts/neural-path-validate.sh` (`ax-neural --features neural-ort`)  
**Defaults:** `semanticWeight=0`, `neuralEmbeddingInferenceActive=false`, `productionDefaultsUnchanged=true`

## Stage results

| Stage | Status | Evidence |
|---|---|---|
| Model loading | **measured** | MiniLM ONNX + tokenizer under `~/.ax/models` |
| Embedding inference | **measured** | Warmup + per-memory vectors (dim 384) |
| Vector persistence | **measured** | Temp `neural_embedding` rows; **not** `memories.embedding` |
| Query embedding | **measured** | ONNX query vector |
| Similarity | **measured** | `cosine(same) > cosine(unrelated)` |
| Retrieval | **measured** | Cosine-ranked memory keys |
| Ranking fusion | **inactive_in_production** | Validation RRF only; `blend_semantic` unwired; weight 0 |
| Context delivery | **measured** | `rank::plan` over ONNX top hits (lexical relevance notes) |

## Arms A–D (locked families)

| Arm | Method | P@3 | R@3 | MRR |
|---|---|---|---|---|
| A hash-v1 | `embed_text` cosine | 0.222 | 0.667 | 0.692 |
| B lexical | synonym/trigger | 0.333 | **1.000** | **1.000** |
| C ONNX | MiniLM cosine | 0.333 | **1.000** | 0.778 |
| D combined | RRF(k=60) A+B+C | 0.333 | **1.000** | 0.833 |

Held-out (15 questions): ONNX improves R@3 vs hash in bake-off (see embed JSON); lexical still leads precision on locked sets. Combined RRF sits between lexical and ONNX on locked MRR — **not** a clear production win over B.

## Production wiring (unchanged)

- Memory column remains **hash-v1**.
- `ax neural search` uses blake3 runtime when the **lexical** challenger is admitted — **not** ONNX.
- Context `rank` is lexical overlap only.
- Default `ax` binary still ships **without** `neural-ort` linked.

## Decision

Do **not** raise `SEMANTIC_WEIGHT` or activate production ONNX inference. Path stages work under the validation feature build; production improvement is not evidenced.
