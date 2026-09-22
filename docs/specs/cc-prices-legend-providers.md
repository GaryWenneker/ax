# Command Center Prices — legend, Cursor, charts

Tier 2. Isolation: none. Spec approval: not obtained (autonomous run).

## Behaviors

### P1 — Chart legend uses distinct colors
Input and output series use fixed chart colors (`#5eb8ff` / `#e0a030`), not `--accent`/`--ok`, so Mono still distinguishes them. Legend swatches match the SVG strokes.

### P2 — Chart draws with one or more points
A single daily snapshot still renders (marker + optional line). Empty history shows the existing hint.

### P3 — Context length is not a decimal
`131072` formats as `131,072` (`en-US`), never `131.072`.

### P4 — Provider cards hug content
Provider sections do not force `min-height: 240px`. Grid gap is ≤ `0.45rem`.

### P5 — Cursor is present
When OpenRouter has no `cursor/` models, curated Cursor rows (Composer 2.5 / Fast from built-in Savings rates) appear under provider `cursor`.

### P6 — Provider jump legend
A chip legend lists every visible provider (including Cursor) and jumps to that section.

### P7 — Display names
Known slugs map to readable labels (`x-ai` → `xAI`, `mistralai` → `Mistral`, `aion-labs` → `Aion Labs`).

### C1 — Default model is not free
`pickDefaultModelId` skips `:free` and $0/$0 rows when any paid row exists.

### C2 — Zero-price chart does not fake a $0.01 axis
`chartDomain([0,0])` is `{min:0,max:0}`. RateChart shows a $0 tick, both series (offset so they are both visible), and a free-model hint.

## Invariants
- Existing OpenRouter rows stay; curated rows do not replace matching `model_id`s.
- English UI strings only.

## Files
- `crates/ax-web/web-ui/src/lib/pricesUi.ts`
- `crates/ax-web/web-ui/src/lib/pricesUi.test.ts`
- `Prices.tsx`, `index.css`
- `site/src/content/docs/guides/command-center.md`

## Deps
None.
