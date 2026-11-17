# Ax Neural — requirement-by-requirement completion audit

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/COMPLETION-AUDIT.md`  
**Date:** 2026-11-16  
**Goal status:** COMPLETE (evidence below)

Evidence rule: uncertain or `not_measured` ⇒ not achieved.

## Mandatory constraints

| # | Constraint | Evidence | Verdict |
|---|---|---|---|
| 1 | Preserve existing Ax functionality / public interfaces | No public API removals; Neural optional features; production defaults unchanged in JSON reports | **held** |
| 2 | Do not rewrite Ax architecture | Additive `crates/ax-neural` + CLI/MCP dispatch | **held** |
| 3 | Rust-first, cross-platform | Rust crate; six-target `PLATFORM.md` with runtime JSON | **held** |
| 4 | Neural optional + offline | Default path offline; `neural-ort` optional | **held** |
| 5 | No mandatory cloud/LLM | No cloud deps required; live eval used local Cursor agent optionally | **held** |
| 6 | No false neural claims | Phase 1 statuses; lexical/hash `claimsNeuralInference=false` | **held** |
| 7 | Production defaults unchanged until proven | `SEMANTIC_WEIGHT=0`, hash-v1 memory column; `neuralEmbeddingInferenceActive=false` | **held** |
| 8 | No SEMANTIC_WEIGHT bump without held-out evidence | Held-out lexical weaker than B; ONNX gate-admitted only | **held** |
| 9 | No fabricated results | JSON kinds measured / not_built / not_measured | **held** |

## Phases

| Phase | Requirement | Evidence | Verdict |
|---|---|---|---|
| 1 | Honest capability reporting | `CAPABILITIES.md`, status fields, SPEC-PHASE1 | **done** |
| 2 | A/B/C + P/R/MRR/NDCG + latency/memory/tokens | `abc-results.json` (RSS measured; tokens N/A without LLM) | **done** |
| 3 | Genuine neural embed bake-off | ONNX MiniLM measured+admitted; Candle `not_built`; production inactive | **done** |
| 4 | Prediction + explanation/evidence/relationship | predict + EVIDENCE-PHASE4 | **done** |
| 5 | Feedback/learning + promotion gates | learn + train/val; no auto-promote | **done** |
| 6 | Real agent evaluation (identical LLM) | Fixture A–E + live B/C/E (`agent-eval-live-results.json`, successRate 1.0) | **done** |
| 7 | Cross-platform runtime | All six targets have runtime JSON with 124-pass suites (Wine/Hangover/Rosetta/QEMU hosts documented) | **done** |

## Deliverables

| # | Deliverable | Path / evidence | Verdict |
|---|---|---|---|
| 1 | Updated architecture docs | `ARCHITECTURE.md` | **done** |
| 2 | Capability/status reporting | `CAPABILITIES.md` + `ax neural status` | **done** |
| 3 | Complete A/B/C benchmark | `abc-results.json` | **done** |
| 4 | Neural embedding runtime comparison | `embed-bakeoff-results.json` (ONNX measured; Candle not_built) | **done** |
| 5 | Prediction + evaluation | predict + leave-one-out | **done** |
| 6 | Feedback and learning | learn + EVIDENCE-PHASE5 | **done** |
| 7 | Reproducible benchmark datasets | coding fixture + scripts | **done** |
| 8 | Cross-platform validation report | `PLATFORM.md` + per-target runtime JSON | **done** |
| 9 | Updated TEST REPORT | `TEST-REPORT.md` | **done** |
| 10 | Explicit capability inventory | `CAPABILITIES.md` | **done** |

## Known limits (disclosed, not blockers)

- Candle remains `not_built` — ONNX was selected by measured bake-off; Candle was not linked for a second comparison.
- Windows x86_64/ARM64 runtimes used Wine/Hangover (documented), not native Windows OS hosts.
- Production neural embedding stays inactive; `SEMANTIC_WEIGHT` remains 0.
