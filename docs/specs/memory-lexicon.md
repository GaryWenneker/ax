# Bilingual software lexicon

Status: Phase 6 lexicon slice. Spec approval: not a separate review. The plan was started with "go".

## Scenario

1. Expanding `fout` returns a set that includes `bug` and `error`. Expanding an empty token returns nothing. Expanding an unknown token returns only that token lowercased.

## Known limits

- Wiring expansion into recall, PMI mining, and Snowball stemming are not in this slice.
