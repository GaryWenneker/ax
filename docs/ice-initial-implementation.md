# Intelligent Context Engine: initial opt-in slice

This branch adds deterministic evidence selection for MCP preflight without any additional AI model, embedding endpoint, or network request.

## Enabling it

Pass `"contextMode": "compact"` or `"contextMode": "expanded"` to `ax_preflight`, or set `AX_ICE_ENABLE=1` on the MCP server to enable the default `auto` behavior. Pass `"contextMode": "off"` to disable it for a call. ICE remains opt-in until the cross-platform quality and retrieval benchmarks pass.

Every source result is read exclusively from the hash-checked `.ax/ax.db` source store. Manifest files admitted in this slice: `package.json`, `Cargo.toml`, `tsconfig*.json` and `*.csproj`. Arbitrary JSON, `appsettings.json`, `.env` and other secret-bearing files are **not** admitted by this change.

The returned `sourceContext` object lists evidence paths, line ranges, indexed freshness, reason, warnings, and block IDs. Actual numbered source snippets appear only in budget-selected `inject` blocks; `contextBudget.included` states which IDs were delivered. Existing required policy always takes precedence.

Existing approved, matched policies can optionally provide retrieval hints using the existing policy `properties` map:

```yaml
---
id: auth-context
level: INFO
triggers: [inloggen]
properties:
  contextTerms: [refreshToken]
  contextPaths: [src/auth/refresh.ts]
---
Prefer the indexed authentication implementation.
```

Hints affect retrieval only and **do not create graph relationships**. Graph neighbors must have existing resolved edges with non-ambiguous confidence.

## Limitations and unfinished work

This PR is an **initial implementation**, not the complete ICE specification. The following work remains mandatory before default rollout:

- A reproducible baseline and 60–100 gold-labeled TypeScript/C#/Rust retrieval questions;
- Positioned config keypaths for JSON, TOML and XML rather than only exact source lines;
- Tested framework-specific relationship extraction, notably .NET DI registrations and Rust trait implementations;
- Deeper intent-aware multi-seed ranking and calibrated retrieval benchmarks;
- Command Center diagnostics/settings and website walkthrough;
- Full Linux/Windows/macOS builds, Clippy, unit/integration/property tests, and real-agent acceptance.

The repository CI workflow currently uses manual `workflow_dispatch`. Do not interpret an empty checks list as a passing build. Keep the PR draft and unmerged until the gates above have been verified.

See the standalone AX Intelligent Context Engine spec for full phased acceptance criteria.
