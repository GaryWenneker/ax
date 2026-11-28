---
title: Resolution & Frameworks
description: How ax connects references and links routes to handlers.
---

Extraction produces nodes and raw edges; **resolution** turns names into real connections.

## Reference resolution

After parsing, ax resolves:

- **Imports** → the source files they point at (including tsconfig path aliases and cargo workspace members). Path aliases follow TypeScript's rules: an exact pattern matches only itself, `@/*` matches only imports that start with `@/`, the longest matching prefix wins, each target is tried in order, and targets are relative to `compilerOptions.baseUrl` when it is set.
- **Calls** → their definitions, by import resolution and name matching.
- **Inheritance** → `extends` / `implements` between types.

## Framework awareness

ax recognizes web-framework routing files and emits `route` nodes linked by `references` edges to their handler classes or functions — so querying the callers of a view or controller surfaces the URL pattern that binds it. See [Framework Routes](/guides/framework-routes/) for the full list of recognized frameworks.

## Dynamic-dispatch coverage

Static parsing misses computed and indirect calls, so flows can break at dynamic dispatch. ax bridges several of these boundaries with synthesizers so a flow connects end-to-end:

- Callback / observer registration
- `EventEmitter` channels
- React re-render (`setState` → `render`)
- JSX child (`render` → child component)
- Interface → implementation dispatch

Every synthesized edge is marked `provenance: 'heuristic'` with the site that wired it, and is shown inline wherever a path crosses it.

## Unresolved references

A reference that cannot be resolved to a target in the graph stays in the index as an unresolved reference. `ax status` and `ax_status` report how many there are, and `ax report` includes an unresolved-refs summary. `ax lsp enrich` asks a language server for the definition of unresolved references and writes the edges it finds; see [`ax lsp status` / `ax lsp enrich`](/reference/cli/#ax-lsp-status--ax-lsp-enrich).

The full ax, which is being phased out, also has an **Unresolved references** page in its Command Center to browse and reconcile them; see [Command Center](/guides/command-center/). `ax-admin` has no such list.
