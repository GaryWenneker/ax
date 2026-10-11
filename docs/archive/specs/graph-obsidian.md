# SPEC: Obsidian-style Graph (Structure view)

Tier 2. Approval: plan "Obsidian-style graph" approved by the user ("Implement the plan as specified").
Isolation: none (working tree, as the plan authorizes; repo already has many uncommitted changes).
New dependency: `d3-force` + `@types/d3-force` — Barnes-Hut force simulation, same engine Obsidian uses; ISC.

## Behaviors (vitest)
- `graphSettings`: `DEFAULT_GRAPH_SETTINGS` has centerForce 0.52, repelForce 10, linkForce 1, linkDistance 250, textFadeThreshold 0, nodeSize 1, linkThickness 1, arrows false, orphans true.
- `loadGraphSettings` returns defaults for missing or corrupt JSON; merges partial JSON over defaults; `saveGraphSettings` round-trips.
- `graphFilter.filterGraph`: search keeps nodes whose name/path contains the query (case-insensitive) and edges between kept nodes; `orphans=false` drops nodes with no kept edge; `attachments=false` drops `doc` nodes; `existingOnly=true` drops nodes with empty `file_path`.
- `graphFilter.groupColor`: first group whose query matches the node wins; no match returns null; empty query never matches.
- `graphHover.buildNeighbors` + `highlightFor(i)`: returns the node, its neighbors, and the incident edge indices; `null` hover returns empty sets.
- `graphForces.seedCluster(n, cx, cy, r)`: every seeded point lies within radius r of (cx, cy); deterministic for the same n.
- `graphForces.forceParams(settings)`: repel strength negative and grows with repelForce; link distance equals linkDistance.

## Behaviors (e2e, `e2e/graph-obsidian.spec.ts`)
- Settings panel opens with Filters, Groups, Display, Forces sections.

## Must NOT
- Domain view, search, pan/zoom/touch, `centerOnNode` behavior unchanged.
- Existing vitest suite stays green.
