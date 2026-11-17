# Retrieval Quality Report — Real-World Benchmark

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/RETRIEVAL-QUALITY-REPORT.md`  
**Source:** `REALWORLD-RESULTS.json` · `REALWORLD-TASKS.json` (30 held-out tasks)

## Method

Independent of LLM final answers: compare a ranked file list to each task’s `expectedRelevantFiles`.

**Current implementation:** `ax neural rank` over file snippets + distractors (`neural-realworld-run.py`).  
**Critical honesty note:** arms A/B/D/E all use the **same** lock_required rank CLI path, so **arm deltas are identically zero** under current wiring. Prediction/learning flags do not change `neural rank`. This measures baseline ranking quality vs gold files — **not** Neural upgrade benefit. Arm **C** is **blocked** (ONNX inactive); not simulated.

## Held-out means (`ax neural rank`, n=30)

| Metric | Value |
|---|---|
| P@3 | 0.544 |
| P@5 | 0.393 |
| R@3 | 0.676 |
| R@5 | 0.810 |
| MRR | 0.882 |
| NDCG@5 | 0.766 |
| NDCG@10 | 0.875 |

Bootstrap CI / paired tests across arms: **not applicable** (identical vectors by construction; effect size 0).

## Classification

| Component | Class |
|---|---|
| Production hash/lexical retrieval (as used in proxy scaffolding) | `not_sufficiently_evaluated` (proxy ≠ production retriever) |
| Neural ranking impact on retrieval | `not_sufficiently_evaluated` |
| ONNX experimental fusion | `rejected_for_production` (blocked + prior validation) |
| Prediction impact on retrieval | `not_sufficiently_evaluated` |
