# ax v4 — Missing Features Roadmap (2026–2027)

This roadmap covers **net-new** capabilities shipped in **v4.0.0** (branch `ax-v4`, merged to `main`).

## Timeline

| Window | Deliverables |
|--------|----------------|
| **Q3 2026** | Monorepo workspace federation + `ax policy pull`; multi-format graph export |
| **Q4 2026** | WASM plugin host; ONNX dense vectors; `ax ship --ci` + test runners |
| **Q1 2027** | LSP bridge; Command Center share / PWA / live stream |

## Q3–Q1 MVP status (released in v4.0.0)

Federation, export formats, plugins, `--ci`, LSP CLI, share/PWA/actions, harden UI, and pro polish (ax Mint + Settings cards) are in **v4.0.0**.

## Harden & Extend

UI, Logging/Quality, and deepen thin MVPs:

- [x] StatusBar Activity chip (replace floating ActionStream)
- [x] Share Settings + themed token gate + badge
- [x] PWA icons + network-first SW + dismissible install hint
- [x] Unresolved → LSP enrich (ModalShell + `/api/lsp/enrich`)
- [x] ONNX tokenizer path + embed-status API
- [x] Plugins list API + Settings note
- [x] Reusable `.github/workflows/ax-ship.yml`
- [x] Verbose domain lines + Logging kind filters + Quality checks (plugin/lsp/ship-ci/…)
- [x] Site docs updated for harden surface

## UI maturity (post-harden)

Thin MVPs that still need a real Command Center surface:

- [x] Multi-format graph export on Graph page (`GET /api/graph/export` + Download)
- [x] Plugins list as its own Settings subsection (`GET /api/plugins` table)
- [x] ONNX / embed-status as its own Settings subsection (paths + feature flag)

## Mobile maturity

Phone / tablet Command Center (share + PWA path). Agent runs Playwright smoke with screenshots — no manual phone check required.

- [x] Fix hamburger dead zone (769–899px) — drawer + menu button align at `--bp-md`
- [x] Status bar: Project + Logging + Activity (+ Share when active) on ≤899px
- [x] Safe-area titlebar/modals; drawer body scroll lock
- [x] Logging / Settings / tables usable at ~390px
- [x] Modal sheets on narrow viewports
- [x] Playwright mobile smoke + `scripts/web-ui-mobile-smoke.ps1` (screenshots for agent)
- [x] Share / Command Center docs: mobile + smoke loop

## Pro polish (post-MVP UI)

- [x] Default **ax Mint** theme (`#3ee4b2`); project-browser tokens track `--accent`
- [x] Unresolved → LSP enrich: limit, server checklist, report panel
- [x] Graph export: node/edge headers, Copy for text formats, density-slice messaging
- [x] Sharing status card + PWA Enable / Install coherence
- [x] Activity chip: relative time, unread, kind badges, meta expand
- [x] Settings: Sharing / Plugins / Embeddings as dedicated cards with Refresh
- [x] **Global mint chrome** — buttons, nav, cards, tables, filters, status bar, share gate, agent terminal all use Open-project outlined mint style

## Extension points

See [EXTENSION_POINTS.md](./EXTENSION_POINTS.md).

## Competitive note — Open Knowledge Format (OKF) vs okf-rs

[okf-rs](https://github.com/jyjeanne/okf-rs) ships a Markdown-first Open Knowledge Format (OKF) toolkit (generate/validate/MCP). ax already covers most of that surface via SQLite + MCP (call graph, explore, insights, LSP enrich, ship), plus ax-only pillars (policy, memory, Command Center).

**Shipped differentiator:** `ax export okf` writes a portable OKF Markdown bundle from `.ax/ax.db`, with relative `okf.outDir` in `ax.json` and optional git-wiki publish (`okf.azdoWiki`). SQLite remains the live query source; OKF is an interoperability/CI projection. See [Open Knowledge Format (OKF)](https://getax.wenneker.io/guides/okf/).

**Non-goals (for now):** DITA bridge, treating the OKF tree as a second query engine, okf-server GraphQL org mesh.
