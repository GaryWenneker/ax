# Learning quality analysis

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/LEARNING-QUALITY-ANALYSIS.md`  
**Date:** 2026-11-16  
**Evidence:** `docs/specs/ax-neural/learn-validate-results.json` and `crates/ax-neural/src/learn_validate.rs`

The live held-out agent tasks were not used as training rows.

## Measured gap

On the locked fixture eval (4 questions, 3 scored):

| | P@3 | R@3 | MRR |
|---|---|---|---|
| Before (query overlap) | 0.222 | 0.667 | 0.714 |
| After ranking-prior | 0.333 | 1.000 | 0.500 |
| Delta | +0.111 | +0.333 | −0.214 |

`promoted` stayed false. Retraining with the same seed reproduced the weights. `validationScored` inside the trainer was 0 because the validation split was empty after locked questions were held out. Sign agreement was not measured.

## Why recall can rise while MRR falls

`train_ranking_prior` adds `outcome_weight` once per context, skill, and rule id. Success is +1, partial is +0.5, failure is −0.5. The result is a single frequency table.

`run_learn_validate` then calls `rank_from_weights` **once** and applies that same id list to every eval question. The rank is not conditioned on the query.

The before-rank is query overlap, so the first hit can be the memory that shares the prompt's words. The after-rank puts the globally most successful ids first. A frequent success id that is only weakly related to the current question moves to rank 1 and pushes the true top hit down. Recall@3 still rises when the relevant id appears somewhere in the top three of that global list. MRR falls because the first position got worse. That is a position error, not evidence that the relevant memory was lost.

## What the trainer does not do

| Method | Present |
|---|---|
| Pairwise ranking loss | evaluated once, not promoted |
| Hard-negative mining | evaluated as memories that share a prompt token |
| Query-conditioned features | no |
| Per-project isolation of feedback rows | `project_scoped_weights` keeps a separate key per project and skips a row with no project id. `train_ranking_prior` does not call it, so the fixture MRR figures above are unchanged. |
| Calibration of the prior as a probability | no; `evaluate_prior` only checks the sign of a summed weight |
| Training on the live 30-task agent set | no |

`evaluate_prior` skips rows whose outcome weight is 0 and skips rows with no known ids. On the thin fixture split it scored nothing, so it cannot gate promotion.

## Query-conditioned candidate

`query_prior::measure` trains a separate candidate on the same unlocked coding-fixture rows. For each training prompt token it adds 1 to every relevant memory and subtracts 1 from every other insertable memory whose text contains that token. At query time the learned score orders candidates, and lexical overlap breaks ties. Locked families stay out of training. `promoted` is false. The live 30-task set was not read.

On the locked fold, 3 scored questions:

| Ranker | P@3 | R@3 | MRR | NDCG@5 |
|---|---|---|---|---|
| Lexical baseline | 0.222 | 0.667 | 0.714 | 0.667 |
| Global frequency prior | 0.333 | 1.000 | 0.500 | 0.631 |
| Query-conditioned prior | 0.222 | 0.667 | 0.700 | 0.667 |

The query-conditioned prior avoids the frequency prior's MRR drop and keeps the baseline precision, recall, and NDCG. It does not beat the lexical baseline: MRR falls from 0.714 to 0.700 because the paraphrase question's relevant memory moves from rank 7 to rank 10. That is not a production improvement.

The same weights were then scored on the three unlocked questions that `split_dataset` held out of training. That split does not overlap the locked fold or the live 30 tasks.

| Ranker | P@3 | R@3 | MRR | NDCG@5 |
|---|---|---|---|---|
| Lexical baseline | 0.333 | 1.000 | 1.000 | 1.000 |
| Learned score as the primary key | 0.222 | 0.667 | 0.750 | 0.810 |
| Lexical primary, learned score as a tie-break | 0.333 | 1.000 | 1.000 | 1.000 |

Letting the learned score outrank lexical overlap drops the `as-of` memory out of the top three. Keeping lexical overlap as the primary key matches the baseline metrics on this split. The tie-break changed the third place on `as-of` and did not raise MRR. Neither result is a promotion sample.

## Pairwise hinge

`query_prior::measure_pairwise` trains one pass on the 9 unlocked questions left after `split_dataset` with ratio 0.25 and seed 7. A hard negative is an insertable memory that is not relevant and shares a prompt token. The update runs only when the relevant score is not already strictly higher. Margin is 0. There were 4 updates. Locked families and the live 30-task set are not in the training rows, and the update rule was not changed after these scores.

| Fold | Ranker | P@3 | R@3 | MRR | NDCG@5 |
|---|---|---|---|---|---|
| Dev, 3 questions | Lexical baseline | 0.333 | 1.000 | 1.000 | 1.000 |
| Dev | Pairwise as the primary key | 0.333 | 1.000 | 0.833 | 0.877 |
| Dev | Lexical primary, pairwise tie-break | 0.333 | 1.000 | 1.000 | 1.000 |
| Locked, 3 scored | Lexical baseline | 0.222 | 0.667 | 0.714 | 0.667 |
| Locked | Pairwise as the primary key | 0.222 | 0.667 | 0.692 | 0.667 |
| Locked | Lexical primary, pairwise tie-break | 0.222 | 0.667 | 0.697 | 0.667 |

Precision and recall stay put. MRR falls on the locked fold for both the primary key and the tie-break. That is the same position error as the frequency prior: the first hit moves down. The rule was not edited to recover the baseline.

A second, narrower rule uses only the lexical top hit as the hard negative, and only when that hit is not relevant. On the same 9 training questions there are 5 such misses and 5 updates. Dev MRR stays 1.000 for the lexical baseline, the learned primary key, and the lexical tie-break. Locked MRR stays 0.714 for all three. The rule ties lexical ranking and does not raise it. It was not edited after that result.

## Decision

Do not promote the frequency prior, the query-conditioned prior, or the pairwise hinge. The live held-out agent tasks stay out of training.
