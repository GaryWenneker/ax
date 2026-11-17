# Code-aware retrieval design

Experimental ordering lives in `crates/ax-neural/src/code_aware.rs`. `rank::plan` does not call it. The default config has `enabled: false`. `AX_MEMORY_BENCH_IDENTIFIER_AFFIX` stays unset. `SEMANTIC_WEIGHT` stays 0. The coverage floor and `ax_text_rank` are unchanged.

Spec approval: not obtained (autonomous run). The 15 diagnosed doc-comment rows were not used to set a threshold, and they are not a validation set.

## What counts as verified

A candidate moves only for one of these:

1. **Exact symbol identity.** A query token and the candidate name become the same string after lowercasing and removing every character that is not an ASCII letter or digit. `parseXml` and `parse_xml` match. `policy` does not match `policy_rows`. `publishers` does not match `publish_for_test`. `policySync` does not match `policy_sync_enabled`. A shared prefix or suffix is not identity.
2. **Extracted graph edge.** The edge kind is `calls`, `references`, or `documents`, and the confidence is `extracted` or `exact`. One end is an exact identity hit. Tree-sitter and SCIP edges are the extracted class. An inferred edge does not move a candidate.
3. **File ownership is not a promotion.** The exact hit already names its file. Another function in that file stays in baseline order unless it has its own identity hit or an extracted edge.

Definition-reference and call edges are the same rule: an extracted edge of that kind. Unresolved cross-file names are not edges, so they do not move a candidate. The same-file call extractor in `ax-extraction` only emits a `Calls` edge when the reference name equals the local function name.

## Order and budget

When the selector is disabled, the candidate ids stay in the order they were given.

When it is enabled:

- A candidate whose project id is set and differs from the requested project is omitted.
- Mandatory candidates stay in their original order, ahead of optional ones.
- Exact identity hits follow, in their original order.
- Extracted-edge neighbors follow, in their original order.
- Every other allowed candidate keeps its original order.
- A positive limit keeps every mandatory candidate, then fills the rest of the limit. Ids past that cut are recorded as `displaced`. The trace does not claim those displaced ids were unused by an agent.

Neural embedding scores are not an input. Adding them would be a separate flag. This selector does not replace production retrieval.

## What this is not

It is not a new substring rule. It is not wired into preflight, memory recall, or the public ranker. A gain on the 15 diagnosed queries would not be independent evidence.

## Frozen comparison

The comparison file is `benchmarks/memory/code-retrieval-frozen.json`, built by `benchmarks/memory/freeze-code-retrieval.py` from commit `85fa176a9b4b6ce5e33687bee268767d87731c60`. Doc, call, and similar-name tasks come from source trees outside `crates/`. Rename tasks are signature-preserving hunks. The 13 diagnosed doc-comment files, `crates/ax-memory`, and `crates/ax-neural` are excluded. Same-file call edges are the only edges passed to the ranker. Cross-file calls are labels, not edges.

Candidates are ordered by stable `path:line` id. Production and affix both use `recall_for_prompt` with limit 10. Affix is set only inside that measurement and removed afterwards. The graph arm is `hybrid_rank`: exact symbols and extracted edges first, then the production list, cut at 10. Neural scores are not an input.

Promotion bar, fixed before the measurement: promote only when the graph arm's MRR is at least 0.02 above production, its Recall@3 is not more than 0.01 below production, its negative-query abstention rate is at least production's, and no task kind loses more than 0.05 MRR. Otherwise production stays.
