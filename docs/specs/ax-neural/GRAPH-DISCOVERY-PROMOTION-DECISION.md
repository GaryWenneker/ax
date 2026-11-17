# Graph discovery promotion decision

The independent bar passed. Production retrieval stays the default. `rank::plan` does not call the walker, `SEMANTIC_WEIGHT` is unchanged, and `AX_MEMORY_BENCH_IDENTIFIER_AFFIX` stays unset. Spec approval: not obtained (autonomous run).

Activate the walker only as an explicit opt-in. This change does not put it on the production path, because the same objective leaves production retrieval unchanged.

## What was compared

The set is `benchmarks/memory/graph-discovery-independent.json`, protocol `graph-discovery-independent-1`, snapshot `85fa176a9b4b6ce5e33687bee268767d87731c60`, blake3 `5ff5a69f4aa4d926c7c59c764d222a0017ebac7bc4b637803688fe006e5f77f6`. It is not the frozen 62-task file. The corpus is crate files whose bytes match that snapshot, excluding `crates/ax-memory/`, `crates/ax-neural/`, dist, node_modules, the 13 diagnosis files, and the five frozen rename files. 55 tasks have gold and 8 are negative. 798 of 800 candidates matched a graph node. 855 links were taken from the code-graph tables.

Two runs of `independent_graph_discovery` produced the same quality figures. The numbers below are the second run, in `docs/specs/ax-neural/GRAPH-DISCOVERY-RESULTS.json`. Traversal time was 3.507936507936508 ms on both runs. End-to-end time moved from 68.111 ms to 68.429 ms. RSS growth was 10371072 bytes, then 10420224 bytes.

| | Production | Graph discovery |
|---|---:|---:|
| Candidate recall | 0.2727272727272727 | 0.4075757575757576 |
| MRR | 0.2727272727272727 | 0.33454545454545453 |
| NDCG@10 | 0.2727272727272727 | 0.35592036700485297 |
| Precision@3 | 0.0909090909090909 | 0.1333333333333333 |
| Recall@3 | 0.2727272727272727 | 0.33030303030303027 |
| Negative abstentions | 7 of 8 | 7 of 8 |
| Candidates added on negatives | 0 | 0 |
| Non-gold additions | 0 | 160 |
| Mean retrieval ms | 64.92063492063492 | 64.92063492063492 |
| Mean traversal ms | 0 | 3.507936507936508 |
| Mean end-to-end ms | 64.92063492063492 | 68.42857142857143 |

Kind MRR, discovery minus production: bug localization 0.25 on 4 tasks, cross-file calls 0.09166666666666666 on 20 tasks, indirect dependencies 0.03777777777777778 on 15 tasks, similar names 0 on 15 tasks, the one rename 0. Indirect Recall@3 stayed 0. Similar-name MRR stayed 1. The rename stayed 0, because a rename has no call link.

Production's 0.2727272727272727 is the 15 similar-name tasks. The other production kinds scored 0. The gain is those other kinds entering the pool.

## Bar

| Condition | Result |
|---|---|
| At least 20 gold tasks | 55 |
| Candidate recall up by at least 0.05 | 0.1348484848484849 |
| MRR up by at least 0.02 | 0.06181818181818183 |
| Recall@3 drop at most 0.01 | recall rose |
| Precision@3 does not drop | precision rose |
| NDCG@10 drop at most 0.01 | NDCG rose |
| No kind MRR drop above 0.05 | none dropped |
| Negative abstention does not fall, and negative additions stay 0 | 7 and 0 |
| Mean traversal at most 100 ms | 3.507936507936508 |
| End-to-end at most production plus 100 ms | plus 3.507936507936508 |
| RSS growth at most 64 MB | 10420224 bytes |

## Limits

160 of the added candidates were not gold, about 2.91 per gold task. Precision@3 still rose. Of 37 labeled cross-file gold pairs, 8 were present as graph links, so the walker cannot recover a call the index never recorded. The bug and rename slices are small. The budgets were not changed after this score.
