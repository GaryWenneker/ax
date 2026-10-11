# Evidence — Savings page Azure colors

Spec: `/Users/gary/io/ax/docs/specs/savings-azure.md`
Spec approval: not obtained (autonomous run).

## Behavior map

| ID | Check | Result |
|---|---|---|
| S1–S5 | `node --experimental-strip-types --test src/savingsPortal.test.ts` | 5 passed |
| Splash stays off | `greenVesselIntro.test.ts` overlay assertion | pass (18 tests with the savings file, 0 failed) |
| Dark page | `http://127.0.0.1:7070/savings`, theme `azure-dark`, no `#ax-green-vessel` | timeline bars `rgb(92, 177, 247)`, heatmap and token path blue, hero figures light |
| Light page | same route after `azure-light` | white panels, blue header, blue bars, dark figures; theme restored to `azure-dark` |

Bundle: `index-B1LPTtH0.js`. Calculations and API calls were not changed.

## Skipped

Mutation tool, coverage fail-under, and Playwright for this CSS slice. Browser check is the execution layer.
