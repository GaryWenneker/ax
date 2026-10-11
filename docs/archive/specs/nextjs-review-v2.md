# SPEC: extend `nextjs-review` (nextjs stack pack)

Status: draft, awaiting approval. Tier 2. Same approach as `dotnet-code-review-v2.md`.

## Goal

Merge the user's Next.js & React directive into
`crates/ax-policy/templates/stacks/nextjs/skills/nextjs-review/SKILL.md`.
Nothing is duplicated, within the skill or against `react-review`: the nextjs pack always installs the react pack, so a rule already in `react-review` stays there.
The skill ships through the stack picker's seeding (`ax init`, `ax policy stack apply/upgrade`).

## New structure

1. Structure and naming: special App Router files; kebab-case folders; route groups `(group)`; private `_folders`; colocation; PascalCase components with `Props` types; `use*` hooks; kebab-case or camelCase utilities; no `I` prefix; Server Action verbs.
2. TypeScript: no `any` (`unknown` plus narrowing); Zod or Valibot at every external boundary; type guards or `z.infer` instead of `as`; named props types.
3. Server and client components: server by default; the four reasons for `'use client'`; boundaries at the leaves; pass server components as `children` or slots; serializable props; `import 'server-only'`.
4. Server Actions: `'use server'`; Zod validation; auth and authorization inside every action; `useActionState`, `useFormStatus`, `useOptimistic`; `revalidatePath` / `revalidateTag`; `redirect()` and `notFound()` outside `try/catch`.
5. Data fetching and caching: explicit `fetch` cache options; `cache()` / `unstable_cache` for non-fetch calls; `Suspense` streaming and PPR; fetch in server components, not `useEffect`; no waterfalls (`Promise.all`); plus the existing `params` / `searchParams` promise rule, `generateMetadata`, and "do not opt the whole app out of caching".
6. Client state and React 19: Zustand or Jotai over one large Redux store; no heavy root providers; `use()`; `useCallback` / `useMemo` only with a measured reason.
7. Performance and assets: `next/image` instead of `<img>`; `priority` on the LCP image; `width`/`height` or `fill`; remote hosts in the config; `next/font`; `next/script` strategies; `next/dynamic` for heavy client libraries; subpath and ESM imports.
8. Styling and UI (when the repo uses Tailwind): utility classes; `cn()` with `clsx` and `tailwind-merge`; headless accessible primitives; `next-themes` with `suppressHydrationWarning`; mobile-first breakpoints.
9. Security: `NEXT_PUBLIC_` only for public values; env validation at build time; Route Handlers check the session, validate with Zod, and return typed `NextResponse.json` with correct status codes; CSP and security headers; `DOMPurify` / `sanitize-html` before `dangerouslySetInnerHTML`.
10. Errors and observability: `error.tsx` per key segment; `global-error.tsx`; a `reset()` retry; `notFound()`; structured server logging; error tracking.
11. Testing: Vitest or Jest plus React Testing Library; Playwright or Cypress for critical journeys; MSW for network mocks.
12. Output format: the same shape as the dotnet skill, with this directive's labels (hydration blocker, RSC violation) and `tsx` code blocks.

## Corrections to the directive (please check)

| In the directive | In the skill, because |
|---|---|
| kebab-case for dynamic parameters too (`[user-id]`) | Static segments are kebab-case. A dynamic segment name becomes a `params` key, so it is a valid identifier (`[userId]`), or matches the repo |
| Ban `Date` objects across the server-to-client boundary | React 19 serializes `Date`, `Map`, `Set`, and promises. The skill bans plain functions (Server Actions are allowed), class instances, and ORM entities |
| `useFormState` as an equivalent of `useActionState` | `useFormState` is deprecated in React 19; the skill mentions it only for React 18 codebases |
| `next/dynamic` with `ssr: false` anywhere | Next.js 15 rejects `ssr: false` in a server component, so the skill says it goes inside a client component |
| `import check from 'lodash/check'` | That module does not exist; the example becomes `lodash/debounce` |
| `next/script` `worker` strategy listed as a normal option | It is experimental; the skill says so |
| "Explicit component return types … or `FC`" (contradicts itself) | The skill requires a named props type and no inline anonymous prop objects; return types may be inferred |
| Tailwind, shadcn, and next-themes as mandatory | These apply when the repo already uses them; a review does not introduce a UI library |
| "Ban `<img>`" versus the current "when the app already does" | `next/image` replaces `<img>` for content images. SVG icons and email templates are exceptions |
| Derived values computed during render; Context use | Already in `react-review`; left out |

## Behaviors (tests)

- N1 `nextjs_review_skill_covers_every_section`: all 12 headings plus key phrases (`'use client'`, `server-only`, `useActionState`, `revalidateTag`, `outside \`try/catch\``, `Promise.all`, `next/image`, `NEXT_PUBLIC_`, `DOMPurify`, `global-error.tsx`, `MSW`, `APPROVED WITH WARNINGS`, `No \`any\``).
- N2 `nextjs_review_skill_has_no_duplicate_bullets`: no duplicate bullets within the skill, and none shared with `react-review`; more than 50 bullets in total.
- N3 `upgrade_rewrites_an_unedited_older_nextjs_review`: an unedited old copy is replaced, and the lock records 1.2.0.
- I will refactor the dotnet tests to share helpers with these (a test-structure refactor: assertions unchanged, mutants rerun).

## Must not

- Change `react-review` or the four nextjs and react `.mdc` rules.
- Overwrite a hand-edited project copy.

## Setup plan

- Edit the skill template.
- Bump the nextjs pack from 1.1.0 to 1.2.0 (`pack.toml` and `stack_catalog.rs`).
- Add tests in `stacks.rs`.
- Rename `scripts/dotnet-review-mutants.py` to `scripts/stack-review-mutants.py` and add nextjs mutants to it.
- Update the Stacks section of the policy engine guide and the dotnet EVIDENCE reference to the renamed script.
- No new dependencies, no commits.
- Gauntlet: tests, mutants, and a real run with the old binary, then reinstall and `upgrade`, then the review loop.
