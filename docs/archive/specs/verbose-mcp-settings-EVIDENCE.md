# EVIDENCE: Verbose MCP logging on Settings

**SPEC:** `/Users/gary/io/ax/docs/specs/verbose-mcp-settings.md`
**spec approval:** not obtained (autonomous run)
**Tier:** 2
**Source state:** git HEAD `b0676ba29130ed93ea1e8f9db1db9ee5f22e5f82` plus uncommitted working tree (this change is not a standalone commit)
**Entry point:** `/Users/gary/io/ax/tools/gauntlet-verbose-mcp-settings.sh`

## Behavior map

| Scenario | Check |
|---|---|
| S1 Settings has the switch | grep `title="Verbose MCP logging"` + `setUiVerboseMcp` in `Settings.tsx` |
| S2 Logging has no switch | Logging.tsx must not contain `aria-label="Verbose MCP logging"` or `role="switch"`; must `navigateRoute({ page: 'settings' })` |
| S3 docs | README + site docs say Settings → Interface; no `Logging → Verbose MCP logging` |
| Invariant Ship page | `Ship.tsx` has no `verbose_mcp` / Verbose MCP logging |

## Gauntlet (fresh after last edit)

Command: `bash /Users/gary/io/ax/tools/gauntlet-verbose-mcp-settings.sh`

Result: `gauntlet-verbose-mcp-settings: ok`

- S1–S3 + Ship invariant: pass
- `npx tsc --noEmit` in `crates/ax-web/web-ui`: pass (npm warned `Unknown env config "devdir"` only)

### Fail-closed / negative control

- Temp file containing `aria-label="Verbose MCP logging"` is matched by the same grep S2 uses (`negative-control: grep detects forbidden Logging switch string (ok)`).
- Limitation: this proves the grep sees that string; it does not prove every possible leftover switch UI.

### Layers skipped

- Full `cargo test` workspace: skipped (no Rust behavior change in this slice)
- Mutation tool: skipped (placement greps only)
- Coverage fail-under: skipped (no JS test runner for these pages)
- Browser click-through of the toggle: not run (no Cursor browser tools). Substitute: `http://127.0.0.1:7070/` serves `index-BHvamGgD.js` matching `crates/ax-web/web-ui/dist/index.html`. Built bundle contains `Verbose MCP logging` (count 5) and `Enable it under` (count 2).

## Rebuild / install

- `npm run build` in `crates/ax-web/web-ui`
- `CARGO_TARGET_DIR=/Users/gary/io/ax/target-dev cargo build --release -p ax-cli`
- SHA-256 `87fb6f0aa7d1101f4fa13bc2979020003dead5ecd1411de46dd5249c6d648119` for `target-dev/release/ax`, `~/.cargo/bin/ax`, `~/.local/bin/ax`
- `ax --version`: `ax 4.6.0`

## Known limits

- Toggle still writes `[ui].verbose_mcp` in `.ax/ship.toml` (same as before). `AX_MCP_VERBOSE=1` in MCP env still forces logging on and ignores the Settings off state.
- Cursor MCP must be restarted after toggling so the MCP process re-reads ship.toml.
