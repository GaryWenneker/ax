# Prediction calibration

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/PREDICTION-CALIBRATION.md`  
**Date:** 2026-11-16  
**Code:** `crates/ax-neural/src/recovery.rs` prediction gate

Existing `predict()` is unchanged. It still drops low confidence inside its own function. The experimental selector adds a second gate and does not replace `predict()`.

## Accept only when all of these hold

- The recovery flag is enabled. Otherwise the baseline is returned and the prediction is not applied.
- `authorized` is true and `project_id` matches the allowed project.
- `calibrated_on_independent_validation` is true. This flag is an input from a finished validation study. The selector does not invent it.
- `evidence_count` is at least `minimum_evidence` (default 3).
- `confidence` is at least `minimum_confidence` (default 0.8).
- The id is not already selected.
- The added tokens fit after displacing an unprotected optional item whose score is strictly lower. Score is the neural score when present, otherwise the baseline score. The prediction's score is its confidence. An equal or higher optional item stays, and the trace reason is `expected benefit does not justify displacement`. The displaced id is recorded only when the add is committed.
- Latency is not measured here. The token budget and the score comparison are the cost check. A rejected prediction leaves any earlier optional rerank in place.

Every accept and reject is a trace row with `source: prediction` and a reason string.

## Calibration status

`predict()` confidence is a frequency, `count / matching rows`, except a graph edge is hardcoded to 1.0. That is not a fitted probability.

The earlier leave-one-out diagram in `VALIDATION-PREDICT.md` used minimum confidence 0.75 on the whole coding fixture. Only the bucket [0.9, 1.0] had any predictions (n=5), and all 5 were hits. The other buckets were empty. That diagram does not calibrate the 0.8 gate.

A separate count, `query_prior::measure_prediction_additions`, uses the recovery gate's existing 0.8 floor on the 12 unlocked coding-fixture questions. Locked families and the live 30-task set are excluded. Leave-one-out produced 5 memory predictions. All 5 were relevant. Four were already in the lexical top 3. One was at lexical rank 6. None were irrelevant.

Moving that one memory to position 3, which is the pre-specified displacement, changes mean reciprocal rank from 0.626 to 0.640 on those 12 questions. The floor was not lowered to find more additions.

Five hits at one threshold are not a reliability curve. The same frozen rule, trained only on the unlocked questions and scored on the three locked-family questions, emitted **0** predictions. Mean reciprocal rank stayed **0.714**. The within-family gain does not cross to a held-out family, because a prediction is only formed when the task name overlaps a training category. Callers still pass `calibrated_on_independent_validation: false`, and the gate rejects the proposal.

`query_prior::measure_benefit_gate` runs the same 12 unlocked questions through `select_context`. Lexical overlap is divided by that query's maximum so it is on the same 0–1 scale as confidence. The uncalibrated arm accepts **0** additions. Forcing the calibration flag, which this fold does not earn, still accepts **0**: all **5** memory predictions are rejected as `insufficient supporting evidence` because each has fewer than 3 supporting rows. The score comparison is not reached. Mean reciprocal rank stays **0.6257936507936509**. `minimum_evidence` stays 3. It was not lowered to recover the older 0.640 figure.
