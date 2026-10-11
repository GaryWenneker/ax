# EVIDENCE — Brighter selected row, bounded scroll (docs/specs/selected-row-text.md)

Spec approval: "dat is perfect" (2026-09-28). The same reply added the scroll request, folded in as Revision 1
with an interpretation of "limited scroll" that the user has not yet confirmed. Tier 2. Branch `calm-list-style`,
HEAD b7caeb1 + working tree. Bundle `index-DsRwh-pF.js` (served = dist).

| Spec | Verified by |
|---|---|
| 1–3. selected title `#ffffff`, subtitle/meta lighter (Memory, Rules, Logging) | e2e `selected row text is brighter than other rows` (RED first: `rgb(204, 204, 204)`) · mutant M16 |
| 4. Memory and Logging `overscroll-behavior: none` | e2e `Memory and Logging scroll the same way…` (RED first: Memory `contain`) · mutant M17 |
| 5. the page itself does not scroll | same e2e (`scrollHeight` = `clientHeight`) |

- `tsc`: exit 0 · unit: 139/139 · e2e `app-feel`, `calm-lists`, `logging-text`, `md-editor-caret`: 22/22
- Mutants `scripts/calm-lists-mutants.sh`: 17/17 killed, sources restored
- Skills is not in the selected-text e2e; it uses the same `PolicyCalmList` as Rules.
- Limit: the tests check the CSS property, not a real trackpad bounce, which a headless browser cannot produce.
