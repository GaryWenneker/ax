# ADR 002 — Embedding bake-off

## Problem

Hash-v1 is a trigram feature hash, not a semantic model. The memory bench already has a paraphrase question. A second model is justified only if it beats hash-v1 on that question without regressing the locked families, and only if it can ship as an optional CPU runtime.

## Candidates

| Candidate | Role in this decision |
|---|---|
| hash-v1 | Baseline. Always built. `ax_memory::embed`. |
| ONNX Runtime CPU | Not linked. Recorded as `not_built`. |
| Candle CPU | Not linked. Recorded as `not_built`. |

Burn, Tract, CUDA, DirectML, Core ML, Vulkan, and WebGPU are out of this decision.

## Pass rule

A challenger is admitted only when all of these hold:

1. The runtime is actually built (`neural-runtime` or a successor feature).
2. On `benchmarks/memory/coding.json`, paraphrase recall@3 is strictly higher than hash-v1.
3. `named_trigger`, `abstention`, and `delivery_budget` are not worse than hash-v1.
4. Query latency and process RSS are recorded.
5. The model file, if any, is opt-in, checksummed, and stored under `~/.ax/models`. Nothing is downloaded on database open.

Failing any line means the winner stays hash-v1. Hash-v1 is not copied into `neural_embedding`. `memories.embedding` stays the hash column.

## What this run can decide

This workspace has no ONNX Runtime or Candle dependency. Both challengers are `not_built`. The winner is hash-v1. No second model is stored. The numbers for hash-v1 on the coding fixture are in `bakeoff-results.json` after `ax neural benchmark` or the `ax-neural` bake-off test.

## Locked fusion

`fuse_scores` weights are not retuned. A semantic term may be added only with weight `0.0` until a later held-out run versions a new weight set. `blend_semantic` with that weight returns the fused score unchanged.

## Amendment

`docs/specs/memory-hash-only.md` is not edited by this work. `hash-only-amendment.md` is a proposal: hash-v1 remains the default model on `memories.embedding`; a second model would be allowed only in `neural_embedding` after this bake-off admits it. That amendment is not applied.

## Phase 3 update (2026-11-16)

Measured via `embed-bakeoff`: lexical-semantic-cpu paraphrase Recall@3 = 1.0; hash-v1 and cpu-blake3 = 0.0. ONNX and Candle remain `not_built` (not linked). No neural embedding model admitted. Decision stands: do not change production memory column or `SEMANTIC_WEIGHT` until a local checksummed neural model beats locked and held-out rules.
