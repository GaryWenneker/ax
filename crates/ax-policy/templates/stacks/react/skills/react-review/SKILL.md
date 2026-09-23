---
name: react-review
description: Review React components, hooks, state, and effects.
triggers: ["react", "hooks", "jsx", "code review"]
tags: ["react"]
priority: 70
enabled: true
status: approved
scope: project
share: true
---

# React review

## Components
- One component does one job. Data enters through props. Do not fetch inside a presentational component if a parent or a loader already owns the request.
- Do not copy props into state unless the state is a draft the user edits.
- Keys are stable ids from the data. Do not use the array index when the list can reorder.

## Hooks
- Hooks run unconditionally at the top of the function component or custom hook.
- Effect dependency lists include every value read inside the effect. If a value should not retrigger the effect, restructure so it is not read.
- Effects synchronize with an external system. Derived values are computed during render, not stored in an effect.

## State
- Server data lives in the data library the repo already uses. Do not also keep a copy in `useState` that can drift.
- Context is for data that many distant children need. A prop is clearer for one or two levels.

## Tests
- Test what the user sees and does. A test that only checks a component rendered is not a behavior test.
