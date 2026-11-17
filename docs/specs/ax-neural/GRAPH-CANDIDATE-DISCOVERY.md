# Graph candidate discovery

Experimental path. `rank::plan` does not call it. Production retrieval, `SEMANTIC_WEIGHT`, and `AX_MEMORY_BENCH_IDENTIFIER_AFFIX` stay as they are. Spec approval: not obtained (autonomous run).

The frozen comparison only reordered candidates it already had. This path adds a definition that was not in that list when a verified link connects it to a resolved seed.

## What counts as verified

A link is followed only when its kind is `calls`, `references`, or `documents`, and:

- `proof` is `extracted`, or
- `proof` is `unique-cross-file` and both ends have a non-empty file path and those paths differ.

`inferred`, `ambiguous`, and a unique-name link inside one file are not followed. The caller may pass a unique-name link only when the target name belongs to one function. This module does not look that up, and it does not read `ax.db`.

## Steps

1. Start from the baseline candidate ids.
2. Resolve each id to a graph node. The same id wins. Otherwise the baseline name must match exactly one in-project node after `symbol_key`. Zero matches and two matches stay unresolved.
3. Walk outgoing verified links from those seeds. Seeds are not re-added.
4. Record each new node with its proof, kind, the node it was reached from, and the depth.
5. Skip a node already recorded, a missing node, and a node whose `project_id` is set and differs from the requested project.
6. Deduplicate against the baseline and against earlier discoveries.
7. Rank with `hybrid_rank` on the pool that now contains the new nodes. Keep that function's exact-symbol and extracted-edge prefix. Place new discoveries next, then the rest of the baseline. A positive limit truncates the tail and records `displaced:<id>`.

Disabled config returns the baseline ids and the trace `baseline`. It does not walk.

## Bounds

These values are the default budget. They were chosen before diagnosing the frozen tasks and are not revised from those tasks.

| Bound | Default | Stop |
|---|---:|---|
| depth | 2 | a seed is depth 0; a direct neighbor is depth 1 |
| fan-out | 8 | per node, after sorting extracted links before unique-cross-file links and then by target id |
| added candidates | 16 | new nodes only, seeds excluded |
| time | 50 ms | checked before expanding a node and before each link |

A link that points at an existing node consumes fan-out, including a rejected project and a node already seen. Fan-out overflow records `stopped:fan-out:<id>`. The candidate cap records `stopped:candidates`. The clock records `stopped:time`.

## Trace

- `seed:<id>:id` or `seed:<id>:name`
- `unresolved:<id>`
- `rejected:<id>`
- `discovered:<id>:<kind>:<proof>:via=<source>:depth=<n>`
- `duplicate:<id>` when the node is already in the ranked list
- `displaced:<id>` when the limit drops it
- `baseline` when the path is off

## What this does not do

It does not treat a shared substring as a relationship. It does not commit the 12,254 ambiguous cross-file edges. It does not invent a target for a call whose name is not a node. It does not change `select` or `hybrid_rank`.

## Frozen-task diagnosis

The 62-task file was not used to change the budget or the walker. `expand_with_graph` has no production caller and was not run inside `recall_for_prompt`. The earlier frozen run already left these gold ids out of the production list, so they did not survive ranking or the context budget. What follows is an index reading with the same acceptance rule as the walker: an extracted call edge, or a bare unresolved call whose name belongs to one function in another file. Depth 2, fan-out 8, and 16 added candidates. That reading is not a score.

| Task | Gold | In the graph | Path from the seed | Step that failed | Discovered by the walker | Survived ranking |
|---|---|---|---|---|---|---|
| `capturePage` | `clipBox` | one function, `site/src/lib/og-card.mjs:117` | no call edge; one unresolved ref at `render-og.mjs:119` | resolution never wrote the edge | no | no |
| `pagesToRender` | `ogImagePath`, `readShareMeta`, `pathnameFromDistHtml` | one function each, all in `og-card.mjs` | no call edge to them; each name is an unresolved ref. The only extracted call from the seed is same-file `walkHtml` | resolution never wrote the cross-file edges | no | no |
| `mountMinimalDownload` | `detectInstallOS` | two functions, the `.ts` and `.js` twins | unresolved ref at `minimal-download.ts:95`. The extracted calls stay in the same file (`openPanel`, `copyCommand`, `render`). One inferred cross-file edge points at `addEventListener` in `greenVesselIntro.test.ts` | the callee name is not unique, so the unique-name rule has no link to follow | no | no |
| `pagesToRender` multi-file | the three names above, plus `metaContent` | `metaContent` is one function in `og-card.mjs` | `readShareMeta` has an extracted call to `metaContent`. The seed has no edge to `readShareMeta` | the first hop was never resolved, so the second hop is unreachable | no | no |

If those unresolved unique names were passed in as `unique-cross-file` links, the same budget would reach `clipBox`, `ogImagePath`, `readShareMeta`, and `pathnameFromDistHtml` in one hop, and `metaContent` in a second extracted hop. `detectInstallOS` would still be excluded. That pass was not turned into a code change.

## Independent comparison protocol

Written before the run. The depth, fan-out, added-candidate, and time budgets above are not revised after the score. This file's frozen tasks are not inputs.

The corpus is crate source whose bytes match snapshot `85fa176a9b4b6ce5e33687bee268767d87731c60`. It excludes `crates/ax-memory/`, `crates/ax-neural/`, `node_modules`, `dist`, the 13 diagnosis files, and the five rename files already used in the frozen set. History is the first 400 commits at that snapshot.

Labels come from the file text and from git hunks. A cross-file task is a bare call to a function name that exists once, in another file. An indirect task is a second hop from that callee to a function that is not a direct callee and that lives in another file. A bug task is a commit subject that says fix or bug, adds one function that still exists, and does not contain that function's name. A rename is a signature-preserving hunk whose new name still exists and whose pair is not one of the frozen five. A similar-name task queries one function and does not list the other function that shares a prefix of at least four characters and has no call between them. Negative queries share no identifier with a candidate name. Caps, in id order, at most two call tasks per source file: 20 cross-file, 15 indirect, 12 bug, 8 rename, 15 similar, 8 negative. The candidate list keeps every task endpoint, then other functions from those files, and stops at 800.

Links given to the walker come from the code-graph tables, not from the label calls. A pair is an extracted `calls` edge, an inferred `calls` edge whose target name is a single function in another file, or an unresolved bare call whose name is a single function in another file. Both ends must be corpus candidates. An extracted link replaces a weaker link for the same pair.

The context limit is 10. Candidate recall counts gold in the pool before that cut. Ranked Recall@3, Precision@3, MRR, and NDCG@10 use the cut list. Traversal time is the expand step. Retrieval time is `recall_for_prompt`. End-to-end time is the sum.

Activation needs every condition. Otherwise the feature stays disabled.

- At least 20 tasks with gold.
- Candidate recall improves by at least 0.05.
- MRR improves by at least 0.02.
- Recall@3 drops by at most 0.01.
- Precision@3 does not drop.
- NDCG@10 drops by at most 0.01.
- No kind's MRR drops by more than 0.05.
- Discovery abstains on at least as many negative queries as production, and it adds zero candidates on negative queries.
- Mean traversal time is at most 100 ms per query.
- Mean end-to-end time is at most the production mean plus 100 ms.
- Peak RSS growth during the run is at most 64 MB.
