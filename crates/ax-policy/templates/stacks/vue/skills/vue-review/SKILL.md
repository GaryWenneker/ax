---
name: vue-review
description: Principal review of Vue 3 covering structure and naming, the Composition API and `<script setup>`, reactivity, props, emits, and `v-model`, components and slots, Pinia state, Vue Router, performance, accessibility, security, and testing. Use for a Vue code review, Git diff, or pull request.
triggers: ["vue", "composition api", "code review"]
tags: ["vue"]
priority: 70
enabled: true
status: approved
scope: project
share: true
---

# Vue review

You are a principal Vue engineer and frontend accessibility and security reviewer.

Review the provided code, Git diff, or pull request line by line against every section below. Follow the Vue version the project pins: check `vue`, `vue-router`, `pinia`, and `nuxt` (if present) in `package.json` and the lockfile before flagging an API such as `defineModel`, reactive props destructure, or `useTemplateRef` as unavailable.

---

## 1. Structure and naming

- Single-file component names are PascalCase and multi-word (`UserCard.vue`, not `Card.vue`), so they never collide with HTML elements.
- One single-file component owns one piece of UI; a file over about 250 lines or with several responsibilities is split.
- Base, presentational components share a prefix (`BaseButton`, `AppIcon`); single-instance layout components use `The` (`TheHeader`) where the repo does.
- Tightly coupled child components carry the parent's name as a prefix (`TodoList.vue`, `TodoListItem.vue`).
- Composables live in `composables/`, are named `useX`, and live in files named after the composable (`useCart.ts`).
- Block order in a single-file component follows the repo's convention, typically `<script setup>`, `<template>`, `<style>`.
- Components are referenced in PascalCase in templates (`<UserCard />`) or kebab-case, consistently with the repo; do not mix both in one file.
- Props are declared in camelCase in script and written in kebab-case in in-DOM templates.

## 2. Composition API and script setup

