# EVIDENCE: same-row toggle (partial, blade-outside-click Revision 1 behavior 7)

Spec: `docs/specs/blade-outside-click.md`, Revision 1, behavior 7. User request 15:29: "bij logging, rules, skills,
memory, moet dat gebeuren." Outside-click (behaviors 1–6) for Memory, Logging, Nodes and Unresolved is not done yet.

| Behavior | Test | Result |
|---|---|---|
| Plain click on the only selected row deselects (Rules, Skills helper) | `src/lib/policySelection.test.ts` | RED seen (1 fail), then green |
| Same row closes + deselects, again reopens: Logging, Rules, Skills, Memory | `e2e/blade-dismiss.spec.ts` "clicking the open row again" | RED seen on Logging, Rules, Skills; green after |
| Memory toggle (pre-existing) | same test | throwaway mutant `setSelectedId(m.id)` → test failed; restored (`cmp` identical) |
| Another row switches | `e2e/blade-dismiss.spec.ts` "another row", `e2e/logging-blade.spec.ts` | green |

Gauntlet (fresh, after last edit): unit 143/143, `tsc --noEmit` exit 0, e2e regression suites + toggle tests 44 passed.
Remaining failures: Nodes and Unresolved toggle and all outside-click tests except Rules/Skills — out of this request's scope.
Served bundle `index-Bg6aWJg4.js` equals `dist/index.html`.

Test change disclosed: `logging-blade.spec.ts` "list click switches" clicked the row that was already open; it now
clicks an unselected "Prompt in" row, which is what the test name claims.
