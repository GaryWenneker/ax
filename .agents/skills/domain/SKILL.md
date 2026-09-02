---
name: domain
description: Extract an opt-in business-domain overlay (domains, flows, steps) and save it to .ax/domain-graph.json for the Command Center Domain graph view. Use when the user asks about business logic, domain view, process flows, or understand-domain.
triggers: ["domain", "business logic", "business process", "understand-domain", "domain graph", "process flow"]
tags: ["graph", "domain", "onboarding"]
priority: 70
---
# Domain overlay (opt-in)

The structural knowledge graph in `ax.db` stays deterministic (tree-sitter, no LLM). This skill writes a **separate** overlay that Command Center Graph can show as a horizontal domain → flow → step view.

Do **not** insert domain nodes into the SQLite index. Do **not** call `/understand-domain` from Understand-Anything. Write `.ax/domain-graph.json` (or `PUT /api/domain-graph`).

## When to use

The user wants to **see business logic** (payments, login, onboarding) rather than Leiden communities. Communities are code coupling; this overlay is a human/agent interpretation.

## Workflow

1. Call `ax_insights` (and `ax_explore` / `ax_search` for entry points: HTTP routes, CLI commands, handlers).
2. Group the system into **domains** (bounded contexts), **flows** (user or system processes), and **steps** (ordered actions). Link steps to existing graph node ids when you know them (`codeNodeIds`).
3. Write `.ax/domain-graph.json` with this shape (camelCase):

```json
{
  "version": 1,
  "nodes": [
    {
      "id": "domain:auth",
      "kind": "domain",
      "name": "Authentication",
      "summary": "Sign-in, sessions, tokens.",
      "codeNodeIds": []
    },
    {
      "id": "flow:login",
      "kind": "flow",
      "name": "Login",
      "summary": "User signs in and receives a session.",
      "codeNodeIds": []
    },
    {
      "id": "step:verify",
      "kind": "step",
      "name": "Verify credentials",
      "codeNodeIds": ["function:src/auth.rs:verify"]
    }
  ],
  "edges": [
    { "source": "domain:auth", "target": "flow:login", "kind": "contains_flow" },
    { "source": "flow:login", "target": "step:verify", "kind": "flow_step", "order": 1 }
  ]
}
```

Allowed node kinds: `domain`, `flow`, `step`.  
Allowed edge kinds: `contains_flow`, `flow_step`, `cross_domain`.  
Every edge endpoint must be a node id. Max 2000 nodes / 5000 edges.

4. Tell the user to open Command Center → **Graph** → **Domain**. If the overlay is empty or invalid, the API returns 400 and the UI stays on an empty state.

## Rules

- Overlay only — never replace or rewrite `ax.db`.
- Prefer a small, teachable graph (a handful of domains) over dumping every function as a step.
- If you cannot map a step to a symbol, omit `codeNodeIds` rather than inventing ids.
- Re-run this skill after large refactors; the overlay does not auto-update on `ax sync`.
