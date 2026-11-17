# EVIDENCE — Phase 5 feedback/learning

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/EVIDENCE-PHASE5.md`  
**Spec:** `SPEC-PHASE5-LEARN.md`  
**Spec approval:** not obtained (autonomous goal run)  
**Tests:** `cargo test -p ax-neural --lib` → **121 passed**

| Scenario | Evidence |
|---|---|
| Reproducible disjoint split | `split_dataset_is_reproducible_and_disjoint` |
| No auto-promote / no neural claim | `train_ranking_prior_does_not_auto_promote_and_skips_neural_claim` |
| Bad ratio rejected | `split_rejects_bad_ratio` |
| Existing promote gates | `promotion_allowed` + dispatch promote tests |
| Secret-shaped refusal | existing `validate_feedback` / export tests |
