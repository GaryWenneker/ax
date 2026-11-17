# SPEC — Phase 6: Real agent evaluation

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/SPEC-PHASE6-AGENT.md`  
**Status:** harness scaffold; live LLM runs **not_measured** until executed under controlled identical conditions.

## Arms

1. Existing Ax (no Neural ranking in agent loop — if ranking required, document that production cannot disable it and compare prediction/learn off vs on instead)
2. Ax + Neural ranking
3. Ax + Neural ranking + prediction
4. Ax + ranking + prediction + learning after feedback

## Metrics

Task success, context P/R, tokens in/out, wall time, retries, retrieval latency, prediction accuracy.

## Constraint

Identical agent models and task conditions; attribute gains to Ax Neural, not the LLM.
