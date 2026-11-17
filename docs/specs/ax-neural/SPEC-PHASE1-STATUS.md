# SPEC — Phase 1: Correct capability reporting

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/SPEC-PHASE1-STATUS.md`  
**Tier:** 2  
**Goal:** `/goal` AX Neural next implementation phase — Phase 1 only  
**Spec approval:** goal authorization (autonomous run)

## Problem

`ax neural status` exposes `semanticActive: true` when the lexical bake-off challenger is admitted. That implies neural embedding inference is running. Lexical matching is not neural inference.

## Acceptance

### S1 — `capabilities` object

`status::report` includes `capabilities` with these keys. Each value is an object:

```json
{ "status": "<enum>", "kind": "<string>", "detail": "<string>" }
```

Allowed `status` values only: `implemented` | `active` | `disabled` | `not_built` | `not_configured` | `not_measured`.

| Key | Expected for current product (defaults) |
|---|---|
| `lexicalRetrieval` | `active` when `semantic_enabled` and lexical challenger admitted; else `implemented` + detail why inactive |
| `hashVectorRetrieval` | `active`, kind `hash-v1` |
| `neuralEmbeddingInference` | `not_built` (Candle/ONNX not linked) |
| `neuralRanking` | `active` when ranking required/on; kind `deterministic_context_ranking` (not a neural net) |
| `statisticalGraph` | `disabled` by default; `active` when `graph_enabled` |
| `graphNeuralNetwork` | `not_built` |
| `prediction` | `disabled` by default; `active` when prediction config on |
| `feedbackCollection` | `disabled` by default; `active` when feedback_collection on |
| `learningAndTraining` | `disabled` (no training loop); automatic training never active |
| `modelPromotion` | `implemented` / `disabled` unless a promoted model exists (manual only; auto promotion never active) |
| `hardwareAcceleration` | CPU path `active` or `implemented`; CUDA/DirectML/CoreML `not_built` |

### S2 — No false neural claim via `semanticActive`

`semanticActive` must **not** become true solely because lexical retrieval is admitted.  
Until real neural embedding inference is `active`, `semanticActive` is `false`.  
(Deprecated field kept for CLI/UI compatibility.)

### S3 — Tests

| Test | Expectation |
|---|---|
| `capabilities_report_distinguishes_lexical_from_neural_inference` | `neuralEmbeddingInference.status == not_built`; when runtime+admitted, `lexicalRetrieval.status == active`; `semanticActive == false` |
| `capabilities_hash_vector_and_ranking_are_active` | hash + ranking active under required config |
| Existing status tests | Still pass; update assertions that required `semanticActive == challenger_admitted()` |

### S4 — Docs

- `CAPABILITIES.md` lists implemented / partial / disabled / unimplemented  
- `ARCHITECTURE.md` notes status honesty  
- `TEST-REPORT.md` §2 updated so semantic ≠ neural inference  

## Must not

- Raise `SEMANTIC_WEIGHT`  
- Link Candle/ONNX  
- Fabricate active neural inference  
- Change production defaults for prediction/graph/feedback  

## Setup

Isolation: none (continue uncommitted Neural tree). No new crates.
