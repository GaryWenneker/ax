# EVIDENCE — policy links open the right rule or skill

Spec: `docs/specs/policy-link-origin.md` (approved: "Yes, build it"). Tier 2 bug fix. No commits (user convention); source state is the uncommitted working tree.

## Root cause (two parts)

1. The Rules and Skills pages ignored `origin`/`projectId` from the URL and reused the origin of the last row clicked, so a link to a global item stored under another project asked the API for a project item and got a 404.
2. `navigateRoute` dropped `origin` and `projectId` before building the URL, so even a correct selection produced a bare `?name=` URL. Found during GREEN when L4 kept failing; the spec's L4 already required it, so the spec did not change.

## Behaviour → test

| Spec | Test | Result |
|---|---|---|
| L1 origin=global URL opens the global item (skills, rules) | `e2e/policy-link-origin.spec.ts` L1 ×2; `policySelection.test.ts` L1, L1-fallback | pass |
| L2 after a global item, a project link still opens | e2e L2 ×2; unit L2 | pass |
| L3 bare name to a global-only item opens it | e2e L3 ×2; unit L3, L3-unknown | pass |
| L4 row click puts origin + projectId in the URL | e2e L4 ×2; `routes.test.ts` L4 + bare-URL case | pass |
| Must not change the API | no Rust changes | — |
| Must not break existing specs | policy-wysiwyg, policy-graph, blade-dismiss runs below | pass (baseline only) |

## Gauntlet (final run after the last edit)

- Unit: `node --experimental-strip-types --test src/*.test.ts src/lib/*.test.ts` → 208/208 pass.
- Types: `npx tsc -b --noEmit` → 0 errors.
- Build: `npm run build` → exit 0.
- e2e: `npx playwright test e2e/policy-link-origin.spec.ts e2e/policy-wysiwyg.spec.ts e2e/policy-graph.spec.ts e2e/blade-dismiss.spec.ts --project=system-chrome` → 43 passed, 6 failed. The 6 are the pre-existing baseline (blade-dismiss on /memory, /logging, /nodes, /unresolved), which also fail with this change reverted.
- Mutation: `scripts/link-origin-mutants.sh` → 5/5 killed (origin and projectId dropped in `navigateRoute`, route projectId ignored, non-global origin treated as global, project copy ignored in bare-name lookup).
- Real execution: rebuilt with `scripts/reinstall-cli.sh`, `ax web` restarted, `/` returns 200, served bundle matches `dist/index.html`. A click on `azdo-pr-review` fetches `/api/policy/skills/azdo-pr-review?origin=global&projectId=2`.
- Browser mutants (page wiring): skipped; the e2e L1–L4 went RED before the page fix and GREEN after, which covers that wiring.
- Supply chain: no new dependencies.

## Known limits

- The blade waits for the first list load before resolving a bare name without origin, so it appears slightly later on a cold load.
- Spec approval was obtained; independent verification not performed (Tier 2).
