# ONNX fusion root cause

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/ONNX-FUSION-ROOT-CAUSE.md`  
**Date:** 2026-11-16

Arm C in the live real-world benchmark is blocked. This note records why. It does not raise `SEMANTIC_WEIGHT` and it does not turn ONNX on in the default build.

## Where the path stops

1. **The default binary does not contain the ONNX module.** `crates/ax-neural/src/lib.rs` compiles `onnx_embed` only with `feature = "neural-ort"`. `runtime_built()` is true only for `neural-runtime`, which is the pure-Rust CPU backend, not ONNX. The published `ax` binary is built without `neural-ort`.

2. **Semantic fusion is a no-op.** `SEMANTIC_WEIGHT` is `0.0`. `blend_semantic` returns the locked fusion score unchanged: `fused + SEMANTIC_WEIGHT * semantic`. The dispatch test `semantic_blend_does_not_move_the_locked_fusion_score` locks that behavior.

3. **Context ranking does not read ONNX vectors.** Production context `rank` stays lexical overlap. Memory embeddings stay hash-v1. `ax neural search` uses the blake3 runtime when the lexical challenger is admitted.

4. **The live agent harness never starts arm C.** `scripts/neural-realworld-agent.py` only writes configs for A, B, D, and E. There is no ONNX flag in that map. A and B write the same prediction and learning flags.

## What the validation feature build already showed

`docs/specs/ax-neural/VALIDATION-ONNX-PATH.md` (`neural-ort`, not the default binary):

| Stage | Status |
|---|---|
| Model load, inference, persistence, query embedding, cosine similarity, cosine retrieval | measured |
| Ranking fusion into production context | **inactive_in_production** |
| `blend_semantic` | unwired, weight 0 |

Locked-family scores from that run: lexical MRR 1.000, ONNX MRR 0.778, RRF of hash+lexical+ONNX MRR 0.833. ONNX did not beat the lexical baseline on MRR. That is why fusion was left inactive.

On the same bake-off file, replacing hash with ONNX on the 15 non-locked questions raises Recall@3 from 0.667 to 0.833 and Precision@3 from 0.222 to 0.278, and lowers MRR from 0.710 to 0.691. The lexical ranker that reaches MRR 1.000 on the locked families uses a hand-written synonym table in `lexical::expand`. That table is not an independent semantic model.

A separate rule keeps every positive hash cosine in its original order and lets another ranker order only the zero-score tail. On 12 non-locked questions with a relevant memory, hash already has a positive score for fewer than 10 memories on 6 questions, so the tail has room. Filling that tail from the synonym ranker changes 3 lists. MRR stays 0.7098214285714285 and Recall@3 stays 0.667. The same frozen rule, with ONNX cosine order in the tail instead of the synonym list, changes 6 of those 12 lists. MRR falls from 0.7098214285714285 to 0.7083333333333334. Recall@3 stays 0.667. The rule was not edited after either result, and neither fill is promoted.

## What was not done

No production integration was added in this change. Repairing the missing link would mean a feature-gated path that still defaults to weight 0, then a fresh comparison on validation data that is not the live 30-task agent set. That comparison is not a promotion.
