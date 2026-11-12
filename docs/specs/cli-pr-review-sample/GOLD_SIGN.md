# Gold sign-off (required to freeze baseline)

## Status

**Signed** by user (`sign gold`) at 2026-11-12T10:47:12.064743+00:00. Baseline frozen at `baseline/metrics.json`.


Auto-derived per QUALITY_PLAN §4.1 from `gh` code paths (`.rs/.ts/.js/.ps1/…`). Symbols curated only for pr-251.

## pr-251 — Release 8.4.6: Windows install resolve + park retry
- PR: https://github.com/GaryWenneker/ax/pull/251
- files (5):
  - `crates/ax-cli/src/commands/upgrade.rs`
  - `install.ps1`
  - `site/public/install.ps1`
  - `site/tests/install-script.test.js`
  - `site/tests/install-script.test.ts`
- symbols: `upgrade_cmd_batch`, `activate_staged_install`, `Test-AxReleaseAsset`, `Disable-AxStaleGitHubTokens`, `Clear-AxPendingUpgradeState`

## pr-250 — Release 8.4.5: preserve policy + CLI lifecycle
- PR: https://github.com/GaryWenneker/ax/pull/250
- files (19):
  - `.agents/rules/author-skill-rule.mdc`
  - `.agents/rules/degraded-continue.mdc`
  - `.agents/rules/explore-before-grep.mdc`
  - `.agents/rules/policy-capture.mdc`
  - `crates/ax-cli/src/commands/init.rs`
  - `crates/ax-cli/src/commands/policy.rs`
  - `crates/ax-cli/src/main.rs`
  - `crates/ax-global-db/src/policy.rs`
  - `crates/ax-installer/src/templates/cli_only_cursor_rule.mdc`
  - `crates/ax-policy/src/ide_seed.rs`
  - `crates/ax-policy/src/lib.rs`
  - `crates/ax-policy/src/pack.rs`
  - `crates/ax-policy/src/seed.rs`
  - `crates/ax-policy/templates/ide/cursor/ax.mdc`
  - `crates/ax-policy/templates/rules/author-skill-rule.mdc`
  - `crates/ax-policy/templates/rules/degraded-continue.mdc`
  - `crates/ax-policy/templates/rules/english-only.mdc`
  - `crates/ax-policy/templates/rules/explore-before-grep.mdc`
  - `crates/ax-policy/templates/rules/policy-capture.mdc`
- symbols: _(none — file-only gold)_

## pr-248 — Release 8.4.3: installer self-heal
- PR: https://github.com/GaryWenneker/ax/pull/248
- files (6):
  - `.agents/rules/install-auto-self-heal.mdc`
  - `install.ps1`
  - `install.sh`
  - `scripts/test_install_self_heal.py`
  - `site/public/install.ps1`
  - `site/public/install.sh`
- symbols: _(none — file-only gold)_

## pr-247 — Harden Windows install release resolution diagnostics
- PR: https://github.com/GaryWenneker/ax/pull/247
- files (2):
  - `install.ps1`
  - `site/public/install.ps1`
- symbols: _(none — file-only gold)_

## pr-245 — Release 8.4.2: CLI-only for all IDEs
- PR: https://github.com/GaryWenneker/ax/pull/245
- files (15):
  - `.agents/rules/cli-mcp-parity-ide-rules.mdc`
  - `.agents/rules/degraded-continue.mdc`
  - `.agents/rules/explore-before-grep.mdc`
  - `.agents/rules/mcp-callmcp-shape.mdc`
  - `.agents/rules/prefer-mcp-ops.mdc`
  - `.agents/rules/subagents.mdc`
  - `crates/ax-installer/src/cli_only.rs`
  - `crates/ax-policy/src/ide_seed.rs`
  - `crates/ax-policy/templates/ide/cursor/ax.mdc`
  - `crates/ax-policy/templates/rules/degraded-continue.mdc`
  - `crates/ax-policy/templates/rules/explore-before-grep.mdc`
  - `crates/ax-policy/templates/rules/mcp-callmcp-shape.mdc`
  - `crates/ax-policy/templates/rules/old-coder-mandatory.mdc`
  - `crates/ax-policy/templates/rules/prefer-mcp-ops.mdc`
  - `crates/ax-policy/templates/rules/subagents.mdc`
- symbols: _(none — file-only gold)_

