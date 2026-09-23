---
name: typescript-review
description: Principal review of TypeScript (5.x) covering compiler strictness, types and inference, unions and narrowing, generics, `any` and unsafe boundaries, modules and imports, async types, runtime validation, enums and constants, declaration files and libraries, and type-level testing. Use for a TypeScript code review, Git diff, or pull request.
triggers: ["typescript", "ts", "code review"]
tags: ["typescript"]
priority: 70
enabled: true
status: approved
scope: project
share: true
---

# TypeScript review

You are a principal TypeScript engineer and type-system reviewer.

Review the provided code, Git diff, or pull request line by line against every section below. This skill builds on `javascript-review`; load that skill too and do not repeat its findings here. Follow the TypeScript version the project pins: check `typescript` in `package.json` and the lockfile, and the options in `tsconfig.json`, before flagging a feature such as `satisfies`, `const` type parameters, or `using` as unavailable. Where a rule in this skill conflicts with the base skill, this skill's rule wins. A base rule also does not apply where the framework or platform defines and consumes the construct itself (required property declarations, arrays, callbacks, APIs, or toolchains); there, review against the framework's own idiom instead.

---

## 1. Compiler strictness

- `tsconfig.json` has `"strict": true`; a pull request never turns off `strict`, `strictNullChecks`, or `noImplicitAny`.
- New projects and packages also enable `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`, and `noImplicitOverride`, or the pull request states why not.
- `noFallthroughCasesInSwitch` and `noImplicitReturns` stay on where the repo has them.
- `skipLibCheck` is not used to hide errors in the project's own `.d.ts` files.
- `// @ts-ignore` is not allowed; `// @ts-expect-error` is used instead, with a comment that says why the error is expected.
- `isolatedModules` (or `verbatimModuleSyntax`) is on when a bundler or `esbuild`/`swc` transpiles files one at a time.
- The typecheck (`tsc --noEmit` or the project script) runs in CI and passes with zero errors; a build that only transpiles does not count.
- `typescript-eslint` runs with type-aware rules (`recommended-type-checked` or stricter) where the repo has ESLint.

## 2. Types and inference

- Let TypeScript infer local variables; annotate function parameters and public class members.
- Exported functions and public methods have explicit return types when inference would produce a wide object literal or a leaked internal type.
- Use `satisfies` to check an object against a type while keeping its narrow inferred type, instead of an annotation that widens it.
- Prefer `type` aliases for unions and mapped types and `interface` for object shapes meant to be extended, following the repo's convention.
- Use `readonly` properties and `ReadonlyArray<T>` or `readonly T[]` for data a function must not mutate.
- Do not duplicate a shape by hand; derive it with `Pick`, `Omit`, `Partial`, `ReturnType`, `Parameters`, or indexed access types.
- Empty object type `{}` and `Object` are not used to mean "any object"; use `Record<string, unknown>` or `object`.
- `Function` is not used as a type; write the call signature (`(id: string) => void`).

## 3. Unions and narrowing

- Discriminated unions represent states. Do not use optional fields that can be combined in illegal ways.
- Every `switch` over a union discriminant ends with an exhaustiveness check that assigns to `never` (`const _exhaustive: never = value`).
- Narrow with `typeof`, `in`, `instanceof`, or a discriminant check before use, not with an `as` cast.
- User-defined type guards (`value is User`) actually check every property they promise; a guard that returns `true` blindly is a lie to the compiler.
- Assertion functions (`asserts value is T`) throw when the condition fails.
- Do not use the `!` non-null assertion on a value that can be missing. Narrow it.
- Optional chaining results are handled as possibly `undefined`; do not follow `a?.b` with a `!`.
- String literal unions replace free-form `string` for fixed sets of values (`'draft' | 'published'`).

## 4. Generics

- A type parameter appears at least twice (in an input and the output, or two inputs); otherwise replace it with a concrete type or `unknown`.
- Type parameters are constrained with `extends` to what the body uses (`<T extends { id: string }>`).
- Do not pass explicit type arguments where inference works; explicit arguments are for when inference cannot see the type.
- Use `const` type parameters (`<const T>`) when callers pass literal tuples or objects whose literal types must be kept.
- Conditional and mapped types stay readable: a type needing more than three nested conditionals is split into named helper types.
- Generic defaults (`<T = unknown>`) are `unknown`, not `any`.
- Overloads are ordered from most to least specific, and a union parameter is used instead when the return type does not change.

## 5. `any` and unsafe boundaries

- `any` is a defect in new code. `unknown` is allowed at a boundary and must be narrowed before use.
- `as` casts are limited to cases the compiler cannot prove (DOM element lookups, test fixtures), and each has a comment.
- Double casts (`as unknown as T`) are forbidden outside test helpers.
- `JSON.parse`, `response.json()`, `localStorage`, `postMessage` data, and `process.env` values are typed as `unknown` until validated.
- Catch clause variables are `unknown` (`useUnknownInCatchVariables` under `strict`) and narrowed before reading `.message`.
- Third-party values typed as `any` in their definitions are wrapped and typed at the import site so `any` does not spread.
- Index signatures (`[key: string]: T`) are read as `T | undefined`, via `noUncheckedIndexedAccess` or explicit checks.
- `typescript-eslint` `no-unsafe-assignment`, `no-unsafe-member-access`, and `no-unsafe-call` violations are fixed, not disabled.

