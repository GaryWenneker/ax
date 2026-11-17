# Ax Neural — capability inventory

Statuses: `implemented` · `active` · `disabled` · `not_built` · `not_configured` · `not_measured`

Authoritative live report: `ax neural status` → `capabilities` (see `SPEC-PHASE1-STATUS.md`).

| Capability | Status (defaults) | Kind / notes |
|---|---|---|
| Lexical retrieval | **active** when `neural-runtime` + bake-off admission + semantic.enabled | `lexical-semantic-cpu` — **not** neural inference |
| Hash vector retrieval | **active** | `hash-v1` on `memories.embedding` |
| Neural embedding inference | **implemented** (optional) / **inactive** in default binary | `neural-ort` + local MiniLM ONNX measured & gate-admitted; `neuralEmbeddingInferenceActive=false` until production promotion; Candle **not_built** |
| Neural ranking | **active** | Deterministic context ranking (required); not a neural net |
| Statistical graph | **disabled** | Enabled via `neural.graph.enabled` |
| Graph neural network | **not_built** | |
| Prediction | **disabled** (implemented) | `neural.prediction.enabled`; explanation, evidence, relationship inferred\|verified (Phase 4) |
| Prediction held-out evaluate (`ax neural evaluate`) | **implemented** | **measured** / **offline** only — leave-one-out on built-in coding fixture (no live DB required). JSON exports `ranking` (`precisionAt3`, `precisionAt5`, `recallAt3`, `mrr`, `samples`) and `calibration` (confidence buckets vs hit rate, `buckets[]` with `lo`/`hi`/`n`/`empiricalAccuracy`). Does **not** enable production ONNX or raise `SEMANTIC_WEIGHT`. Evidence: `predict-validate-results.json`, `VALIDATION-PREDICT.md`. |
| Feedback collection | **disabled** | `neural.learning.feedback_collection` (Phase 5) |
| Learning / training | **disabled** (partial) | Offline `ax neural learn` ranking-prior + train/val split; no auto-promote (Phase 5) |
| Model promotion | **implemented** | Explicit promote/rollback only |
| Hardware acceleration | **not_built** | CPU available; CUDA/DirectML/CoreML not linked |
| A/B/C retrieval benchmark | **implemented** | `ax neural benchmark-abc` / `scripts/neural-abc.sh` |
| Embed bake-off | **implemented** | `ax neural embed-bakeoff` / ort script; ONNX measured |
| Agent evaluation (fixture) | **implemented** | `ax neural agent-eval` / `scripts/neural-agent-eval.sh` — measured (incl. RSS) |
| Agent evaluation (live LLM) | **measured** | `scripts/neural-agent-live.py` via `cursor-agent` (`agent-eval-live-results.json`); successRate 1.0 on B/C/E |
| Linux ARM64 Neural runtime | **measured** | `linux-arm64-runtime.json` — 124 tests |
| Linux x86_64 Neural runtime | **measured** | `linux-x86_64-runtime.json` — 124 tests |
| macOS x86_64 Neural runtime | **measured** | `macos-x86_64-runtime.json` — 124 tests (Rosetta) |
| Windows x86_64 Neural runtime | **measured** | `windows-x86_64-runtime.json` — 124 tests (Wine) |
| Windows ARM64 Neural runtime | **measured** | `windows-arm64-runtime.json` — 124 tests (Hangover) |
| Candle embedding runtime | **not_built** | Not linked; ONNX selected by measured bake-off |

## Phase 6 note

Fixture harness arms A–E measured (`agent-eval-results.json`), including leave-one-out ranking-prior learning (`promoted=false`). Live LLM task success / tokens / retries remain **`not_measured`**. Attribution is to ranking/prediction/learning adapters on a fixed fixture, not an LLM.

## Phase 5 note

`split_dataset` + `train_ranking_prior` / `ax neural learn`: reproducible train/validation split; frequency ranking prior artifact with `promoted: false`. Feedback poisoning still refused via secret-shaped checks. Promotion remains explicit + bake-off gated.

## Phase 4 note

Predictions include `explanation`, `evidence[]`, and `relationship` (`inferred` | `verified`). Kinds: memory, skill, rule, file, pattern, graph. Default remains disabled. Leave-one-out context-usage eval scores memory predictions only.

`ax neural evaluate` (CLI/MCP `evaluate`) prints the same measured JSON as `evaluate_fixture`: core precision/coverage counts, plus **`ranking`** (P@3, P@5, R@3, MRR on memory predictions) and **`calibration`** (fixed confidence buckets). All fields are offline fixture scores — not live-agent traces and not a claim that ONNX embedding inference is active in production (`neuralEmbeddingInferenceActive=false`, hash-v1 column unchanged).

## Phase 3 measurement note

`scripts/neural-embed-bakeoff-ort.sh` (2026-11-16): ONNX MiniLM paraphrase Recall@3 = **1.0** vs hash-v1 = **0.0**; held-out R@3 ≈ **0.833** vs hash ≈ **0.667** → `neuralEmbeddingAdmitted=true`. Winner `onnx-runtime-cpu`. Candle remains **`not_built`**. Production: `neuralEmbeddingInferenceActive=false`, `SEMANTIC_WEIGHT=0`, memory column `hash-v1`.

## Phase 2 measurement note

On locked families (2026-11-16, `abc-results.json`): arm C (lexical) raises locked Recall@3 / MRR / NDCG vs A/B but lowers Precision@k. On held-out families, C does **not** beat B — do not raise `SEMANTIC_WEIGHT` from these numbers alone.

## Deprecated field

`semanticActive` is always `false` until `neuralEmbeddingInference` is `active`. Do not treat it as “semantic features on.”
