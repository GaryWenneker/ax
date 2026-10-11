# SPEC — Logging detail window: step badges

Status: draft, awaiting approval · Tier 2 · Branch `calm-list-style`

Request (2026-09-28 15:01): screenshot of the detail window; the user chose "Step badges: no overlap with the
text, same kind colors as the list".

## Behaviors

1. In the Steps list of the Logging detail window, a step badge never overlaps its text: the badge's right edge
   is left of the time/message text's left edge, for every step (IN, ENR, INT, OUT, PREV, MEM, …).
2. All step badges have the same width (one column), so the texts line up.
3. Each step badge uses the same border color as that kind's badge in the list (ENR purple `#b58cff`,
   IN blue `#4fa8ff`, OUT green `#3ee4b2`, PREV light green `#8ae8c8`, INT grey, MEM pink, …), with light
   grey text.

## Must NOT change

The rest of the detail window (fields, message, raw line bar, timestamps, fonts), and all existing tests.

## Tests (RED first)

- e2e `e2e/calm-lists.spec.ts`: open a Logging entry; for every step, badge right ≤ body left; all badge widths
  are equal; the ENR step badge border equals the ENR list badge border.
- Mutant in `scripts/calm-lists-mutants.sh`: step badge colors rule removed.

No new dependencies.