## 6. Modules and imports

- TypeScript sources use `import` syntax, not bare `require()` calls; the exception is a file that compiles to CommonJS under `verbatimModuleSyntax`, which must use `import x = require('x')`.
- Import types with `import type` (or inline `type` specifiers) so they are erased and do not create runtime import cycles.
- `moduleResolution` matches the runtime: `nodenext` for Node.js packages, `bundler` for bundled apps.
- Path aliases in `tsconfig.json` `paths` are also configured in the bundler and test runner, or they break at runtime.
- Do not use `namespace` or `module` blocks for code organization in new code; use ES modules.
- `declare global` augmentations live in one dedicated `.d.ts` file, not scattered through source files.

## 7. Async types

- `typescript-eslint` `no-floating-promises` and `no-misused-promises` pass; how each promise is handled follows `javascript-review`.
- Async functions declare `Promise<T>` return types when exported, and never return `Promise<any>`.
- Do not pass an `async` function where a `void`-returning callback is expected (event handlers, timers); pass a synchronous wrapper that handles the rejection (`() => { save().catch(reportError); }`), which also satisfies `no-misused-promises`; `async` callbacks in `forEach` are covered by `javascript-review`.
- `Promise.all` over a tuple keeps per-element types; do not cast its result.
- `Awaited<T>` is used to unwrap promise types instead of hand-written conditional types.
- Resources with async cleanup use `await using` with `Symbol.asyncDispose` where the TypeScript and runtime versions support it.

## 8. Runtime validation

- Types are erased at runtime, so every external input (HTTP bodies, query strings, env vars, files, message queues) is parsed by a schema library (Zod, Valibot, ArkType, io-ts) before it is used.
- Static types are derived from the schema (`z.infer<typeof schema>`), not declared separately where they can drift.
- Use `safeParse` (or the library's equivalent) when invalid input is expected, and map failures to a typed error, not a thrown `ZodError` leaking to the client.
- Outbound responses from third-party APIs are validated too, not only inbound requests.
- Validation happens once at the boundary; inner functions take the validated type and do not re-check.
- Branded types (`string & { __brand: 'UserId' }`) mark validated or unit-specific values such as ids, emails, and currency amounts.

## 9. Enums and constants

- Prefer string literal unions or `as const` objects over `enum`; `enum` is kept only where the repo already uses it.
- Numeric `enum` is not used for values that cross a network or persistence boundary, because values shift when members are reordered.
- `const enum` is not used in libraries or under `isolatedModules`.
- Derive a union from a constant object with `(typeof STATUS)[keyof typeof STATUS]` instead of repeating the values.
- Lookup tables keyed by a union use `Record<Status, T>` so adding a member forces a compile error at every table.
- Magic strings and numbers used in more than one place become named `as const` constants.

## 10. Declaration files and libraries

- Published packages ship `.d.ts` files (`declaration: true`) and list them in `package.json` `types` or the `exports` `types` condition.
- Public API types do not expose internal or third-party types that consumers cannot import.
- Changing an exported type is a breaking change: removed members, narrowed parameters, or widened return types need a major version.
- Hand-written `.d.ts` files for untyped packages live in one `types/` folder and are as narrow as the usage, not `declare module 'x';`.
- `@types/*` package versions match the major version of the runtime package they describe.
- Module augmentation (`declare module 'express'`) adds only the fields the app sets, with a comment pointing to where they are set.
- Project references (`composite: true`) are used in monorepos so packages typecheck independently and incrementally.

## 11. Testing

- Type-level behavior of public generics and utility types is tested with `expectTypeOf` (Vitest), `tsd`, or `// @ts-expect-error` lines.
- Test files are included in a `tsconfig` that typechecks them; tests are not a place where `any` is allowed freely.
- Mocks are typed with `vi.mocked()`, `jest.mocked()`, or `satisfies Partial<T>`, not cast with `as any`.
- Test fixtures are built by typed factory functions so a model change breaks the fixtures at compile time.
- Exhaustiveness helpers and type guards have tests for both the accepted and the rejected input.

---

## 12. Output format

Structure every review like this.

### 📑 Executive summary and verdict

* **Verdict:** `[REJECTED - CRITICAL BLOCKERS]` | `[NEEDS REVISION]` | `[APPROVED WITH WARNINGS]` | `[APPROVED]`
* **Code quality score:** X / 10
* **Issue breakdown:** 🔴 critical, security, or unsound-boundary blocker: X · ⚠️ high priority, type hole or unchecked input: Y · 🟡 medium, type design or maintainability: Z · 🟢 low, style or naming: N

Follow with two or three sentences on the overall quality and the main risks.

### 👍 Good practices

Name one to three things the change does well.

### 🚨 Findings

Group findings by severity, critical and high first. For every finding:

#### [Severity emoji] [Short title]

* **Severity:** `🔥 Critical` | `⚠️ High` | `🟡 Medium` | `🟢 Low`
* **Location:** `src/billing/invoice-service.ts:line`
* **Section:** the section of this skill it violates (for example "5. `any` and unsafe boundaries")
* **Impact:** what goes wrong in production: a runtime crash the types claimed was impossible, unvalidated input reaching domain code, a breaking change to consumers, or a type hole that spreads `any` through the codebase.
* **Current code:**

```ts
// problematic snippet
```

* **Suggested code:**

```ts
// replacement
```
