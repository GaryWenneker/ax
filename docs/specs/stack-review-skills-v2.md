# SPEC: principal-level review skills for every stack pack

## Goal

Today only `dotnet-code-review` (211 lines) and `nextjs-review` (166 lines) are full reviews. The other 28 stack packs ship a review skill of 19 to 48 lines. Every one of those 28 gets a review skill at the same level as dotnet and nextjs, delivered through the stack-selection seeding (`ax init` stack picker, `ax policy stack apply`, `ax policy stack upgrade`).

## Scope: the 28 skills

Every skill has 10 to 12 numbered review sections plus a final `Output format` section. The section plans below are the approved scope. Bullets are concrete, checkable rules ("`unwrap()` outside tests and `main` needs a comment explaining why it cannot fail"), never vague advice ("handle errors well").

| Stack | Skill | Review sections |
|---|---|---|
| angular | angular-review | structure and naming, standalone components and modules, dependency injection, signals and change detection, RxJS, templates and control flow, forms, routing and guards, HTTP and interceptors, performance, security, testing |
| astro | astro-review | project structure, components and props, islands and hydration directives, content collections, routing and endpoints, data fetching and SSR/SSG, images and assets, performance, SEO and accessibility, security, testing |
| c | c-review | naming and headers, memory ownership, buffers and strings, undefined behavior, integer safety, error handling, resources and cleanup, concurrency, portability and build flags, security, testing and tooling |
| cpp | cpp-review | naming and layout, ownership and RAII, value semantics and moves, modern C++ features, templates and concepts, error handling, containers and algorithms, concurrency, performance, undefined behavior and security, testing and tooling |
| dart | dart-review | naming and structure, null safety, types and immutability, async and streams, Flutter widgets, state management, performance, packages and dependencies, platform and security, testing |
| drupal | drupal-review | module structure, hooks and events, services and dependency injection, entities and fields, configuration management, caching and cache metadata, render arrays and Twig, forms and routing, access and security, updates and deployment, testing |
| go | go-review | naming and packages, errors, context, concurrency, API and types, interfaces, HTTP and IO, performance and allocation, security, modules and dependencies, testing |
| java | java-review | naming and structure, null safety and Optional, immutability and records, exceptions, collections and streams, concurrency, Spring and dependency injection, persistence and JPA, performance and memory, security, build and dependencies, testing |
| javascript | javascript-review | modules and structure, equality and types, variables and scope, functions and closures, async and promises, errors, DOM and browser, Node.js runtime, performance, security, dependencies, testing |
| kotlin | kotlin-review | naming and structure, null safety, immutability and data classes, sealed types and when, coroutines and structured concurrency, Flow, collections, Android specifics, interop with Java, security, testing |
| laravel | laravel-review | structure and naming, routing and controllers, validation and form requests, Eloquent and queries, migrations, authorization and policies, queues, jobs, and events, caching and configuration, Blade and APIs, security, testing |
| lua | lua-review | naming and modules, locals and globals, tables and metatables, strings, errors and pcall, coroutines, performance, C interop and sandboxing, security, testing |
| luau | luau-review | modules and requires, type checking modes and annotations, generics and type safety, tables and OOP, Roblox services, client-server and remotes, performance and memory, errors, security and anti-exploit, testing |
| objc | objc-review | naming and prefixes, memory and ARC, nullability and generics, properties and attributes, blocks and retain cycles, errors and exceptions, concurrency and GCD, Swift interop, API design, security, testing |
| optimizely | optimizely-review | structure, content types and properties, initialization modules and dependency injection, content repository and loading, caching, rendering and views, search and Find, commerce, security and access rights, deployment and configuration, testing |
| pascal | pascal-review | naming and units, types and records, memory and ownership, strings and encoding, exceptions and try/finally, classes and interfaces, generics and collections, threads, platform and compiler directives, testing |
| php | php-review | strict types and declarations, naming and PSR standards, classes and immutability, errors and exceptions, arrays and collections, input and output, database access, security, performance, dependencies and Composer, testing |
| python | python-review | naming and structure, typing, data models, errors and exceptions, resources and context managers, async, iterators and collections, performance, security, packaging and dependencies, logging, testing |
| r | r-review | naming and style, functions and arguments, vectors and types, data frames and tidyverse, NA and missing data, errors and conditions, performance and vectorization, reproducibility, packages and namespaces, testing |
| react | react-review | components and props, hooks rules, effects, state and derived state, Context, lists and keys, forms, rendering performance, accessibility, errors and boundaries, security, testing |
| ruby | ruby-review | naming and style, objects and modules, blocks and enumerables, nil and errors, Rails models and queries, controllers and routes, background jobs, performance, security, gems and dependencies, testing |
| rust | rust-review | naming and modules, ownership and borrowing, errors, types and traits, unsafe, async, concurrency, performance and allocation, API design, dependencies and features, testing |
| scala | scala-review | naming and structure, immutability, types and ADTs, implicits and givens, Option, Either, and errors, collections, effects and futures, concurrency, performance, build and dependencies, testing |
| sitecore | sitecore-review | Helix architecture, templates and fields, rendering and components, data access and Content Search, serialization, pipelines and events, caching, headless and JSS, security, deployment and configuration, testing |
| svelte | svelte-review | structure and naming, runes and reactivity, props and events, stores and state, SvelteKit routing and load functions, form actions, SSR and hydration, performance, accessibility, security, testing |
| swift | swift-review | naming and API design guidelines, optionals, value and reference types, protocols and generics, errors, concurrency and actors, memory and retain cycles, SwiftUI, performance, security, testing |
| typescript | typescript-review | compiler strictness, types and inference, unions and narrowing, generics, `any` and unsafe boundaries, modules and imports, async types, runtime validation, enums and constants, declaration files and libraries, testing |
| vue | vue-review | structure and naming, Composition API and script setup, reactivity, props, emits, and v-model, components and slots, state and Pinia, routing, performance, accessibility, security, testing |

