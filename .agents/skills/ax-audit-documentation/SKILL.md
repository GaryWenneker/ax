---
name: ax-audit-documentation
description: Audit Ax product functionality against its website documentation, repair stale walkthroughs, capture current UI screenshots and run a second verification pass. Use for Ax documentation refreshes, feature coverage reviews, logging/UI changes and screenshot maintenance.
---

# Audit Ax documentation

Read [the Ax surfaces and evidence rules](references/ax-surfaces.md). Work from a fresh main-based feature branch. Read AGENTS.md and use Ax preflight and graph tools where connected. Treat live production as read-only. Keep UI/documentation changes and their evidence in the same PR.

1. Record the source commit, live app version and website version separately. Enumerate routes from `web-ui/src/lib/routes.ts`, Settings from `settingsNavigation.ts`, CLI commands from `ax-cli/src/main.rs`, MCP names from `ax-mcp/src/tools.rs` and HTTP routes from `ax-web/src/workspace_state.rs` (discover the current path if moved). Do not infer completeness from sidebar entries alone.
2. Inspect every visible page and Settings group. Follow meaningful read-only filters, details, comparisons, mobile navigation and empty/error states. Never change production access, tokens, replication, policy or data to obtain screenshots. Mark unavailable/local-host-only features explicitly.
3. Build a feature-to-doc matrix with source path, user task, documentation path/anchor, screenshot and evidence status. Check actual labels, prerequisites, defaults, permissions, outputs and recovery steps. Distinguish full project replication, Git policy pack sync, remote policy sharing, LAN UI sharing and authenticated remote MCP. Edit current guides in place; no per-version updates.
4. Refresh screenshots from the compiled UI using existing capture scripts and synthetic public data. Capture desktop and mobile, important inspectors and configured/unconfigured forms. Preserve theme consistency, hide credentials, wait for content, fail on uncaught errors, missing fixtures or wrong pages, and visually inspect every capture. Record source hash, route, theme, viewport and synthetic/live provenance. Never present fixture metrics as measured results.
5. Run local frontend build and relevant unit/browser tests, site tests and the repository documentation audit. Do not run Netlify builds/deployments, backend/Docker builds for frontend-only changes, or dispatch suspended GitHub workflows.
6. Run a second pass from the finished diff: rediscover surfaces, check matrix coverage, doc links/assets, screenshot content, source/deployed drift and examples. Fix each finding and rerun affected checks. Repeat review until a pass has zero unresolved findings; blocked checks remain blocked and cannot be called passed.
7. Persist the matrix and rerunnable EVIDENCE in `docs/specs/<task>/`. Include exact commands/results, review rounds, omissions and deployment status. Push the feature branch and create/update its PR. Merge only under existing user authorization and applicable passing gates. Never claim a running server changed merely because source merged.

When asked to propose this workflow, explain its three evidence sources (code, observed UI, docs), feature matrix, fresh screenshot provenance and final independent audit in a short paragraph. Do the authorized work without adding an approval gate for ordinary reversible edits.
