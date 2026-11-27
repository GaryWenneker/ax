# Evidence: fix/review-delivery

Scope: installers (`install*.sh`, `install*.ps1` and their `site/public` copies), `scripts/`, `tools/`, `.github/workflows/`, `slim/`, Dockerfiles, `render.yaml`, `site/` code and tests, `telemetry-worker/`.

Base commit: `9dca60e4`. The branch changes no Rust source.

## Findings

Every finding is fixed. "RED" means the covering test failed before the fix.

| Finding | State | Covering tests | Commit |
|---|---|---|---|
| delivery-001 | fixed (RED) | `site/tests/install-script.test.ts`: uninstall keeps `~/.ax` data; `--purge` needs confirmation and refuses unsafe directories | 4ad510b2 |
| delivery-002 | fixed (RED) | `scripts/test_workflows.py`: WorkflowTriggers, PagesDeployment | e0792f90 |
| delivery-003 | fixed (RED) | `scripts/test_workflows.py`: ActionPins (SHA pins, dependabot), Permissions | fa83f9ee |
| delivery-004 | fixed (RED) | `site/tests/install-ps1.test.ts` (rate-limit 403, rejected token, no environment writes); `site/tests/install-script.test.ts` (Windows tokens unchanged on rate or auth failure); `scripts/test_install_self_heal.py` | 1ebbe641 |
| delivery-005 | fixed (RED) | `site/tests/install-script.test.ts` (failed download keeps install); `site/tests/install-ps1.test.ts` (offline install, swap without staging leftovers); `scripts/test_install_self_heal.py` | 65157720 |
| delivery-006 | fixed (RED) | `site/tests/install-script.test.ts` (bad, missing, mirror-tampered checksum; GitHub-down fallback); `site/tests/install-ps1.test.ts` (wrong checksum, no entry, no SHA256SUMS) | 153cd59a |
| delivery-007 | fixed (RED) | `scripts/test_dependency_age.py` LockfileCoverageTests; `scripts/test_lcm_autofix.py`; `scripts/test_dependency_advisories.py` | 675e402b |
| delivery-008 | fixed (RED) | `scripts/test_gauntlet_clippy_filter.py` (7 tests) | e625d116 |
| delivery-009 | fixed (RED) | `scripts/test_neural_realworld_metrics.py` (7 tests) | 0d933bfa |
| delivery-010 | fixed (RED) | `site/tests/admin-auth.test.ts` | d2effabc |
| delivery-011 | fixed (duplicate of delivery-006) | `site/tests/install-script.test.ts`, `site/tests/install-ps1.test.ts` | 153cd59a |
| delivery-012 | fixed (RED) | `site/tests/azure-deploy.test.ts`: startup probe waits long enough, within Azure limits | eef6549c |
| delivery-013 | fixed (RED) | `scripts/test_restore_on_exit.py` | 0c7e30d9 |
| delivery-014 | fixed (RED) | `scripts/test_workflows.py` ExpressionInjection | b381f665 |
| delivery-015 | fixed (RED) | `scripts/test_docker_build.py` | c2d18b45 |
| delivery-016 | fixed (RED) | `scripts/test_install_slim.py` | 75fa3b05 |
| delivery-017 | fixed (RED) | `site/tests/install-ps1.test.ts`, `site/tests/install-script.test.ts` (mixed tag ranking) | 88ee9c50 |
| delivery-018 | fixed (RED) | `scripts/test_repo_hygiene.py` | 61bb5bff |
| delivery-019 | fixed (RED) | `scripts/test_bench_live_arm.py` | cd9e5917 |
| delivery-020 | fixed (RED) | `scripts/test_bench_mcp_client.py` | 5f919344 |
| delivery-021 | fixed (RED) | `scripts/test_ops_scripts.py` | d399f780 |
| delivery-022 | fixed (RED) | `scripts/test_mutant_lists.py` | 4766bd01 |
| delivery-023 | fixed (RED) | `scripts/test_changed_coverage.py` | 2a678a9a |
| delivery-024 | fixed (RED) | `scripts/test_cloudflare_tunnel.py` (5 tests) | 77f93821 |
| delivery-025 | fixed (RED) | `scripts/test_check_cli_docs.py` | f0a953ff |
| delivery-026 | fixed (RED) | `scripts/test_gauntlet_lib.py` (12 tests); partial gauntlet runs | 78c93a76 |
| delivery-027 | fixed (RED) | `scripts/test_restore_on_exit.py` (MutatingScriptsUseTheGuard, relative path, scratch cleanup); manual SIGINT | 732d84ec |
| delivery-028 | fixed (RED) | `scripts/test_restore_on_exit.py` MutateSpecRegistry; real run killed 2/2 | 91363fdf |
| delivery-029 | fixed (RED) | `scripts/test_neural_agent_scripts.py` NeuralAgentLive | 44f51a96 |
| delivery-030 | fixed (RED) | `scripts/test_neural_agent_scripts.py` NeuralAgentSuite | b32df031 |
| delivery-031 | fixed (RED) | `scripts/test_neural_coverage.py` | 28c01d9b |
| delivery-032 | fixed (RED) | `scripts/test_neural_realworld_agent.py` | 149417e4 |
| delivery-033 | fixed (RED) | `scripts/test_neural_realworld_agent.py` (timed-out agent keeps stderr) | 149417e4 |
| delivery-034 | fixed (RED) | `scripts/test_neural_realworld_daemon.py`, `scripts/test_neural_realworld_agent.py` | 9d26cdce |
| delivery-035 | fixed (RED) | `scripts/test_mutant_runners.py`, `scripts/test_restore_on_exit.py` | 579046a1 |
| delivery-036 | fixed (RED) | `scripts/test_mutant_runners.py` | 579046a1 |
| delivery-037 | fixed (RED) | `scripts/test_mutants_lib.py` | d590824e |
| delivery-038 | fixed (RED) | `scripts/test_release_tag.py` | e6933919 |
| delivery-039 | fixed (RED) | `scripts/test_no_history_rewrite.py` | 962cc797 |
| delivery-040 | fixed (RED) | `scripts/test_release_commands.py` | f8bd4e36 |
| delivery-041 | fixed (RED by mutation) | `scripts/test_install_self_heal.py` | 795d7651 |
| delivery-042 | fixed (RED) | `site/tests/admin-telemetry.test.ts`, `site/tests/admin-auth.test.ts` | 263d734b |
| delivery-043 | fixed (RED) | `site/tests/screenshot-provenance.test.ts` | 936dc6af |
| delivery-044 | fixed (RED) | `site/tests/screenshot-writers.test.ts` | 59cd0df0 |
| delivery-045 | fixed (RED) | `site/tests/component-reachability.test.ts` | ee06bcaa |
| delivery-046 | fixed (RED) | `site/tests/home-minimal.test.ts`; build with `SITE_BASE=/ax` | ac01970a |
| delivery-047 | fixed (RED) | `site/tests/admin-pages.test.ts`; Puppeteer check under the netlify.toml CSP | 5ea33875 |
| delivery-048 | fixed (RED) | `site/tests/latest-request.test.ts`; Puppeteer race check | 834fb8be |
| delivery-049 | fixed (RED) | `site/tests/admin-auth.test.ts` (cookie, bearer refused, cross-origin refused, sign-out), `site/tests/admin-pages.test.ts` | 17c3e1ea |
| delivery-050 | fixed (RED, manual) | `TMPDIR=<empty dir> node --test tests/azure-deploy.test.ts tests/latest-release.test.ts` leaves the directory empty | 9df2b7be |
| delivery-051 | fixed (RED) | `site/tests/home-minimal.test.ts` (as-written and reformatted sheets), `site/tests/design-coherence.test.ts` | 56994630 |
| delivery-052 | fixed (RED) | `site/tests/install-script.test.ts`: every shell run has a timeout | 800bdb1a |
| delivery-053 | fixed (RED) | `scripts/test_docker_build.py` SlimBuildContext | b8f1a38a |
| delivery-054 | fixed (RED) | `site/tests/azure-deploy.test.ts`: slim README Koyeb wording | 506ea5ef |
| delivery-055 | fixed (RED) | `scripts/test_slim_scripts.py` ServeRepoTest, IndexRepoTest (restart fetches and follows ref) | c053dfa0 |
| delivery-056 | fixed (RED) | `scripts/test_slim_scripts.py` (token not stored in the checkout) | c053dfa0 |
| delivery-057 | fixed (RED) | `scripts/test_slim_scripts.py` CreateJobHelpTest | 2f287c74 |
| delivery-058 | fixed (RED) | `scripts/test_slim_scripts.py` CreateJobSecretTest | 3de90af1 |
| delivery-059 | fixed (RED) | `telemetry-worker/test/events.test.ts` | 0ac9fe64 |
| delivery-060 | fixed (RED) | `scripts/test_ax_admin_e2e_tools.py` | 0236f010 |
| delivery-061 | fixed (RED) | `scripts/test_gauntlet_tools.py` MutantTimeoutTest | d2fc65e7 |
| delivery-062 | fixed (RED) | `scripts/test_gauntlet_tools.py` TestBaselineTest | 29ac7505 |
| delivery-063 | fixed (RED) | `scripts/test_gauntlet_tools.py` EncodingLayerTest | a35c90df |
| delivery-064 | fixed (RED) | `scripts/test_gauntlet_tools.py` NegativeControlTest | 7c2b78b5 |
| delivery-065 | fixed (RED) | `scripts/test_gauntlet_tools.py` CaretGauntletServerTest (polling stops) | 23673247 |
| delivery-066 | fixed (RED) | `scripts/test_gauntlet_tools.py` CaretGauntletServerTest (own server, busy port refused) | 23673247 |
| delivery-067 | fixed (RED) | `scripts/test_gauntlet_tools.py` GauntletMutantsTest | 3a5d4dad |
| delivery-068 | fixed (RED) | `scripts/test_gauntlet_tools.py`: no gauntlet announces a control it does not run | 68a3fb9d, c1c9812b |

