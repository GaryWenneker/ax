# Spec: call edges through Rust macro arguments, and history by commit subject

Status: draft, written 2026-11-27. Spec approval: not obtained (the owner answered "Fix"
to the question whether to fix the two known gaps; the spec itself was not reviewed).

## Problem

The agent end-to-end test (`tools/ax_agent_e2e.py`) records two known gaps.

1. A Rust call written inside a macro argument gets no call reference:
   `println!("{}", invoice_total(items))` leaves `print_invoice` out of the callers of
   `invoice_total`. tree-sitter does not parse macro arguments; they are a `token_tree`
   of raw tokens, and the extractor only looks for `call_expression` nodes. Callers,
   impact, paths and affected tests are incomplete for every call inside `println!`,
   `format!`, `assert!`, `assert_eq!`, `vec!`, `matches!` and the like.
2. `ax_history` / `ax history` with a topic ("invoice billing") searches turn memories only.
   A commit is listed only when the query is a path or a symbol name, although the docs
   say history lists "the turns and git commits that touched a file, a symbol, or a topic".

## Behavior

### M. Calls inside macro arguments (`crates/ax-extraction/src/languages/refs.rs`, `rust.rs`)

Inside the `token_tree` of a Rust `macro_invocation`, an `identifier` token directly
followed by a `token_tree` that opens with `(` is a call to that identifier.

```gherkin
Scenario: M1 a call in a println! argument
  Given fn print_invoice(items) { println!("{}", invoice_total(items)); }
  Then  there is a Calls reference "invoice_total" from print_invoice
  And   when invoice_total is defined in the same file, a Calls edge print_invoice -> invoice_total

Scenario: M2 calls nested in macros and in call arguments
  Given assert_eq!(total(a), compute(vec![one()]))
  Then  there are Calls references "total", "compute" and "one"

Scenario: M3 a method or path call
  Given format!("{}", self.name()) and format!("{}", billing::rate())
  Then  there are Calls references "name" and "rate"

Scenario: M4 what is not a call
  Given println!("{}", count) and vec![a, b] and matches!(x, Some(y) if y > 1)
  Then  "count", "a", "b", "x", "y" get no Calls reference
  And   a macro name ("println", "vec", "matches") gets no Calls reference from this rule
  And   an identifier after the keyword `fn` gets none (an item defined inside a macro)

Scenario: M5 the reference is attributed to the enclosing function, at the call's line
```

The existing filters for noise and external callees apply unchanged. `macro_rules!`
definitions are a different node kind and are not walked.

Because extractor output changes, `EXTRACTION_VERSION` goes from 7 to 8.

### H. History by topic includes commits (`crates/ax-memory/src/history.rs`)

```gherkin
Scenario: H5 a topic query lists a commit whose message has every significant word
  Given a commit "Add review comment language setting"
  When  history is asked for "comment language"
  Then  the commit is listed, kind "commit", with its files
  And   matching ignores case

Scenario: H6 a commit that lacks one of the words is not listed
  When  history is asked for "comment dropdown"
  Then  that commit is not listed

Scenario: H7 `since` and `limit` apply to topic commits as they do to path commits

Scenario: H8 a query with no significant word (short or stop words only) lists no commit
  And   a query outside a git repository lists turns only, as before
```

"Significant word" is the existing rule: four characters or more and not a stop word.
Turn matching is unchanged. Path and symbol queries are unchanged.

## Non-goals

- N1 No macro expansion; calls a macro generates itself are not found.
- N2 No change for languages other than Rust.
- N3 No ranking of commits by relevance; entries stay newest first.
- N4 No existing test weakened.

## Verification

- Unit tests M1–M5 in `refs.rs`; integration tests H5–H8 in `crates/ax-memory/tests/history.rs`.
- The two `known_gap` entries in `tools/ax_agent_e2e.py` are removed; both checks must pass
  on the full and the slim build.
- Manual mutants for M and H with a no-op control.
