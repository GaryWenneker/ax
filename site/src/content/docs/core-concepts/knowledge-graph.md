---
title: The Knowledge Graph
description: The node and edge kinds the graph is built from.
---

ax stores three things: **nodes** (symbols and files), **edges** (relationships between them), and **files**. Every node and edge carries an exact `kind`, drawn from a fixed vocabulary so queries are consistent across languages.

![Nodes — browse indexed symbols with kind/language filters, caller/callee detail, and pagination](/screenshots/cc-nodes.png)

## Node kinds

`file`, `module`, `class`, `struct`, `interface`, `trait`, `protocol`, `function`, `method`, `property`, `field`, `variable`, `constant`, `enum`, `enum_member`, `type_alias`, `namespace`, `parameter`, `import`, `export`, `route`, `component`, `test`, `table`, `doc`.

## Edge kinds

`contains`, `calls`, `imports`, `exports`, `extends`, `implements`, `references`, `type_of`, `returns`, `instantiates`, `overrides`, `decorates`.

## Provenance

Most edges come straight from the AST. A few — at dynamic-dispatch boundaries that static parsing can't follow — are **synthesized** and marked with `provenance: 'heuristic'` plus the wiring site that created them. These are surfaced inline in `explore` and the `node` trail, so an agent can see exactly where a connection came from.

## Edge confidence

Where provenance records *how* an edge was created, **confidence** records *how sure ax is that it's correct*:

- **`extracted`** — read directly from the source via the tree-sitter AST (e.g. an import, or a direct call to a resolved local symbol). Rendered as a **solid** line.
- **`inferred`** — resolved by a heuristic, name-matching, or framework pass. Rendered **dashed**.
- **`ambiguous`** — several candidate targets existed and the resolver picked one. Rendered **dotted**.

Confidence is surfaced as a badge in the Command Center edge lists and in `ax_node` / `ax_callers` / `ax_callees` output, so an agent can weigh a relationship before acting on it.

## Documentation nodes

Markdown files (`.md` / `.mdx`) are indexed as `doc` nodes so READMEs, ADRs, and design notes are part of the graph — not invisible to it. Relative links between docs become `extracted` `references` edges; inline code spans that match a code symbol's name become `inferred` `references` edges from the doc to that symbol. Docs appear as distinct square nodes in the [graph visualization](/guides/architecture-insights/#visual-graph).

## Querying it

![Search — full-text symbol search for "pipeline" with detail panel showing kind, language, callers, and source preview](/screenshots/cc-search.png)

- **Search** symbols by name (FTS5).
- **Callers / callees** walk the call graph one hop at a time.
- **Impact** computes the transitive radius affected by a change.
- **Explore** returns source for several related symbols grouped by file, plus the call path among them, in one call.

See the [CLI](/ax/reference/cli/) and [MCP Server](/ax/reference/mcp-server/) references for how to run these.
