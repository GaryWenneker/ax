# AX Neural — Production Validation Evidence Audit (§1)

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/VALIDATION-EVIDENCE-AUDIT.md`  
**Date:** 2026-11-16  
**Scope:** Independently verify claimed results; identify gaps before ONNX/prediction/learning/agent validation.  
**Rule:** Do not change production defaults (`SEMANTIC_WEIGHT=0`, hash-v1 memory column, `neuralEmbeddingInferenceActive=false`) until held-out evidence justifies it.

## Reproduction runs (this session)

| Artifact | Command | Result vs checked-in |
|---|---|---|
| `abc-results.json` | `./scripts/neural-abc.sh` → `target-dev/neural-abc.json` | **Reproduced.** Locked P@3/R@3 identical for arms A/B/C; `semanticWeight=0`. RSS ~32.8–32.9 MiB (process noise). |
| `agent-eval-results.json` | `./scripts/neural-agent-eval.sh` | **Reproduced.** Arms A/B/C/E context P@3/R@3 match; `productionDefaultsUnchanged=true`. |
| `embed-bakeoff-results.json` (default binary) | `./scripts/neural-embed-bakeoff.sh` | **Reproduced path.** Default `ax` has `neural-ort` **not** linked → ONNX candidate `not_built` in this binary. Checked-in JSON is from `neural-embed-bakeoff-ort.sh` (feature build). |
| `agent-eval-live-results.json` | not re-run | **Not re-run.** Live LLM cost/time; treat as historical n=1 task until §5. |
| Platform `*-runtime.json` | not re-run | **Not re-run.** Documented hosts (Podman/Wine/Rosetta/Hangover); suites claimed 124-pass. Re-run only if platform claims are disputed. |

## Sample sizes (authoritative JSON)

| Study | Locked n | Held-out n | Notes |
|---|---|---|---|
| A/B/C retrieval (`abc-results.json`) | **4** questions (3 scored samples for P/R after abstention) | **15** questions (12 scored) | Means only; **no CI / bootstrap / significance test**. |
| Embed bake-off | **4** locked | ONNX held-out summary present in ort JSON | Paraphrase gate + held-out vs hash; still tiny n. |
| Fixture agent-eval | same coding fixture families | — | Proxy for context quality; **no LLM**. |
| Live agent-eval | **1** task × 3 arms (B/C/E) | — | Identical cursor-agent / composer-2-fast; successRate 1.0 on anchors. **Far below §5 ≥30 requirement.** |

## Controls / independence / confidence

| Dimension | Status |
|---|---|
| Control groups | A/B/C arms present for retrieval; live B vs C vs E. **No arm that disables all Ax context** as a pure LLM baseline in live JSON. |
| Test independence | Fixture questions share one coding corpus → **not independent across families**. Live task is a single parser prompt → **one dependent unit**. |
| Statistical confidence | **Missing** for all mean metrics (no SE, CI, p-values, power). With n≈3–12 scored, effect sizes are descriptive only. |
| Train/eval leakage (learning) | Phase 5 claims train/val split + no auto-promote. Live arm E seeds feedback — **must re-verify in §4 that eval tasks are held out**. |
| Environmental limits | Default production binary: `neuralEmbeddingInference=not_built` (ort not linked). ONNX measured only under `--features neural-ort`. Windows runtimes via Wine/Hangover, not native OS. Memory RSS measured on macOS host. |

## Claim vs production path

| Claim | Production truth |
|---|---|
| ONNX MiniLM admitted | **Bake-off / feature-build only.** `neuralEmbeddingInferenceActive=false`; memory column remains **hash-v1**. |
| Lexical “neural ranking” arm C | **Not neural embedding inference** (`claimsNeuralInference=false`). |
| Live successRate 1.0 | Anchor-token task success on **1** prompt — does **not** prove coding-task improvement. |
| Score 4 / COMPLETE (TEST-REPORT) | Implementation goal complete; **production validation goal not satisfied** by that audit. |

## Gaps blocking “measurable production improvement”

1. **ONNX end-to-end production path** not active: load/infer measured in bake-off; **vector persistence + ranking fusion + context delivery with ONNX** still off (`SEMANTIC_WEIGHT=0`).
2. **Combined arm D** (hash+lexical+ONNX fusion) not evidenced as a production or validation report separate from bake-off candidates.
3. **Held-out prediction** metrics beyond fixture leave-one-out / arm D proxy incomplete for coding-task prediction accuracy + calibration.
4. **Learning before/after + restart** not independently re-validated under this goal.
5. **≥30 identical-LLM coding tasks** across categories — **missing** (live n=1).

## Verdict on §1

- Checked-in fixture/ABC metrics are **reproducible** on this host with current `target-dev/debug/ax`.
- Checked-in ONNX numbers require the **ort feature build** + local MiniLM files (present under `~/.ax/models/`).
- Evidence is **insufficient** to claim production improvement: tiny n, no stats, inactive ONNX path, live eval n=1.

## Follow-on validation (this goal)

| Section | Artifact | Status |
|---|---|---|
| §2 ONNX path A–D | `VALIDATION-ONNX-PATH.md`, `path-validate-results.json` | **measured**; fusion inactive in production; defaults unchanged |
| §3 Prediction | `VALIDATION-PREDICT.md`, `predict-validate-results.json` | **measured**; P@k≈0.33; FP=0; calibration under-powered |
| §4 Learning | `VALIDATION-LEARN.md`, `learn-validate-results.json` | **measured**; R@3↑ MRR↓; no promote; restart stable |
| §5 ≥30 agents | `agent-suite-30.json`, `scripts/neural-agent-suite.py`, `agent-suite-live-results.json` | **in progress** (identical composer-2-fast / cursor-agent) |
