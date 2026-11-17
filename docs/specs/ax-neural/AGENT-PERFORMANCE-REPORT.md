# Agent Performance Report — Real-World Benchmark

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/AGENT-PERFORMANCE-REPORT.md`  
**Suite:** 30 held-out tasks in `REALWORLD-TASKS.json` (multi-file, non-trivial)

## Status

| Item | State |
|---|---|
| Task definitions with automated `verify[]` | **ready** (inject RED subset) |
| Identical LLM / snapshot / budget controls | **specified** in design + runner |
| Randomized arm order + reps≥2 | **specified**; inject matrix running at reps=1 first |
| Full agent matrix (30 × operational arms × 2 reps) | **in progress** — rep 0 at **17/30** paired; rep 1 not started |
| Verify triage (no inject) | **22/22 cheap heldOut already GREEN** — false successes; not usable |
| Inject RED protocol | **30/30 heldOut** in `injects/manifest.json` |
| Paired complete (rep 0) | **17/30** heldOut |
| McNemar vs A (n=17) | B net **−1** (1/2), D net **−2** (0/2), E net **0** (1/1) |
| Latest pair | `ref-08`: A pass, B fail, D fail, E pass |
| Efficiency (44 latency samples) | input **4,779,578**; output **174,023**; latency p50 **234 s**, p95 **420 s** |
| Runner | Double-fork daemon `scripts/neural-realworld-daemon.py` (still alive) |
| Pilot coding runs | `feat-03` (pre-inject) A/D |

Primary success criterion remains: **all automated verify steps exit 0**. The agent does not self-score.

## Pilot (`feat-03`, composer-2-fast / cursor-agent)

| Arm | Verify success | Input tokens | Output tokens |
|---|---|---|---|
| A production retrieval | true | 137 957 | 3 893 |
| D prediction | true | 111 172 | 4 329 |

Verify-only baseline was **RED** before the pilot; both arms made status expose `semanticWeight=0` and `productionDefaultsUnchanged=true`. n=1 task — **not** a Neural benefit claim.

## Remaining gaps

- Full 30-task completion rate and second repetition: still running
- Regressions / retries / wrong-file selection: `not_measured` (verify is binary; those counters are not in the runner yet)
- Arm C: blocked

Interim overall success rate on scored rows: **0.56** (Wilson 95% recorded in `REALWORLD-RESULTS.json`). That rate mixes arms and is not a Neural effect. The paired nets above are the comparison.

## Classification

| Component | Class |
|---|---|
| Agent benefit of Neural ranking | `not_sufficiently_evaluated` |
| Agent benefit of prediction | `not_sufficiently_evaluated` (prior short-suite showed no gain) |
| Agent benefit of learning | `not_sufficiently_evaluated` |
| Agent benefit of ONNX fusion | `rejected_for_production` (arm blocked) |
