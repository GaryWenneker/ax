---
title: Architecture Insights
description: Communities, god nodes, surprising connections, the architecture report, and the interactive graph — from one analysis engine.
---

**ax v2.1.8+** turns the knowledge graph into a map of your architecture. One analysis engine powers three surfaces: the `ax insights` / `ax report` CLI commands, the `ax_insights` / `ax_report` MCP tools, and the Command Center **Graph** page. It works entirely on your local index — no LLM calls required.

![Interactive Graph — Leiden communities, god nodes, confidence-tagged edges, and doc nodes](/screenshots/cc-graph.png)

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

Open it with `ax web --open` and pick **Graph** in the sidebar.

**Start here (onboarding, no LLM).** The left panel lists Leiden **subsystems**, a **god-node tour** (Prev/Next), and **Ask the graph** prompts (the same templates as `ax report`). Click a subsystem to hide the rest of the canvas. These clusters are code coupling, not business processes.

**Domain view (opt-in overlay).** Toggle **Structure / Domain** in the Graph toolbar. Domain is a horizontal graph of `domain` → `flow` → `step` read from `.ax/domain-graph.json`. It does **not** change `ax.db`. Ask an agent to run the `domain` skill (or write the JSON yourself). `GET`/`PUT /api/domain-graph` load and save the file. Empty overlay → empty canvas plus a hint. The skill is seeded on `ax init` as `.ax/policy/skills/domain/SKILL.md`.

Controls:

- **Structure / Domain** toggle
- **Search / Kind / Community** filters — focus a subsystem or symbol kind
- **Density** slider — tighten or loosen the force layout (structure view)
- **Node-count selector** — cap large graphs (e.g. 100 of N nodes)
- **Recompute communities** — re-run Leiden detection after index changes
- **Reload overlay** — re-read `.ax/domain-graph.json` in Domain view

![Interactive Graph — Leiden communities, god nodes, confidence-tagged edges, and doc nodes](/screenshots/cc-graph.png)

### Portable export

**Command Center → Graph:** use the toolbar **Export** format + **Download** (and **Copy** for Mermaid / PlantUML / DOT / Cypher text) to save the current density slice as JSON, DOT, GraphML, GEXF, Cypher, Mermaid, or PlantUML (`GET /api/graph/export?format=…&limit=…`). The control shows node/edge counts and notes when the slice is truncated vs the full CLI HTML export.

For a self-contained interactive HTML file (PR attachment, wiki, or committed artifact) use the CLI:

```bash
ax export graph-html --out graph.html
ax export graph-html --limit 1500 --resolution 1.2
ax export graph --format graphml --out graph.graphml
```

The HTML export inlines the graph data and a small renderer, so it opens in any browser with no server.

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
