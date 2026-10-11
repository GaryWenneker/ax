# Evidence: Grep alternation is guarded like a symbol name

spec approval: not obtained (the user asked to stop the grepping in the same message, 2026-10-03)

Tier: 2. The hook already existed. This change closes the hole where `name|name` was treated as "not a symbol".

## Behavior

| Spec | Check |
|---|---|
| A Grep that joins known symbol names with `\|` is denied once | `alternation_of_known_symbols_is_denied_once` |
| The deny names each graph hit and `ax_search` | same test |
| A non-identifier piece such as `live-new` is not listed | same test |
| The same pattern a second time is allowed | same test |
| An alternation of unknown names is allowed | same test |
| One symbol name is still denied once | `symbol_search_is_denied_once_with_graph_hits` |
| Phrases and regex still pass | `symbol_name_rejects_text_and_regex`, `non_symbol_or_unknown_or_out_of_index_searches_pass` |

## Gauntlet

Command: `cargo test -p ax-cli --bin ax commands::read_guard::tests -- --test-threads=8`

Result: 30 passed, 0 failed, 0.03s (after the last edit of `read_guard.rs`).

Mutation: `python3 scripts/read-guard-mutants.py`

Result: 15/16 killed, sources restored (sha256 verified). The new mutant "alternation is not split" was killed by `symbol_candidates_split_alternation_and_skip_non_identifiers` and `alternation_of_known_symbols_is_denied_once`.

NOT APPLIED (0 matches): "every hook treated as ours" in `crates/ax-installer/src/hooks.rs`. That file already had uncommitted edits from other work. This change does not touch it. The mutant string is absent in that working copy, so the script cannot apply it.

## Review

One round. The deny lists only names that exist in the graph, so a CSS class in the same pattern does not become a false symbol. Unknown names stay unguarded. Fail-open paths are unchanged.

## Known limits

A search that is only a phrase, a CSS class, or a regex is still allowed. Those strings are not symbol names in the graph. The hook binary on PATH has to be the build that contains this change; an older `ax read-guard` still lets alternation through.
