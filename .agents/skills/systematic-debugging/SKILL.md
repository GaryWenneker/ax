---
name: systematic-debugging
description: Root-cause debugging process. Use before proposing fixes for any bug, test failure, or unexpected behavior.
triggers: ["bug", "debug", "error", "fail", "broken", "crash", "fix", "unexpected", "not working", "regression"]
tags: ["debugging", "methodology"]
priority: 70
---
# Systematic Debugging

No fixes without root cause. Symptom fixes are failure.

## Phase 1: Investigate

1. Read error messages and stack traces completely — they often contain the answer.
2. Reproduce consistently. If not reproducible, gather more data — do not guess.
3. Check recent changes (`git diff`, recent commits, config changes).
4. In multi-component systems, add diagnostic logging at each boundary. Run once to find WHERE it breaks before proposing WHY.

## Phase 2: Analyze

1. Find working examples of similar code in the same codebase.
2. List every difference between working and broken — however small.
3. Trace the bad value backward through the call stack to its origin. Use `ax_callers` for call paths.

## Phase 3: Hypothesize

1. State one hypothesis: "X is the root cause because Y."
2. Test with the smallest possible change. One variable at a time.
3. Did not work? New hypothesis — do not stack fixes.
4. After 3 failed attempts: stop. Question the architecture with the user.

## Phase 4: Fix

1. Write a failing test that reproduces the bug.
2. Implement one fix addressing root cause.
3. Verify: test passes, no other tests broken.

Stop signals: "just try changing X", "quick fix for now", "it's probably X", proposing solutions before tracing data flow.
