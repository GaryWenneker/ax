# SPEC — Phase 5: Feedback and learning

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/SPEC-PHASE5-LEARN.md`  
**Depends on:** Phase 4  
**Tier:** 2  
**Spec approval:** not obtained (autonomous goal run)

## Already present (regression armor)

- Feedback collection with secret-shaped refusal + project isolation (`validate_feedback`)
- `training_export` without project path / source text
- Explicit promote + bake-off gate + rollback (`promotion_allowed`, `promote`, `rollback`)
- No automatic promotion

## New this phase

1. Deterministic **train / validation** split of exported feedback rows (seeded; reproducible).
2. Controlled **ranking-learning**: frequency prior weights from train outcomes only; evaluate on validation; emit a **versioned** model artifact JSON.
3. Training is separated from inference; artifact is **registered** only when the caller asks; **never** auto-promoted.
4. Poisoning: secret-shaped rows already skipped; split ignores rows without outcome success/failure signal for weight updates.

## Scenarios

1. **Given** N feedback-shaped rows and seed S, **when** `split_dataset` runs, **then** train∪val = all eligible rows, train∩val = ∅, and the same S yields the same partition.
2. **Given** a train set with successful context ids, **when** `train_ranking_prior` runs, **then** those ids get positive weights; validation metrics are reported; `promoted: false`.
3. **Given** learning output, **when** inspected, **then** it includes `datasetVersion`, `modelVersion`, `trainSize`, `validationSize`, `metrics`, and does not claim neural inference.
4. **Given** promote without `--explicit`, **when** called, **then** it still fails (unchanged).

## Must not

- Auto-promote
- Train on validation rows
- Raise `SEMANTIC_WEIGHT`
- Cross-project rows in one dataset