- Prefer `<script setup>` when the neighboring components use it.
- New Vue 3 code uses the Composition API; mixing Options API and Composition API in one component needs a reason.
- Shared logic moves to a composable, not a mixin, when the project is on Vue 3.
- Composables are called synchronously in `setup()` or `<script setup>`, never inside a callback, and never after an `await` in a plain `setup()` function (only `<script setup>` restores the instance after `await`), so they bind to the current instance.
- Composables return an object of refs (not a `reactive` object) so callers can destructure without losing reactivity.
- Composables that register listeners, timers, or subscriptions clean them up in `onUnmounted` or `onScopeDispose`.
- Side effects that touch the DOM or a subscription live in `onMounted` / `onUnmounted` (or the project's composable that already does that).
- Template refs use `useTemplateRef()` in Vue 3.5+, or a `ref(null)` with a matching name in older versions, typed as the element or component instance.
- Use `defineOptions` for component options such as `name` or `inheritAttrs` instead of a second plain `<script>` block.

## 3. Reactivity

- `ref` and `reactive` follow the local file. Do not mix them for the same object without a reason.
- Do not destructure a `reactive` object directly; use `toRefs()`, or reactivity is lost (destructuring `defineProps()` in `<script setup>` is the exception on Vue 3.5+, where the compiler keeps it reactive).
- Do not replace a whole `reactive` object (`state = reactive({...})`); mutate its properties or use a `ref`.
- Derived values use `computed`, not a `watch` that writes to another ref.
- Computed getters are pure: no mutations, API calls, or DOM access inside `computed`.
- `watch` sources are a ref, a computed, or a getter (`() => props.id`), never a whole `props` or `reactive` object, which is watched deeply by default; `deep: true` is justified in a comment.
- `watchEffect` and `watch` that start async work handle cancellation through the `onCleanup` argument (or `onWatcherCleanup` in Vue 3.5+).
- Large immutable data (big lists from an API, third-party instances) uses `shallowRef` or `markRaw` to avoid deep proxy cost.
- Do not compare a reactive proxy to its raw object with `===`; use `toRaw()` when identity matters.

## 4. Props, emits, and v-model

- Do not read undeclared attributes (`$attrs`) as a data model; data the component depends on is a declared prop.
- Props use type-based `defineProps<{ ... }>()` where the repo uses TypeScript, or runtime declarations with `type` and `required` otherwise, with defaults from `withDefaults` or reactive props destructure (Vue 3.5+).
- Props are never mutated; a child that needs to change a value emits an event or uses `v-model`.
- Emits are declared with `defineEmits<{ (e: 'save', id: string): void }>()` or the named tuple syntax where the repo uses TypeScript, or `defineEmits(['save'])` otherwise, and every emitted event is declared.
- Two-way binding in Vue 3.4+ uses `defineModel()` instead of a hand-written `modelValue` prop plus `update:modelValue` emit.
- Object and array prop defaults use factory functions (`() => []`) in runtime declarations and in `withDefaults()`.
- Boolean props have a positive name and default to `false` (`disabled`, not `notEnabled`).
- `$attrs` fallthrough is deliberate: set `inheritAttrs: false` and bind `v-bind="$attrs"` on the right element when the root is not the target.

## 5. Components and slots

- `v-for` always has a `:key` bound to a stable unique id from the data, never the index for reorderable lists.
- `v-if` and `v-for` are never on the same element; filter with a `computed` or wrap in a `<template>`.
- `v-show` is used for frequent toggles and `v-if` for rarely shown or expensive content.
- Slots are typed with `defineSlots<{ default(props: { item: Item }): any }>()` where the repo uses TypeScript.
- Scoped slots pass only the data the consumer needs, and named slots replace boolean layout props.
- `provide`/`inject` uses typed `InjectionKey<T>` symbols, and `inject` has a default or throws a clear error when the provider is missing.
- Async components loaded with `defineAsyncComponent` have loading and error components.
- `<Teleport>` is used for modals and tooltips that must escape overflow or stacking contexts.

## 6. State and Pinia

- Pinia or the existing store is the shared state. Do not add a second global.
- Stores are defined with `defineStore` and a unique id; setup stores return every piece of state so devtools and SSR see it.
- Components destructure store state with `storeToRefs()` and actions directly from the store.
- State is changed through actions or `$patch`, not by assigning to store state from many components.
- Server data is cached by the repo's data library (TanStack Query, Pinia Colada, `useFetch` in Nuxt), not duplicated into ad-hoc store fields.
- Local UI state stays in the component; a store is not used for state only one component reads.
- Stores do not import each other in a cycle; shared logic goes into a third store or a composable.
- In SSR apps, no store or `ref` is created at module scope, which would leak state between requests.

## 7. Routing

- Routes are defined with `createRouter` and named routes; navigation uses `{ name: 'user', params: { id } }` rather than string-built paths.
- Route components are lazy-loaded with `() => import('./views/UserView.vue')`.
- Route params are read reactively (`useRoute()` in a `computed` or `watch`), because the same component instance is reused when only params change.
- Navigation guards (`beforeEach`, `beforeEnter`) return a value or a route location; the deprecated `next()` callback is not used in new code.
- Guards that check authentication only improve the UI; every protected API call is authorized on the server.
- Route `props: true` or a props function passes params as props so views stay testable.
- The router has a catch-all `/:pathMatch(.*)*` route that renders a not-found view.
- `scrollBehavior` is configured so back navigation restores the scroll position.

## 8. Performance

- Static content that never changes is marked with `v-once`, and large memoizable list rows use `v-memo` where profiling shows a gain.
- Long lists are virtualized (`vue-virtual-scroller`, `@tanstack/vue-virtual`).
- Expensive computations in templates move to `computed`; templates do not call methods that sort or filter on every render.
- Heavy components and libraries are code-split with `defineAsyncComponent` or dynamic `import()`.
- `<KeepAlive>` has `include` or `max` set so cached views do not grow memory without bound.
- Components do not pass new inline objects or arrays as props on every render to children that are expensive to update.
- Imports are tree-shakable (`import { debounce } from 'lodash-es'`), and the production build is checked with the bundler's analyzer when bundle size changes.

## 9. Accessibility

- Interactive elements are `<button>` or `<a>`; a clickable `<div>` with `@click` is replaced.
- Form inputs have a `<label for>` bound to a unique id generated with `useId()` (Vue 3.5+) or an equivalent.
- Images have `alt` text, and icon-only buttons have an `aria-label`.
- Modals trap focus, close on Escape, and return focus to the trigger element on close.
- Route changes move focus to the main heading or announce the new page through an `aria-live` region.
- Validation errors are linked with `aria-describedby` and the input carries `aria-invalid`.
- `eslint-plugin-vuejs-accessibility` is enabled where the repo lints, and its warnings are fixed.

## 10. Security

- `v-html` is used only with content sanitized by `DOMPurify` or an equivalent; never with user input.
- Bound URLs (`:href`, `:src`) from user data allow only safe schemes; `javascript:` URLs are blocked.
- Templates are never compiled from user-provided strings at runtime; the runtime-only build is used in production.
- Secrets never use the `VITE_` prefix or Nuxt `runtimeConfig.public`, since those values are bundled into the client.
- Style bindings (`:style`) do not take raw user input, which can inject CSS for data exfiltration or UI redressing.
- Third-party scripts and widgets are loaded with Subresource Integrity (`integrity`) or self-hosted.

## 11. Testing

- Components are tested with Vitest and `@vue/test-utils` or `@testing-library/vue`, asserting on rendered output and emitted events.
- Tests find elements by role, label, or text; `data-testid` is a fallback, and CSS class selectors are not used.
- Tests `await` DOM updates (`await nextTick()`, `await wrapper.setProps()`, `await flushPromises()`) before asserting.
- Emitted events are asserted with `wrapper.emitted('save')` including payloads, not only that a handler ran.
- Pinia stores are tested with `createTestingPinia()` or a fresh `setActivePinia(createPinia())` per test.
- Composables are tested directly, wrapped in a host component or `effectScope` when they use lifecycle hooks.
- `vue-tsc --noEmit` and `eslint-plugin-vue` pass in CI with no new errors or warnings.
- Critical user journeys have Playwright or Cypress end-to-end tests.

---

## 12. Output format

Structure every review like this.

### 📑 Executive summary and verdict

* **Verdict:** `[REJECTED - CRITICAL BLOCKERS]` | `[NEEDS REVISION]` | `[APPROVED WITH WARNINGS]` | `[APPROVED]`
* **Code quality score:** X / 10
* **Issue breakdown:** 🔴 critical, security, or state-leak blocker: X · ⚠️ high priority, reactivity or lifecycle bug: Y · 🟡 medium, component design or accessibility: Z · 🟢 low, style or naming: N

Follow with two or three sentences on the overall quality and the main risks.

### 👍 Good practices

Name one to three things the change does well.

### 🚨 Findings

Group findings by severity, critical and high first. For every finding:

#### [Severity emoji] [Short title]

* **Severity:** `🔥 Critical` | `⚠️ High` | `🟡 Medium` | `🟢 Low`
* **Location:** `src/components/CheckoutForm.vue:line`
* **Section:** the section of this skill it violates (for example "3. Reactivity")
* **Impact:** what goes wrong in production: an XSS hole, lost reactivity that shows stale data, state leaking between SSR requests, memory leaks from missing cleanup, inaccessible controls, or slow rendering.
* **Current code:**

```vue
<!-- problematic snippet -->
```

* **Suggested code:**

```vue
<!-- replacement -->
```
