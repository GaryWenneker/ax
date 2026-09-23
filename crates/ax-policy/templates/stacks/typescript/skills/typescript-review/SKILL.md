---
name: typescript-review
description: Review TypeScript for types, modules, and async boundaries.
triggers: ["typescript", "ts", "code review"]
tags: ["typescript"]
priority: 70
enabled: true
status: approved
scope: project
share: true
---

# TypeScript review

## Types
- Model data with `type` or `interface`. `any` is a defect on new code. `unknown` is allowed at a boundary and must be narrowed before use.
- Discriminated unions represent states. Do not use optional fields that can be combined in illegal ways.
- Public functions have explicit return types when the inference is a wide object or a promise of `any`.

## Modules
- Stay on the module style the package already uses. Do not mix `require` into an ESM package.
- Import types with `import type` when the project already does, so types are erased.

## Async
- Await or return every promise. A floating promise needs a comment and a `.catch` that records the failure.
- Do not use the `!` non-null assertion on a value that can be missing. Narrow it.

## Tests
- Typecheck is part of done (`tsc` or the project script). A test that only checks a mock was called is not coverage of the behavior.