Review-round fixes:

| Commit | Fix | Covering tests |
|---|---|---|
| 0920d208 | Delete stale compiled `install-command` siblings | `site/tests/component-reachability.test.ts` |
| b7a4c5ad | Check `ax_mutate` scripts against the `mutate.sh` contract | `scripts/test_restore_on_exit.py` |
| f845e264 | Run every `scripts/test_*.py` in CI | `scripts/test_workflows.py` |
| 9aa8db7d | Kill the whole test process group when a mutant times out | `scripts/test_gauntlet_tools.py` |
| 0f42bf57 | Never park the live install inside a stale `current.old` | `scripts/test_install_self_heal.py` |
| 402724eb | Serve the existing checkout when a restart cannot fetch | `scripts/test_slim_scripts.py` |
| 130e931d | Never leave an unpacked `.install-new-*` bundle behind (sh and ps1) | `scripts/test_install_self_heal.py` |

## Review rounds

| Round | Findings | Fixed |
|---|---|---|
| 1 | 1 | 1 |
| 2 | 5 | 5 |
| 3 | 2 | 2 |
| 4 | 0 | 0 |

## Commands run (final check)

All commands ran in `/Users/gary/io/ax-wt/delivery` at the head of the branch.

| Command | Result |
|---|---|
| `python3 -m unittest discover -s scripts -p 'test_<name>.py'` for each of the 34 `scripts/test_*.py` files | 34/34 files pass; 238 tests run, 237 pass, 1 subtest skipped (`rg` not installed), 0 fail |
| `cd site && npm test` | 144 tests, 144 pass, 0 fail |
| `cd site && npm run build` | pass |
| `cd telemetry-worker && npm ci && npm test` | 6 tests, 6 pass, 0 fail |
| `cd telemetry-worker && npx wrangler types && npx tsc --noEmit` with `.dev.vars` copied from `.dev.vars.example` | pass |
| `cmp` of `install.sh`, `install.ps1`, `install-slim.sh`, `install-admin.sh` against their `site/public` copies | identical |
| `sh -n install.sh`, `bash -n install-slim.sh` | pass |
| `cargo fmt --all --check` | pass |
| `cargo clippy` | not run: the branch changes no Rust source, so no crate is in scope |

## Open items

- `telemetry-worker` `npm run check` fails with `Property 'POSTHOG_KEY' does not exist on type 'Env'` when no `.dev.vars` exists, as in CI, because `wrangler types` only learns secret names from `.dev.vars`. The failing line is not changed by this branch, so the failure predates it. With `.dev.vars` copied from the example, the typecheck passes.
- `npx tsc --noEmit` in `site/` reports 29 errors, in `tests/scroll-reveal.test.ts` and in the null narrowing of `src/scripts/minimal-download.ts` (lines 153 to 169). No site script or workflow runs this check, and the reported lines are not changed by this branch, so the errors predate it.
- One subtest of `scripts/test_gauntlet_tools.py` (the `gauntlet-policy-storage.sh` negative control) skipped locally because `rg` is not installed.
- Docker is not installed here, so the Dockerfile image builds were not run; `scripts/test_docker_build.py` checks the ignore files and base image pins.
