---
name: vue-review
description: Review Vue components, composition API, and props.
triggers: ["vue", "composition api", "code review"]
tags: ["vue"]
priority: 70
enabled: true
status: approved
scope: project
share: true
---

# Vue review

- Prefer `<script setup>` when the neighboring components use it.
- Props and emits are declared with types. Do not read undeclared attributes as a data model.
- One single-file component owns one piece of UI. Shared logic moves to a composable, not a mixin, when the project is on Vue 3.
- `ref` and `reactive` follow the local file. Do not mix them for the same object without a reason.
- Side effects that touch the DOM or a subscription live in `onMounted` / `onUnmounted` (or the project's composable that already does that).
- Pinia or the existing store is the shared state. Do not add a second global.
