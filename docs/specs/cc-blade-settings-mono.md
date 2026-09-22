# Command Center — blade dismiss, Settings polish, Mono theme

Tier 2. Isolation: none (UI in tree). Spec approval: not obtained (autonomous run).

## Behaviors

### B1 — Policy blade slides out on dismiss
Given a Rules or Skills editor blade is open, when the user clicks outside the blade (or Close), the blade plays the reverse of `policy-blade-slide-in` (`translateX(0)` → `translateX(100%)`, 320ms) and only then unmounts. Instant unmount without the closing class is forbidden.

### B2 — Dismiss helpers
`policyDetailOpen(selected, closing)` is true when `selected` is set or `closing` is true. `policyWorkspaceHostClass(closing)` includes `policy-inline-host--closing` iff closing.

### B3 — Mono theme
`THEMES` includes `id: 'mono'`, label `Mono`. Accent and chrome are grayscale. `themeById('mono')` resolves. Existing theme ids remain. W8 statusbar ink vs painted fill stays ≥ 4.5:1 for every theme including Mono.

### B4 — Settings layout
Settings AI Agents targets render as cards (`agent-target-card`), not a flat checkbox+pills row. Theme chooser sits in a full-width subsection. No new non-English strings.

## Invariants
- Existing W1–W10 WCAG tests still pass.
- Clicking a list row while closing cancels close and opens that row.
- `prefers-reduced-motion: reduce` still disables blade animation.

## Files
- `crates/ax-web/web-ui/src/lib/policyBladeMotion.ts`
- `crates/ax-web/web-ui/src/lib/policyBladeMotion.test.ts`
- `crates/ax-web/web-ui/src/lib/themes.ts`, `themes.test.ts`
- PolicyRules.tsx, PolicySkills.tsx, AgentsSettingsSection.tsx, Settings.tsx, index.css
- `site/src/content/docs/guides/command-center.md`

## Deps
None.

## Gauntlet
`node --test` on new + themes tests; `npx tsc --noEmit`; grep CSS for `policy-blade-slide-out`; browser verify.
