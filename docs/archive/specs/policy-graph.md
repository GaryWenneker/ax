# SPEC: Policy graph (rules, skills, memories) in Rules and Skills

Tier 2. Status: waiting for approval.
Your decisions: show rules, skills and memories; open as a full-screen overlay from a **Graph** button in the Rules and Skills page headers. No extra navigation link.

## What you get
- A **Graph** button in the header of the Rules page and the Skills page (next to the existing actions).
- It opens a full-screen overlay with the same Obsidian-style graph as the Graph page: one start cluster, zoom grows nodes, labels fade in and never overlap, thin links, hover in the accent color, and a pulsing ring on the selected node.
- The same **Settings** panel (closed by default, closes on outside click) and a **legend**. The legend shows the node kinds: rule, skill, memory. Global items (from `~/.ax`) get a ring.
- Nodes are the items; edges are the `[[links]]` in their bodies, the same links preflight follows and Obsidian draws.
- Clicking a node selects it and shows a small blade (kind, scope, outgoing links and backlinks), with an **Open** button that goes to that rule, skill or memory. Esc or the close button closes the overlay.
- The item you are on (the open rule or skill) is preselected, so its ring shows where you are.

## Behaviors (tests)
Server, `GET /api/links/graph` (Rust test in `crates/ax-web/tests/links_api.rs`):
- Returns `nodes[]` `{ key, kind, id, label, scope }` for all rules, skills and memories (project and global) and `edges[]` `{ source, target }` for each resolved `[[link]]`.
- A link to an unknown target produces no edge. Duplicate links between the same pair produce one edge. A self-link produces no edge.
- Turn memories are left out (they are raw prompts, as in the link map).

Client, pure (`node --test`):
- `policyGraphModel(payload)` maps nodes and edges to the graph input with kind colors; an edge whose endpoint is missing is dropped.
- `backlinks(key)` and `outgoing(key)` for the blade.

E2E (`e2e/policy-graph.spec.ts`):
- The Rules page has a Graph button; clicking it opens the overlay with a canvas, a legend (rule, skill, memory) and the Settings button.
- Clicking a node opens the blade; Open navigates to that item. Esc closes the overlay.
- The same from the Skills page.

## Must NOT
- No new navigation entry.
- The code Graph page keeps working as it does now (existing e2e tests stay green).

## Technical approach
- Extract the canvas, simulation and drawing from `Graph.tsx` into a reusable `ForceGraphCanvas` component (input: nodes, edges, colors, selected id, callbacks). `Graph.tsx` uses it for the Structure view, and the policy overlay uses it as well, so both graphs behave the same. This is the largest part of the work.
- A new `PolicyGraphOverlay` component, plus the server endpoint in `crates/ax-web/src/links_api.rs`, built on the existing `entries()` and `ax_policy::links::{parse_links, LinkIndex}`.
- No new dependencies.

## Revision 1 (during implementation)
- `ForceGraphCanvas` is a new shared component used by the policy overlay. `Graph.tsx` is **not** migrated onto it in this change: its Structure view has its own Domain view, search dimming, project rings and edge thinning, and migrating all of that would put the working page at risk. Both graphs share the tested modules `graphSettings`, `graphFilter`, `graphHover`, `graphForces` and `graphLabels`. The overlay stores its settings under its own key, `ax.graph.policy.settings`.

## Setup
Isolation: working tree (as before). No commits unless you ask. Gauntlet: cargo test for ax-web links, `node --test`, tsc, e2e (system-chrome), manual mutants added to a script, rebuild + served-bundle check, docs page update.
