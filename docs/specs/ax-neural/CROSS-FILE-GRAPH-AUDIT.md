# Cross-file graph audit

The live project index was read on 2026-11-17. Production retrieval, `SEMANTIC_WEIGHT`, and `AX_MEMORY_BENCH_IDENTIFIER_AFFIX` were not changed. The frozen 62-task benchmark was not used to set a threshold.

## What the index contains

| Edge kind | Rows |
|---|---:|
| calls | 39449 |
| contains | 14203 |
| documents | 3265 |
| references | 424 |
| covers | 39 |

There are no `imports`, `extends`, or `implements` edges. Unresolved references: 76343, of which 71583 are calls, 4637 are imports, and 123 are function refs.

Function and method nodes: 10360, with 8656 distinct names.

## Call edges

| Span | Provenance | Confidence | Rows |
|---|---|---|---:|
| same file | tree-sitter | extracted | 15064 |
| same file | heuristic | inferred | 1314 |
| cross file | heuristic | inferred | 23071 |

Same-file tree-sitter edges are the only extracted call edges. `emit_same_file_call_edges` writes those when the callee name equals a function or method in the same file, and it leaves every other call in `unresolved_refs`.

`ReferenceResolver::resolve_all` then tries the remainder. A bare call that is not in the import map goes to `NameMatcher::resolve_ref`, which loads every node with that name, prefers the same file, and otherwise commits `candidates[0]` as a heuristic inferred edge. It does not require the name to be unique. Dotted `this.` and `().` calls are deferred and only resolve inside one file.

Of the 23071 cross-file call edges:

| Target name among functions and methods | Rows |
|---|---:|
| unique | 7630 |
| ambiguous (more than one function or method) | 12254 |
| target is not a function or method | 3187 |

A unique name is the only cross-file call set that points at a single function. The ambiguous set is a first-match guess. Neither set is extracted.

## Unresolved bare calls

71557 unresolved calls have no dot and no `(`. Against current function and method nodes:

| Bucket | Refs |
|---|---:|
| no function or method with that name | 68559 |
| exactly one function, in another file | 2286 |
| more than one function | 693 |
| exactly one function, in the same file | 19 |

The 68559 refs have nothing to traverse to. A later walk of existing edges cannot create those targets. The 2286 unique other-file refs are the ones a unique-name resolution could still commit. They were not committed: the nodes exist and the refs are still unresolved. The 19 same-file leftovers are an indexer miss.

Imports are the missing relationship type. 4637 import refs remain, and the edge table has zero import edges.

## The frozen cross-file examples

`capturePage`, `pagesToRender`, `clipBox`, `ogImagePath`, `readShareMeta`, `pathnameFromDistHtml`, `metaContent`, `detectInstallOS`, and `mountMinimalDownload` are function nodes. There is no call edge from `pagesToRender`, `capturePage`, or `mountMinimalDownload` to those callees. The calls are still unresolved refs, including `site/scripts/render-og.mjs` to `clipBox` at line 119. `detectInstallOS` exists twice, in the `.ts` and `.js` twins, so a first-match edge would be ambiguous. `clipBox` exists once, in another file, and the edge is still missing.

## Where retrieval stands

`recall_for_prompt` does not read this edge table. `code_aware::select` only reorders candidates it is given, and the frozen comparison did not give it cross-file edges. So the 7630 unique cross-file edges are in the index and unused by retrieval. The frozen gold calls are not in that set: the edge was never written.

Traversal of the current call edges does not fix the frozen misses, and it does not recover the 68559 calls whose names are not functions in this index.