## Shared shape (all 28)

1. Frontmatter keeps the current `name`, `triggers`, `tags`, `priority`, `scope`, and `share`. The `description` is rewritten to list what the review covers.
2. An intro: the reviewer role, "review line by line against every section below", and "follow the versions the project uses" (pinned language or framework version from the manifest).
3. Where one skill builds on another, the intro names it and says "load that skill too and do not repeat its findings here". The pairs are: laravel and drupal on `php-review`, sitecore and optimizely on `dotnet-code-review`, typescript on `javascript-review`, luau on `lua-review`, cpp and objc on `c-review`. nextjs on react already exists.
4. The final `Output format` section is the one nextjs uses: verdict, score out of 10, issue breakdown, good practices, and per-finding severity, location, section, impact, current code, and suggested code. The code fences use the stack's language.
5. Existing good bullets are kept, rewritten into the new sections where needed.

## Seeding

- All 28 packs go from 1.1.0 to 1.2.0 in `pack.toml` and `stack_catalog.rs`.
- `ax policy stack upgrade` then rewrites every unedited copy in existing projects. Edited copies stay untouched unless `--force`, as today.
- New projects get the new skills through `ax init` stack selection and `ax policy stack apply`.
- Nothing changes in global seeding or global.db: these are stack skills, not global ones.

## Acceptance tests (in `crates/ax-policy/src/stacks.rs`)

- S1 `every_stack_review_skill_is_complete`: for every stack pack, its review skill has at least 10 numbered `## N.` sections, ends with a `## N. Output format` section containing the verdict line and `**Location:**`, and has at least 50 bullets.
- S2 `every_stack_review_skill_has_no_duplicate_bullets`: no bullet appears twice within a skill.
- S3 `building_skills_do_not_repeat_their_base`: for each pair in shared-shape item 3, no bullet of the building skill is also in the base skill, and the building skill's intro names the base skill.
- S4 `upgrade_rewrites_every_unedited_older_review_skill`: for every stack, an unedited older copy is rewritten and the lock records `1.2.0`.
- S5 `every_stack_pack_is_version_1_2_0`: catalog version equals the `pack.toml` version, and both are `1.2.0` for all 30 packs.
- Existing tests stay green; no existing assertion is weakened.

## Must not

- Change dotnet-code-review or nextjs-review content.
- Change the stack engine (`apply`, `upgrade`, lock handling).
- Overwrite a project's edited skill copy without `--force`.
- Add any dependency.

## Setup plan

- Isolation: the current working tree, like the previous stack-review tasks, because the tree already holds uncommitted work this change builds on. No commits unless you ask.
- Authoring: the 28 skills are written in parallel batches by subagents that each get this spec, the nextjs skill as the format reference, and a strict list of the files they may touch (only their skill files). I then review every skill against its section plan before the tests count.
- Files touched: 28 `templates/stacks/<id>/skills/<skill>/SKILL.md`, 28 `pack.toml`, `stack_catalog.rs`, `stacks.rs` (tests), `scripts/stack-review-mutants.py` (new mutants), `site/src/content/docs/guides/policy-engine.md` (Stacks section), and `docs/specs/stack-review-skills-v2-EVIDENCE.md`.

## Gauntlet

- `cargo test -p ax-policy` and `cargo test -p ax-cli`.
- Clippy scoped to the changed Rust files.
- Mutation with `scripts/stack-review-mutants.py`: drop a section, duplicate a bullet, drop the output format, copy a base bullet into a building skill, and leave a pack at 1.1.0, each on several stacks.
- Real execution: reinstall, then `ax policy stack apply` and `ax policy stack upgrade` on a temporary project, checking that the new skills land.
- Review loop over the Rust, Python, and every Markdown skill until a round has zero findings.
