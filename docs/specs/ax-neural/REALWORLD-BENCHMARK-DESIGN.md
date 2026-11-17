# AX Neural — Real-World Coding Intelligence Benchmark Design

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/REALWORLD-BENCHMARK-DESIGN.md`  
**Date:** 2026-11-16  
**Prior validation (frozen):** ONNX not justified · prediction not shown to help agents · learning not promoted · no production defaults change · no architecture redesign.

## Goal

Discover whether Ax Neural **genuinely** improves realistic software-engineering outcomes on the Ax repository. Negative results are valid.

## Repository snapshot

| Field | Value |
|---|---|
| Repo | `/Users/gary/io/ax` |
| Intended commit | `85fa176a9b4b6ce5e33687bee268767d87731c60` |
| Execution rule | Clean git worktree at that SHA (or record `dirtyTree=true` if forced to run dirty) |
| Language | Rust workspace + Command Center web-ui (TS) where cited |

## Task suite

- **Count:** 50 (`REALWORLD-TASKS.json`)
- **Categories (10 each):** `multi_file_bug`, `new_feature`, `architectural_refactor`, `security`, `performance`
- **Not allowed:** naming probes, single-keyword lookups, single-file “find the symbol” prompts
- **Each task includes:** id, category, difficulty (`easy`/`medium`/`hard`), description, acceptanceCriteria[], expectedRelevantFiles[], expectedContextSymbols[], verify[] (automated commands/assertions), split (`train`|`heldOut`), estimatedFilesTouched

### Train / held-out split

| Split | Count | Use |
|---|---|---|
| `train` | 20 | Learning arm E feedback collection only |
| `heldOut` | 30 | All comparative metrics; never used to train |

Neural implementation must **not** be tuned against held-out tasks during this benchmark.

### Verification must be able to go RED

Bug-fix tasks that only run an already-green suite are invalid. A 2026-11-16 triage found **22/22** cheap held-out suite verifies already green without agent work — those cannot measure Neural benefit.

**Inject protocol (authoritative for agent scoring):**

1. `docs/specs/ax-neural/injects/manifest.json` defines `fixed`/`broken` anchors per task  
2. `scripts/neural-realworld-inject.py break|fix --task <id>` mutates the tree  
3. `scripts/neural-realworld-agent.py` requires **baseline RED** after break, then agent, then verify, then restore  

All **30 held-out** tasks now have injects. Harness bootstrap tasks (`feat-05`/`feat-07`/`feat-10`) stay in **train** and are excluded from Neural benefit claims.

## Experimental arms

| Arm | Intent | Production impact |
|---|---|---|
| A | Existing Ax production retrieval / locked fusion | Baseline |
| B | Ax + Neural ranking (lock_required path as shipped) | Ranking already required; config flags only |
| C | Ax + ONNX embeddings + experimental fusion | **Must not** change `SEMANTIC_WEIGHT`; if path inactive → `blocked` |
| D | Prediction enabled | `neural.prediction.enabled=true` only |
| E | Learning from **train-split feedback only** | Candidate artifact; `promoted=false` |

If an arm cannot run end-to-end without simulating behavior, mark **`blocked`** with reason (do not fake scores).

## Execution controls

- Identical LLM model/version (`composer-2-fast` unless overridden via env)
- Identical task prompt text per task
- Identical snapshot / worktree
- Identical token budget and tool permissions
- Identical wall-clock / turn limits
- **≥2 independent repetitions** per arm×heldOut-task (default `reps=2`)
- **Randomize arm order** per repetition (seeded)
- Log every config delta in results JSON

## Evaluation (separate tracks)

### Retrieval (no LLM self-score)

Compare ranked context / file lists to `expectedRelevantFiles` / symbols:

- P@3, P@5, R@3, R@5, MRR, NDCG@5/@10
- Bootstrap 95% CIs when n≥10; paired tests (Wilcoxon / paired bootstrap) across arms; report effect sizes (Cliff’s Δ or paired mean Δ)

### Agent coding (automated primary)

- Task completion = all `verify[]` steps exit 0
- Automated test pass rate, regression count (suite delta), retry count, incorrect file selection
- Agent narrative must **not** decide success

### Efficiency

Input / output / total tokens; latency median and p95

### Learning & prediction

Accuracy, calibration, before/after ranking on held-out, negative transfer

## Production decision gates

Unchanged from the objective (ONNX / prediction / learning). No component promoted for “implementation complete.”

## Deliverables

| File | Role |
|---|---|
| `REALWORLD-BENCHMARK-DESIGN.md` | This design |
| `REALWORLD-TASKS.json` | 50 tasks |
| `REALWORLD-RESULTS.json` | Raw runs + aggregates |
| `RETRIEVAL-QUALITY-REPORT.md` | Retrieval metrics |
| `AGENT-PERFORMANCE-REPORT.md` | Coding outcomes |
| `LEARNING-AND-PREDICTION-REPORT.md` | Learn/predict |
| `PRODUCTION-DECISION.md` | Gate decisions + classifications |

## Classifications

Each component: `production_validated` | `experimentally_promising` | `implemented_not_beneficial` | `not_sufficiently_evaluated` | `rejected_for_production`
