---
name: tdd
description: Test-driven development. Use when implementing features or fixing bugs — write the failing test first.
triggers: ["implement", "feature", "test", "tdd", "red green", "test first", "test-driven"]
tags: ["testing", "methodology"]
priority: 60
---
# Test-Driven Development

Write the test first. Watch it fail. Write minimal code to pass. Refactor.

## The cycle

1. **RED** — Write one test for the next behavior. Run it. It must fail because the feature is missing (not because of a typo).
2. **GREEN** — Write the simplest code that makes the test pass. No extras.
3. **REFACTOR** — Clean up duplication and names. Tests stay green.
4. **COMMIT** — Then repeat for the next behavior.

## Hard rules

- Code written before its test? Delete it. Start over with the test.
- Test passes immediately? You are testing existing behavior — fix the test.
- Do not add features, refactor other code, or "improve" beyond what the test requires during GREEN.
- Mocks only when unavoidable. Test real code.

## When stuck

| Problem | Fix |
|---|---|
| Don't know how to test | Write the assertion first. Design the API you wish existed. |
| Must mock everything | Code is too coupled. Use dependency injection. |
| Test setup is huge | Simplify the interface being tested. |

## Rationalizations that mean STOP

"Too simple to test" — simple code breaks; test takes 30 seconds.
"I'll test after" — tests written after pass immediately and prove nothing.
"TDD slows me down" — TDD is faster than debugging. Always.
