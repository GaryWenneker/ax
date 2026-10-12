# Ax feature and documentation audit

Source base: `ea51c84567e61f9f2a0287894b2c373c927ac5cb` (main). Live admin displayed Ax 8.1.0. The public Command Center guide retrieved during this audit displayed 8.0.0 and older UI descriptions. Source changes do not deploy either service.

## Coverage matrix

| Surface | Source | Documentation | Audit |
|---|---|---|---|
| 19 UI destinations, including direct Match/editor/Sonar routes | `crates/ax-web/web-ui/src/lib/routes.ts` | `guides/command-center-walkthrough.md`, page destination table | Enumerated; live sidebar pages inspected read-only; local rendering verified |
| 14 Settings sections across General, Connections, Access and Data | `crates/ax-web/web-ui/src/lib/settingsNavigation.ts`, section components | Walkthrough Settings table; linked feature guides | Every section inspected; local-owner restrictions documented |
| Policy Rules/Skills, Match, packs, Review | Policy UI and `crates/ax-web/src/policy.rs` | `guides/policy-engine.md`, `guides/policy-sharing.md` | Current controls/captions, broken storage anchor and review screenshots corrected |
| Memory, vault, embeddings, OKF | Memory page and Settings section components | `guides/memory.md`, `guides/obsidian-vault.md`, `guides/okf.md` | Available controls mapped; local mount access explained; synthetic captures |
| Code Graph, Search, Nodes, Files, Unresolved | Code pages and graph API router | Core concepts, indexing, resolution, architecture insights | Labels/captions match current captured screens; backend extraction not rerun |
| Logging, Quality, Savings, Prices | `McpTraceLive.tsx`, activity pages | `guides/mcp-quality.md`, `guides/token-savings.md`, Command Center | Duplicate logging bars removed; always-on logging and timezone defaults corrected |
| Replication/access/remote MCP/sharing | Settings components, access and remote API routers | `guides/standalone-ax.md`, `guides/remote-mcp.md`, `guides/share.md` | Distinct workflows retained; fixture-backed permissions/setup/recovery checked |
| 60 public top-level CLI command names | `crates/ax-cli/src/main.rs` | `reference/cli.md` | Every public name mentioned; hidden serve/prompt/session hooks exempt |
| 38 MCP tool names | `crates/ax-mcp/src/tools.rs` | `reference/mcp-server.md` | Added missing `ax_budget`; corrected tracing compatibility flag |
| 174 mounted HTTP paths | `workspace_state.rs` and delegated module router factories | `reference/web-api.md` | Added omitted access, login, Sonar, live refresh and 12 publication paths |
| 40 documentation files | `site/src/content/docs/` | Current product guidance in place | File links, same-page/cross-page fragments and screenshot hashes checked |
| Current images | Capture scripts and browser screenshot hooks | `site/public/screenshots/manifest.json` | Synthetic/public data; bundle hash, image hash, route/state/theme; crops use separate image size |

## Observed limits

The live remote admin exposes local-owner restrictions for host exports and WebDAV drive connection; no production settings, credentials, policy or data were changed to bypass them. Source/fixture inspection does not execute real authentication, CLI operations, Primary/Replica synchronization, all command flags or arbitrary HTTP bodies. Structural coverage checks names/paths and document links, not semantic correctness of every sentence. Browser tests and manual image review provide separate evidence about UI behavior. Public publication remains the owner's deployment process.

## Review repairs

The independent skill audit found delegated routing omissions, silent screenshot request failures, incomplete source/state provenance, legacy logging-toggle instructions, differing host/browser timezone defaults, root-relative and same-page broken fragments, overclaiming captions and a duplicate Light/Dark image. The final review additionally corrected Light screenshot theme metadata, separated crop dimensions from declared fixture viewport, and repaired a remote-host fixture that showed a schema error in its screenshot.
