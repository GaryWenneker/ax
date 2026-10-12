# Logging and documentation audit

Isolation: fresh clone, feature branch `fix/logging-docs-audit`; production is read-only.
Spec approval: not obtained (autonomous run under the user's request to fix and complete).

## Acceptance criteria
1. Logging has one visible H1 and one project/live summary. Duplicate project/quality strips do not consume page height; quality and source path remain accessible.
2. Date/tool/search and kind filters, resizable/sortable columns, live arrivals, keyboard navigation and event inspector retain existing behavior at 393, 900, 1366 and 2048 px.
3. Inventory every Command Center route and Settings section plus CLI, MCP and web API surfaces. Map each to current website documentation; fix omissions and stale claims.
4. Refresh public walkthrough screenshots from the compiled UI with synthetic data. Record provenance and validate referenced assets and document links. Inspect representative desktop/mobile images.
5. Create and persist `ax-audit-documentation` with a repeatable evidence-based audit and second-pass workflow. Apply it to the finished changes, fix findings and rerun checks.

## Setup and validation
Use existing npm lockfiles and installed Node; install their locked frontend/site dependencies. No new production dependency, Rust/Docker build, Netlify build/deployment, release or workflow dispatch. Run web UI build/unit tests, relevant Playwright tests, site tests, documentation audit and skill validator. Save SPEC, audit matrix and EVIDENCE under this directory. Source/deployed versions must be distinguished; synthetic screenshot success does not prove backend behavior.
