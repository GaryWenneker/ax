# SPEC: Live updates across Command Center

Tier 2 (UI + one internal endpoint). Spec approval: pending.

## Goal

Every Command Center page shows new and changed data without F5. New items are
easy to notice: a spark on the graph and a glow on list rows.

## Design

1. **One change feed.** A new internal SSE endpoint, `GET /api/changes`, sends
   `{ topic, version }` whenever a data area changes. Topics: `graph`, `memory`,
   `rules`, `skills`, `usage` (logging, stats, savings, prices), `ship`.
   - The server checks the project `ax.db` and `~/.ax/global.db` once per second
     (SQLite `PRAGMA data_version` plus file mtime) and sends a topic only when
     its version changed. Nothing is sent when nothing changed.
   - It reuses `sharedEventSource`, so all pages share one connection.
2. **One page hook.** `useLive(topic, reload)` reloads a page's data when its
   topic changes. Reloads are debounced by 400 ms, so a burst of writes causes
   one reload. The page keeps its scroll position, selection, open editors and
   filters.
3. **New-item detection on the client.** `diffNewKeys(before, after, keyOf)`
   returns keys that are in the new list and not in the old one. The first load
   of a page never counts as new.
4. **Glow on lists.** New rows get the class `live-new`: a soft accent outline
   and background that fade out over 2.5 s. With `prefers-reduced-motion` it is
   a static outline for 2.5 s, without animation.
5. **Spark on the graph.** A new node gets a spark: two expanding accent rings
   and a short bright core, 1.2 s, drawn on the graph canvas. If the node is
   off-screen, a small edge marker points toward it for 3 s. Several new nodes
   at once each get a spark, capped at 20 per update so a large re-index does
   not flood the view (a re-index shows one "N new nodes" toast instead).
6. **Style:** colors come from the theme accent; no confetti or bounce.
   Text contrast rules (WCAG) still apply.

## Behaviors (tests)

- `diffNewKeys([], [a,b])` on first load returns nothing.
- `diffNewKeys([a], [a,b])` returns `[b]`; removed items are never "new".
- Debounce: 5 topic events within 400 ms cause exactly 1 reload.
- `useLive` ignores topics other than its own.
- Server: writing a memory to `ax.db` produces exactly one `memory` event
  within 2 s; an idle database produces no events for 5 s.
- Graph: a node id added between two loads gets a spark; existing nodes do not.
- Spark cap: 50 new nodes trigger 20 sparks and one summary toast.
- Reload keeps the selected row and scroll position (Rules, Memory, Nodes).

## Pages covered

Rules, Skills, Memory, Sync, Review, Graph, Search results, Nodes, Files,
Logging, Stats, Savings, Unresolved, Settings, Prices, and the status bar
counters.

## Must not

- Reload while the user is editing a form; changes wait until it is saved or
  closed.
- Move the graph camera or reset the physics layout on a reload.
- Add polling per page (one feed only).
- Add new npm or crate dependencies.

## Setup

- No new dependencies. Isolation: a branch, `live-updates`.
- Files added: `crates/ax-web/src/changes.rs`,
  `crates/ax-web/web-ui/src/lib/live.ts`, `lib/live.test.ts`, and a spark
  helper in the graph canvas code.
- Docs: Command Center guide gets a short "Live updates" section.
- Gauntlet: `cargo test -p ax-web`, clippy `-D warnings`, `node --test` for the
  web UI, `npm run build`, and a real run on http://127.0.0.1:7070 (add a
  memory, watch it glow; index a new file, watch the graph spark).

## Revision 1 (during implementation)

- **No `ship` topic.** The Ship page already streams its own events
  (`/api/ship/events`); a second signal would duplicate it.
- **Logging and Settings are not wired.** Logging is already a live stream.
  Settings reads `.ax/ship.toml` and UI settings, which no feed topic covers.
- **Scope added to `useNewKeys(keys, scope)`.** Without it, changing a
  filter, search, page, or opening a folder made every row glow. A new scope
  sets a baseline and marks nothing new (tests in `lib/live.test.ts`).
- **Polling removed, not added.** Memory (5 s) and the status bar (30 s) had
  timers; both now listen to the feed.
- **Graph merge, not reload.** A `graph` change fetches the graph and adds only
  unknown nodes and edges next to a linked node; the camera and layout stay.
- **Known gap:** the graph shows the top *N* nodes by degree (50 to 600). A
  new node outside that set is not drawn, so it gets no spark.
