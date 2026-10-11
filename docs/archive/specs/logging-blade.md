# SPEC — Logging detail opens in a blade, like Memory

Status: draft, awaiting approval · Tier 2 · Branch `calm-list-style`
Supersedes `docs/specs/inspector-step-badges.md` (its behaviors are covered by 4 below).

Request (2026-09-28 15:03, translated): "When I click an item in Logging I get a popup that is also ugly, as you
can see with those badges. I want, the same way as in Memory, a beautiful blade."

## Behaviors

1. **Blade, not popup.** Clicking a Logging row opens a side blade on the right, the same component look as the
   Memory blade (`aside.memory-blade`: same width `min(480px, 100%)`, same slide-in, panel background, left border
   and shadow). No centered sheet, no dark overlay; the list stays visible and scrollable next to it.
2. **Header like Memory.** Kind icon + tool name (or "Log event") as the title; on the right the previous/next
   buttons and close. The kind badge keeps its kind color.
3. **Details block like Memory** (`detail-kv` key/value rows): Time (no milliseconds), Kind, Direction
   ("Prompt in" / "Returned to agent" / "Internal", with its color dot), Tool, Steps (count, when > 1).
4. **Steps as a calm list.** The call's steps are rows of the shared calm list inside the blade: timeline dot in
   the direction color, kind badge in its kind color, message as the title, time (no ms) as meta. Badges never
   overlap the text. The open step is the selected row (brighter text).
5. **Content sections** keep what the popup showed: text payload (prompt/text hero), payload/fields, message,
   and a collapsed "Raw line" with Copy — styled as calm sections (section titles like Memory's
   `detail-section-title`, no bright blue bar).
6. **Closing** with the close button or Esc; clicking another row in the list switches the blade to it.

## Revision 1 (user, 15:15: "the blur in Logging may be lifted when the blade is visible")

7. While the Logging blade is open, the list is never blurred or faded, even in the "offline / reconnecting"
   state (`.mcp-trace-shell--offline`), which otherwise blurs the list. Without a blade that state keeps its blur.

## Must NOT change

Row selection, keyboard navigation, filters, infinite history scroll; existing tests, including
`logging-text.spec.ts` (text hero `.mcp-inspect-section--text-hero` / `.mcp-inspect-text-hero`).

## Tests (RED first) — `e2e/logging-blade.spec.ts`

- click a row → `aside.memory-blade.mcp-blade` is visible, no `.mcp-inspect-overlay`; the list scroller is
  still visible;
- the blade width equals the Memory blade width;
- details show a Time without `.\d{3}` and a Direction value;
- steps: `.calm-list` rows inside the blade; for every step, the badge's right edge ≤ the title's left edge;
  the ENR step badge border color equals the ENR list badge border;
- clicking a step selects it; Esc closes the blade; clicking another list row updates the title.
- Mutants added to `scripts/calm-lists-mutants.sh`: the blade class dropped; the step kind colors dropped.

No new dependencies. Files: this spec, `e2e/logging-blade.spec.ts`, EVIDENCE `docs/specs/logging-blade-EVIDENCE.md`;
edits in `McpTraceLive.tsx`, `index.css`, the mutants script.