## pr-243 — CLI-MCP parity, LCM audit fix, and 8.4.0
- PR: https://github.com/GaryWenneker/ax/pull/243
- files (32):
  - `crates/ax-cli/src/commands/cache_cmd.rs`
  - `crates/ax-cli/src/commands/doctor.rs`
  - `crates/ax-cli/src/commands/durable_cmd.rs`
  - `crates/ax-cli/src/commands/init.rs`
  - `crates/ax-cli/src/commands/install.rs`
  - `crates/ax-cli/src/commands/mod.rs`
  - `crates/ax-cli/src/commands/preflight.rs`
  - `crates/ax-cli/src/commands/session_cmd.rs`
  - `crates/ax-cli/src/commands/turn_cmd.rs`
  - `crates/ax-cli/src/installer/mod.rs`
  - `crates/ax-cli/src/main.rs`
  - `crates/ax-cli/tests/cli_parity_gaps.rs`
  - `crates/ax-cli/tests/cli_parity_turn.rs`
  - `crates/ax-installer/src/cli_only.rs`
  - `crates/ax-installer/src/lib.rs`
  - `crates/ax-installer/src/templates/cli_only_cursor_rule.mdc`
  - `crates/ax-usage/src/activity.rs`
  - `crates/ax-usage/src/lib.rs`
  - `crates/ax-usage/src/store.rs`
  - `crates/ax-usage/src/turn_lifecycle.rs`
  - `crates/ax-web/web-ui/dist/assets/index-DzLcDlqd.js`
  - `crates/ax-web/web-ui/src/App.tsx`
  - `crates/ax-web/web-ui/src/components/McpTraceLive.tsx`
  - `crates/ax-web/web-ui/src/lib/loggingPresentation.test.ts`
  - `crates/ax-web/web-ui/src/lib/loggingPresentation.ts`
  - `crates/ax-web/web-ui/src/lib/mcpTrace.ts`
  - `crates/ax-web/web-ui/src/lib/mcpTraceEvents.ts`
  - `crates/ax-web/web-ui/src/lib/routes.test.ts`
  - `crates/ax-web/web-ui/src/lib/routes.ts`
  - `crates/ax-web/web-ui/src/pages/Logging.tsx`
  - `crates/ax-web/web-ui/src/pages/Memory.tsx`
  - `docs/specs/cli-mcp-parity/examples/cli-only-turn.ps1`
- symbols: _(none — file-only gold)_

## pr-121 — Fix legacy AX brand name to AgentXpansion
- PR: https://github.com/GaryWenneker/ax/pull/121
- files (3):
  - `crates/ax-web/web-ui/src/lib/brand.ts`
  - `site/src/lib/brand.d.ts`
  - `site/src/lib/brand.ts`
- symbols: _(none — file-only gold)_

## pr-120 — Persist installer PATH for fresh macOS and Linux shells
- PR: https://github.com/GaryWenneker/ax/pull/120
- files (4):
  - `install.sh`
  - `scripts/gauntlet-install-shell-path.sh`
  - `site/public/install.sh`
  - `site/tests/install-script.test.ts`
- symbols: _(none — file-only gold)_

## pr-119 — Withdraw the STE-inspired preflight experiment
- PR: https://github.com/GaryWenneker/ax/pull/119
- files (18):
  - `crates/ax-mcp/examples/evaluate_ste_output.rs`
  - `crates/ax-mcp/src/context_budget.rs`
  - `crates/ax-mcp/src/invoke.rs`
  - `crates/ax-mcp/src/lib.rs`
  - `crates/ax-mcp/src/server.rs`
  - `crates/ax-mcp/src/smart_output.rs`
  - `crates/ax-mcp/src/text_style.rs`
  - `crates/ax-mcp/src/tools.rs`
  - `crates/ax-mcp/tests/preflight_standard_contract.rs`
  - `crates/ax-mcp/tests/ste_output_contract.rs`
  - `tools/check_mcp_removal_coverage.py`
  - `tools/evaluate-ste-output.py`
  - `tools/gauntlet-ste-output.sh`
  - `tools/gauntlet-ste-removal.sh`
  - `tools/ste_output_mutants.py`
  - `tools/ste_removal_mutants.py`
  - `tools/test_evaluate_ste_output.py`
  - `tools/test_mcp_removal_coverage.py`
- symbols: _(none — file-only gold)_

## pr-118 — Add opt-in STE-inspired preflight instructions
- PR: https://github.com/GaryWenneker/ax/pull/118
- files (15):
  - `crates/ax-mcp/examples/evaluate_ste_output.rs`
  - `crates/ax-mcp/src/context_budget.rs`
  - `crates/ax-mcp/src/invoke.rs`
  - `crates/ax-mcp/src/lib.rs`
  - `crates/ax-mcp/src/server.rs`
  - `crates/ax-mcp/src/smart_output.rs`
  - `crates/ax-mcp/src/text_style.rs`
  - `crates/ax-mcp/src/tools.rs`
  - `crates/ax-mcp/tests/ste_output_contract.rs`
  - `tools/check_ste_output_coverage.py`
  - `tools/evaluate-ste-output.py`
  - `tools/gauntlet-ste-output.sh`
  - `tools/ste_output_mutants.py`
  - `tools/test_evaluate_ste_output.py`
  - `tools/test_ste_output_coverage.py`
- symbols: _(none — file-only gold)_

