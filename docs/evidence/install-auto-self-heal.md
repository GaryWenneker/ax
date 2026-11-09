## Evidence Report — Installer automatic self-heal (Tier 2)

- Spec approval: obtained (user: implementeer / bump / online)
- Spec: `/Users/gary/io/ax/docs/specs/install-auto-self-heal.md`
- Source state: recorded at release commit (see git SHA after merge)
- Toolchain: python3, sh, pwsh (parse check)
- Entry point: `python3 -m unittest scripts.test_install_self_heal -v`
- Independent verification: not performed

### Spec → Test mapping

| Scenario | Test | Status |
|---|---|---|
| Clear stuck Windows upgrade status | `test_ps1_defines_pending_upgrade_clear`, `test_pending_upgrade_fixture_cleanup_contract` | pass |
| Ignore bad GitHub token | `test_ps1_disables_stale_github_tokens_on_auth_failure` | pass |
| Clear invalid AX_VERSION | covered by existing Clear-StaleAxVersionPin + contract that self-heal runs before Resolve | pass (contract) |
| Unix clears upgrade leftovers | `test_sh_clears_pending_upgrade_before_resolve` | pass |
| Public copies stay in sync | `test_public_copies_match_root` | pass |

### Gauntlet layers

| Layer | Command | Result |
|---|---|---|
| Unit / contract | `python3 -m unittest scripts.test_install_self_heal -v` | 5/5 OK |
| Shell syntax | `sh -n install.sh` | OK |
| PowerShell parse | pwsh Parser::ParseFile install.ps1 | OK |
| Full workspace suite | skipped: installer-script change only; not Rust | skipped |
| Mutation | skipped: contract/string-order tests; manual mutants low value | skipped |
| Real execution | after GitHub release: `irm` / `curl \| sh` against published tag | pending release |

### Known limits

- getax.wenneker.io trails GitHub until Netlify deploys site/`install.ps1` and `latest.txt`. Repository raw URLs work immediately after merge.
- User/Machine `GITHUB_TOKEN` clear on Windows may need elevation for Machine scope; Process scope is always cleared after 401/403.
