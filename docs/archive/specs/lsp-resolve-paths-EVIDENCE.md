# EVIDENCE — LSP resolve working binaries

- Spec: `/Users/gary/io/ax/docs/specs/lsp-resolve-paths.md`
- Spec approval: not obtained (autonomous run)

## Mapping

| Behavior | Check |
|---|---|
| L1 shim not available | `servers::tests::l1_*` |
| L2 working wins | `l2_working_binary_wins_over_shim` |
| L3 node_modules/.bin | `l3_node_modules_bin_is_searched` |
| L4 status uses project root | `lsp_api.rs` `discover_servers_in(Some(&root))` |

Environment: `rustup component add rust-analyzer` (2026-09-16). `typescript-language-server` added as web-ui devDependency.

## Gauntlet

```
cargo test -p ax-lsp --offline --lib
# 8 passed; 0 failed; 1 ignored
```

Skipped: mutation, coverage fail-under, cargo-audit (no new crates; npm added typescript-language-server).
