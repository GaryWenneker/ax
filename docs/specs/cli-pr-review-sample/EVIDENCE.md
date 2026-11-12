# EVIDENCE — CLI PR-review quality (Wave A + Wave B)

**Spec approval:** QUALITY_PLAN approved (`approve`); gold signed (`sign gold`)  
**Date:** 2026-11-12

## Baseline (frozen)

| Metric | Value |
|---|---|
| median UCP@5 | 0.16233766233766234 |
| median recall@5 | 0.16666666666666666 |
| S2 skill inclusion | 1.0 |
| Source | `baseline/metrics.json` |

## Wave A (candidate)

| Metric | Value |
|---|---|
| median UCP@5 | 0.24404761904761904 |
| median recall@5 | (S1 gate passed vs baseline) |
| S2 | 1.0 |
| Source | `runs/wave-A/metrics.json` |

### compare.sh Wave A

```json
{
  "primary": {
    "metric": "median_ucp_at_k",
    "before": 0.16233766233766234,
    "after": 0.24404761904761904,
    "lift": 0.5033333333333333
  },
  "ship": true,
  "claimAllowed": ">=50%"
}
```

Exit code: **0** — claim **≥50%** only.

## Wave B (candidate)

| Metric | Value |
|---|---|
| median UCP@5 | 1.0 |
| median recall@5 | 0.256578947368421 |
| S2 | 1.0 |
| Source | `runs/wave-B/metrics.json` |

### compare.sh Wave B (authoritative for ≥90%)

```json
{
  "primary": {
    "metric": "median_ucp_at_k",
    "before": 0.16233766233766234,
    "after": 1.0,
    "lift": 5.16
  },
  "secondaries": {
    "S1_recall_ok": true,
    "S2_skill_inclusion_before": 1.0,
    "S2_skill_inclusion_after": 1.0,
    "n_scored_before": 10,
    "n_scored_after": 10
  },
  "ship": true,
  "claimAllowed": ">=90%"
}
```

Exit code: **0**

Scorer unit tests: `python3 docs/specs/cli-pr-review-sample/tools/test_score_ucp.py` → 4 OK.  
Frozen baseline re-scored with same dual scorer still **0.16233766233766234**.

## Claim allowed

**≥90%** useful-context precision improvement on the frozen 10-PR suite under Wave B protocol (`compare.sh` `claimAllowed: ">=90%"`).  
Wave A remains valid for **≥50%** under its own protocol.  
**Not** claimed: human review-grade lift beyond UCP/recall gates, or product CLI `ax node` source-parity changes (optional later).

## Mapping

| Plan item | Artifact |
|---|---|
| Gold sign | `GOLD_SIGN.md` + `tasks.json` `gold.signed=true` |
| Baseline freeze | `baseline/metrics.json` |
| Wave A protocol | `WAVE_A_PROTOCOL.md` + `runners/run_cli_turn.py --wave A` |
| Wave B protocol | `WAVE_B_PROTOCOL.md` + `runners/run_cli_turn.py --wave B` |
| Scorer tests | `tools/test_score_ucp.py` (4 ok) |
