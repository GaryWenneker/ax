# Code-aware promotion decision

Do not promote code-aware retrieval. Keep production retrieval.

The comparison is the frozen snapshot `85fa176a9b4b6ce5e33687bee268767d87731c60` in `benchmarks/memory/code-retrieval-frozen.json`. It is not the 15 diagnosed doc-comment rows and not the 30-task held-out run. Sixty-two tasks have a gold id and eight are negative queries. Production and the affix pass both used `recall_for_prompt` with limit 10. The affix flag was set only for its pass and then removed. The graph arm reorders that production list with exact symbol identity and extracted same-file edges.

| Arm | MRR | NDCG@10 | Precision@3 | Recall@3 | Abstained | Latency |
|---|---:|---:|---:|---:|---:|---:|
| Production | 0.447599846390169 | 0.4753426947889109 | 0.15591397849462366 | 0.46774193548387094 | 6 of 8 | 4511 ms |
| Affix | 0.447599846390169 | 0.4753426947889109 | 0.15591397849462366 | 0.46774193548387094 | 6 of 8 | 4879 ms |
| Graph hybrid | 0.4395353302611367 | 0.4693899488787731 | 0.15591397849462366 | 0.46774193548387094 | 6 of 8 | 162 ms |

The graph arm's MRR is 0.0080645161290323 below production. The promotion bar required a gain of at least 0.02. Recall@3 and abstention are tied. The only kind that moved is similar-but-unrelated identifiers: production MRR 1.0, graph hybrid 0.975. Cross-module calls and the multi-file trace stayed at MRR 0 for every arm. Those labels are bare calls in another file, and the ranker was not given cross-file edges, because the current extractor does not emit them.

The affix pass matched production on every figure above. `AX_MEMORY_BENCH_IDENTIFIER_AFFIX` stays unset. `SEMANTIC_WEIGHT` stays 0. The coverage floor, `ax_text_rank`, and the held-out benchmark were not changed.

Process RSS moved from 16728064 bytes to 23920640 bytes during the shared run. That is not a per-arm memory overhead. The 162 ms graph figure is the reorder after production retrieval, not a second retrieval.

The ranker was not edited after these numbers.
