# EVIDENCE — Phase 6 agent evaluation

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/EVIDENCE-PHASE6.md`  
**Spec:** `/Users/gary/io/ax/docs/specs/ax-neural/SPEC-PHASE6-AGENT.md`  
**Spec approval:** not obtained (autonomous goal run)  
**Sources:**
- Fixture: `docs/specs/ax-neural/agent-eval-results.json` via `scripts/neural-agent-eval.sh`
- Live: `docs/specs/ax-neural/agent-eval-live-results.json` via `scripts/neural-agent-live.py`  
**Tests:** `cargo test -p ax-neural --lib` → **124 passed**

## Spec → evidence

| Scenario | Result |
|---|---|
| Fixture arms A–E measured | `kind: measured`; locked context P/R/MRR/NDCG per arm; RSS measured |
| A / B | P@3=R@3≈0.667 |
| C Neural ranking (lexical admitted) | P@3≈0.333, R@3=1.0, MRR=1.0 |
| D Prediction-as-context | context P/R@3=0.0; leave-one-out evaluateFixture precision=1.0 coverage≈0.33 |
| E Learning (ranking-prior LOO) | context P@3≈0.333, R@3=1.0; trainReports `promoted=false` |
| Live identical LLM task success | **measured** — `cursor-agent` model `composer-2-fast`; successRate **1.0** on arms B/C/E |
| Live tokens | **measured** — B in/out 14433/450; C 17662/736; E 13505/644 |
| Live latency | **measured** — ~9–15 s per arm |
| Retries | **`not_measured`** — single-shot turns |
| Attribution | Same agent binary + model + prompt; only ax.json prediction/learning flags differ |
| Production defaults | `productionDefaultsUnchanged: true`, `semanticWeight: 0.0` |
| Arm A (disable ranking) | Not runnable — ranking is `lock_required` (documented) |

## Gauntlet

| Layer | Result |
|---|---|
| Unit | agent_eval + rss in 124-pass suite |
| Fixture execution | `./scripts/neural-agent-eval.sh` exit 0 |
| Live LLM | `./scripts/neural-agent-live.py --out docs/specs/ax-neural/agent-eval-live-results.json` with authenticated `cursor-agent` (claude CLI was unauthenticated) |
