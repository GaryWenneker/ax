---
title: Architecture Insights
description: Communities, god nodes, surprising connections, the architecture report, and the interactive graph — from one analysis engine.
---

**ax v2.1.8+** turns the knowledge graph into a map of your architecture. One analysis engine powers three surfaces: the `ax insights` / `ax report` CLI commands, the `ax_insights` / `ax_report` MCP tools, and the Command Center **Graph** page. It works entirely on your local index — no LLM calls required.

## What it computes

### Communities (subsystems)

ax runs [Leiden community detection](https://en.wikipedia.org/wiki/Leiden_algorithm) over the semantic edges (`calls`, `references`, `imports`, `extends`, `implements`) to partition the graph into **communities** — clusters of code that talk to each other far more than to the rest of the codebase. These usually line up with real subsystems (auth, the ship pipeline, the extraction layer). Each community gets a human-readable label derived from its dominant top-level module or its highest-degree member.

Turn the `--resolution` dial up for more, smaller communities; down for fewer, broader ones.

### God nodes

**God nodes** are the most-connected symbols, ranked by degree (in-degree + out-degree). They are the concepts everything depends on — the ones worth understanding first and changing most carefully. A symbol with a huge in-degree is a linchpin; a huge out-degree often signals an orchestrator or a god object worth splitting.

### Surprising connections

A **surprising connection** is an edge whose endpoints sit in **different communities *and* different top-level modules** — cross-cutting coupling you probably didn't design on purpose. These are the first things to review for layering violations or hidden dependencies.

## CLI

```bash
# Print insights as text (or --json)
ax insights
ax insights --resolution 1.4 --god-limit 30 --surprising-limit 30
ax insights --json

# Full Markdown report → AX_REPORT.md
ax report
ax report --out docs/ARCHITECTURE.md
ax report --stdout
```

The report includes a god-node table, communities with member counts, surprising connections, dead code, an unresolved-refs summary, and a set of **suggested questions** templated from your top god nodes and communities (e.g. *"How does `AuthService` connect to the ship pipeline?"*) — good prompts to hand straight to an agent.

Community assignments are cached in the index (`node_communities` table) and only recomputed after `ax index` / `ax sync`, or when you pass a different `--resolution`.

## MCP tools

Agents get the same analysis without shelling out:

| Tool | Returns |
|---|---|
| `ax_insights` | `{ communities, godNodes, surprisingConnections }` — params `resolution`, `godLimit`, `surprisingLimit` |
| `ax_report` | `{ markdown }` — the full report as a string; param `resolution` |

Use `ax_insights` for a fast architectural overview before diving into a large unfamiliar codebase, and `ax_report` when you want a durable, shareable summary.

## Visual graph

The Command Center **Graph** page renders the graph as an interactive force-directed layout:

- **Node color = community** — subsystems separate visually.
- **Node size = degree** — god nodes pop.
- **Edge style = confidence** — solid `extracted`, dashed `inferred`, dotted `ambiguous`.
- **Docs = squares** — `.md`/`.mdx` files stand apart from code (circles).
- Click any node to open the existing detail panel; pan, zoom, and drag to explore.

Open it with `ax web --open` and pick **Graph** in the sidebar. Use the node-count selector to cap large graphs and **Recompute communities** to re-run detection.

### Portable export

For sharing outside the Command Center — a PR attachment, a wiki, or a committed artifact — export a single self-contained HTML file:

```bash
ax export graph-html --out graph.html
ax export graph-html --limit 1500 --resolution 1.2
```

The file inlines the graph data and a small renderer, so it opens in any browser with no server.

## Edge confidence

Every edge carries a confidence tag alongside its provenance:

| Confidence | Meaning | Rendered |
|---|---|---|
| `extracted` | Read directly from the tree-sitter AST | solid |
| `inferred` | Resolved by a heuristic / name-matching / framework pass | dashed |
| `ambiguous` | Several candidate targets existed; the resolver picked one | dotted |

It shows up as a badge in the Command Center edge lists and in `ax_node` / `ax_callers` / `ax_callees` output, so you can weigh how certain a relationship is before acting on it. See [The Knowledge Graph](/core-concepts/knowledge-graph/#edge-confidence).

## Docs in the graph

Markdown files are indexed as `doc` nodes, so READMEs, ADRs, and design notes become first-class graph citizens. Relative `[text](./other.md)` links become `extracted` `references` edges between docs; inline `` `code` `` spans that match a symbol name become `inferred` `references` edges from the doc to that code. This lets you ask "what docs mention `AuthService`?" and see documentation next to the code it describes.
