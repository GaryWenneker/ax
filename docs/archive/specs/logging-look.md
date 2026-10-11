# SPEC — Logging look everywhere, readable log, colored kinds, prompt in vs returned

Status: draft, awaiting approval · Tier 2 · Branch: `calm-list-style` (current work branch)

## Request (2026-09-28 14:03, translated)

"The background must be more in line with the Logging background, on all pages. The Logging font must be
more readable than the current monospace. Timestamps may keep their font but show no milliseconds.
Logging badges get specific colors so they stand out. Very important: I must see visually, by color,
what goes in as a prompt and what is returned to the agent to continue with."

## Behaviors

1. **One panel background.** The main panel on every Command Center page (Stats, Nodes, Search, Rules,
   Skills, Memory, Settings, …: `.settings-card` and the page panels) uses the Logging panel look:
   flat `rgba(0,0,0,0.28)` over the page background, 1px quiet border (`#3c3c3c`-ish), no gradient,
   no bright green border. Logging itself is unchanged.
2. **Readable log text.** Logging row titles and subtitles use the UI sans font (same as Rules/Skills),
   not Cascadia/monospace. Tool names and ids inside badges stay as they are.
3. **Timestamps.** The time column keeps its monospace font and shows `YYYY-MM-DD HH:MM:SS`
   — `2026-09-28 14:03:21.005` becomes `2026-09-28 14:03:21`. Tooltip keeps the full value.
4. **Colored kind badges.** Each kind has its own color (outline + icon; text stays readable, ≥4.5:1):
   | Kind | Color |
   |---|---|
   | IN (prompt to ax) | blue `#4fa8ff` |
   | OUT (returned to agent) | green `#3ee4b2` |
   | PREV (preview of what is returned) | green, lighter `#8ae8c8` |
   | INT (internal) | grey `#9d9d9d` |
   | ENR (enrich) | purple `#b58cff` |
   | MEM (memory) | pink `#f28bc0` |
   | POL (policy) | amber `#e0b341` |
   | CLI | orange `#f0955a` |
   | SHIP | cyan `#4fd6e6` |
   | WS (workspace) | slate `#8fa6c4` |
   | error | red (unchanged) |
   The legend chips at the top use the same colors.
5. **Prompt in vs returned, at a glance.**
   - IN rows: blue timeline dot and a thin blue left accent on the row; subtitle starts with "Prompt in".
   - OUT and PREV rows: green dot and green accent; subtitle starts with "Returned to agent".
   - All other rows: grey dot, no accent — they are ax's own work, quieter.
   - Legend line above the list: "● Prompt in  ● Returned to agent  ● Internal".

## Must NOT change

- Calm-list structure (calm-lists rule), outline badges with square corners (outline-badges rule).
- Logging data, filters, selection, inspector, existing e2e (`logging-text.spec.ts`), 133 unit tests.

## Tests (RED first)

- unit `src/lib/calmRows.test.ts`: `formatTraceTime('2026-09-28 14:03:21.005') === '2026-09-28 14:03:21'`;
  `traceDirection('in'|'out'|'preview'|'enrich')` → `in`/`out`/`out`/`internal`.
- e2e `e2e/calm-lists.spec.ts` (Logging): meta has no `.\d{3}` suffix; title font-family is not monospace;
  IN badge and OUT badge have different border colors; IN row and OUT row nodes differ;
  `.settings-card` background on Skills equals Logging panel background.
- Mutants: added to `scripts/calm-lists-mutants.sh` (ms kept, direction swapped, uniform badge color).

## Setup plan

No new dependencies. Files: this spec, EVIDENCE at `docs/specs/logging-look-EVIDENCE.md`,
edits in `McpTraceLive.tsx`, `lib/calmRows.ts`, `index.css`, e2e + mutants script.
