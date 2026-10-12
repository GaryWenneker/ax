# Ax surfaces and evidence

Repository: `GaryWenneker/ax`. Public documentation source: `site/src/content/docs/`; homepage: `site/src/pages/index.astro`; navigation: `site/astro.config.mjs`; UI: `crates/ax-web/web-ui/`. Live admin starts at `https://admin.wenneker.dev/stats`; public docs are `https://getax.wenneker.io`.

Inventory Policy (Rules, Skills, Memory, Sync, Review, Match, editors), Code (Graph, Search, Nodes, Files), Activity (Logging, Stats, Savings, Unresolved), System (Settings, Prices, Reload MCP) and direct routes such as Sonar. Inspect column resize/sort/filter, pagination and loaded-row limitations, detail blades, source/history, package comparisons, accessibility and mobile behavior.

Settings has General, Connections, Access and Data groups. Enumerate current IDs from source rather than freezing the list here. UI preferences are browser-local; host operations and credentials can require local-owner access. A remote screen showing a local-host restriction is evidence of that restriction, not a successful configuration.

Primary/Replica replication synchronizes project data and requires node credentials. Policy Sync uses a Git pack. Policy sharing pulls GitHub/OneDrive policy. Sharing exposes a token-gated LAN UI/PWA. Remote MCP exposes authenticated project tools over HTTPS. Do not interchange these workflows or their access models.

Use `node site/scripts/audit-documentation.mjs` when present. Use `AX_SCREENSHOT_DEMO=1 node site/scripts/capture-screenshots.mjs` after building web-ui; choose `AX_THEME` and `AX_SHOTS` for targeted refreshes. Use `docs/specs/list-columns/ui-check.mjs` for the shared-list/browser checks when present. Verify the current scripts before running; their names may change.

Automated coverage counts prove that enumerated surface names and links have mapped documents. They do not prove semantic correctness or live backend behavior. Synthetic captures prove current UI rendering only. Live read-only inspection proves displayed behavior only. Record unsupported CLI/API runtime checks rather than hiding them.

Follow delegated router factories when counting HTTP paths. Check same-page and cross-page fragments. Read the screenshot fixture’s explicit theme and viewport; image dimensions can differ for element crops, full-page captures and device pixel ratio. Store imageSize separately, label declared viewport metadata, and never infer the theme from a filename.
