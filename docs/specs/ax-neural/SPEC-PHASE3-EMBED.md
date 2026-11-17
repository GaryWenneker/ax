# SPEC — Phase 3: Genuine neural semantic inference (bake-off)

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/SPEC-PHASE3-EMBED.md`  
**Depends on:** Phase 2 (`SPEC-PHASE2-BENCHMARK.md`)  
**Tier:** 3  
**Spec approval:** not obtained (autonomous goal run)

## Goal

Compare **lexical-semantic-cpu** and **hash-v1** against **actual neural embedding inference** on the same coding fixture, offline, CPU-first, without raising `SEMANTIC_WEIGHT` or changing production defaults.

## Candidates (must be measured or explicitly `not_built` / `not_configured`)

| Id | Kind | Claim |
|---|---|---|
| `hash-v1` | Hash trigram | Not neural |
| `lexical-semantic-cpu` | Lexical + synonym | Not neural |
| `cpu-blake3` | Deterministic hash vector | Not neural; never admit as semantic |
| `onnx-runtime-cpu` | ONNX Runtime + local model file | Neural only when built **and** model checksum present |
| `candle-cpu` | Candle + local model file | Neural only when built **and** model checksum present |

## Pass / admit rule (extends ADR 002)

A neural challenger is admitted only when **all** hold:

1. Runtime feature actually links the inference library.
2. Model file is local under `~/.ax/models/<id>/`, checksummed; no download on DB open.
3. Locked-family paraphrase Recall@3 **strictly** above hash-v1; named_trigger / abstention / delivery_budget not worse.
4. Held-out mean Recall@3 from Phase 2 split is not worse than arm B (Ax fusion) — or admission is refused with reason.
5. Latency measured; RSS recorded or `not_measured` with reason.
6. Status `neuralEmbeddingInference` becomes `active` only while that admitted model is configured and healthy.

Failing any line → winner stays lexical (retrieval challenger) / hash-v1 (memory column); capabilities stay honest.

## Scenarios

1. **Given** default binary without ONNX/Candle features, **when** status runs, **then** `neuralEmbeddingInference` is `not_built` (not `active`).
2. **Given** feature linked but no model file, **when** status/bake-off runs, **then** status is `not_configured` and no admission.
3. **Given** `embed-bakeoff` report, **when** emitted, **then** every candidate has `kind` in {`measured`,`not_built`,`not_configured`} and never claims neural for hash/lexical/blake3.
4. **Given** no admitted neural model, **when** production search runs, **then** memory column stays hash-v1 and `SEMANTIC_WEIGHT` stays 0.

## Deliverables

- `crates/ax-neural/src/embed_bakeoff.rs` — comparable metrics (reuse Phase 2 helpers)
- `ax neural embed-bakeoff` / MCP `ax_neural_embed_bakeoff`
- `scripts/neural-embed-bakeoff.sh` → `target-dev/neural-embed-bakeoff.json`
- Updated `CAPABILITIES.md` / `TEST-REPORT.md` / ADR 002 amendment note
- Optional Cargo features `neural-ort` / `neural-candle` (default **off**)

## Must not

- Link ONNX/Candle into the default production binary without measured admission
- Download models on `ax init` / DB open
- Set `semanticActive=true` or raise `SEMANTIC_WEIGHT` without held-out win
- Fabricate latency/RSS/quality numbers for `not_built` runtimes
