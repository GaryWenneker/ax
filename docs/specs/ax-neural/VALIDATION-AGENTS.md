# AX Neural — Real agent validation (§5)

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/VALIDATION-AGENTS.md`  
**Date:** 2026-11-16  
**Suite:** `/Users/gary/io/ax/docs/specs/ax-neural/agent-suite-30.json` (30 tasks)  
**Results:** `/Users/gary/io/ax/docs/specs/ax-neural/agent-suite-live-results.json`  
**Runner:** `scripts/neural-agent-suite.py`  
**Model / agent:** `composer-2-fast` / `cursor-agent`  
**Arms:** `B_ax_baseline` (prediction off) vs `C_ax_neural_prediction` (prediction on)  
**Controls:** identical prompt, repo snapshot, agent binary, model; only `ax.json` neural.prediction.enabled differs. Ranking remains lock_required.

## Categories (5 each)

bug_fix · new_feature · refactoring · security · performance · cross_module

## Success rates

| Arm | Strict anchors | Soft anchors (markdown-tolerant) | Tasks | Input tokens |
|---|---|---|---|---|
| B Ax baseline | **0.967** (29/30) | **1.000** | 30 | 607 528 |
| C Neural prediction | **0.933** (28/30) | **1.000** | 30 | 602 272 |

Strict failures were markdown-wrapped tokens (`PERF_FN=\`avoid_alloc\``) not wrong answers. Soft scoring treats those as hits → both arms **30/30**.

### By category (strict)

| Category | B | C |
|---|---|---|
| bug_fix | 0.80 | 1.00 |
| new_feature | 1.00 | 1.00 |
| refactoring | 1.00 | 1.00 |
| security | 1.00 | 1.00 |
| performance | 1.00 | 0.60 |
| cross_module | 1.00 | 1.00 |

## Comparison verdict

- Neural prediction **does not improve** overall task success on this suite (strict: slightly worse; soft: tie).
- Token spend: Neural used fewer input tokens (−5.3k) but more output tokens (+3.6k).
- Ceiling effect: tasks are short naming probes — weak power to show context-ranking gains.
- Production defaults unchanged (`semanticWeight=0`).

## Limits

- Not multi-turn editing / test-running work; success = anchor emission.
- n=30 per arm is the minimum requested; no confidence intervals.
- No arm that disables required Neural ranking (lock_required).
