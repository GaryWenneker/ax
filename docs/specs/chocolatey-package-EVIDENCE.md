# EVIDENCE: Chocolatey package for AX

**SPEC:** `/Users/gary/io/ax/docs/specs/chocolatey-package.md`  
**Spec approval:** approved 2026-11-09 (`approved` → recommended options)  
**Source state:** local uncommitted packaging + workflow  
**Tier:** 2

## Decisions locked

| Item | Choice |
|---|---|
| Package id | `ax-agent` |
| Push | manual `push=true` only |
| Install root | `%LOCALAPPDATA%\ax\current` (match `install.ps1`) |

## Mapping

| Scenario | Check | Result |
|---|---|---|
| Placeholders stamp from SHA256SUMS | `python3 scripts/chocolatey_prepare.py --tag v8.4.3 --dry-run` | PASS — x64/arm64 hashes from published release |
| Parse / stamp unit tests | `python3 -m unittest test_chocolatey_prepare.py` | PASS — 4/4 |
| Package files present | `packaging/chocolatey/ax-agent/**` | PASS |
| Workflow manual-only | `.github/workflows/chocolatey.yml` `on: workflow_dispatch` | PASS |
| Docs | `site/.../installation.md` Chocolatey section | PASS |
| Live `choco pack` / install-test / push | GitHub Actions on Windows | NOT RUN — needs commit + `gh workflow run` + optional `CHOCOLATEY_API_KEY` |

## Gauntlet

| Layer | Command | Result |
|---|---|---|
| Unit | `cd scripts && python3 -m unittest test_chocolatey_prepare.py -v` | 4 passed |
| Dry-run against v8.4.3 | `python3 scripts/chocolatey_prepare.py --tag v8.4.3 --dry-run` | ok |
| Real choco pack/install | skipped locally (macOS; no choco) | deferred to CI |
| Mutation | skipped | packaging stamp logic covered by unit tests |

## Review rounds

| Round | Skills (status) | Findings | Fixed |
|---|---|---|---|
| 1 | `old-coder` usable; no PowerShell/YAML stack skill | 1 minor — nuspec `licenseUrl` pointed at README | R1-1 → LICENSE.txt URL |
| 2 | same | 0 | — |

## Owner follow-up

1. Commit/push these files.
2. Create a Chocolatey.org account; add repo secret `CHOCOLATEY_API_KEY`.
3. `gh workflow run chocolatey.yml -f tag=v8.4.3 -f push=false` (pack + install-test).
4. After green: `gh workflow run chocolatey.yml -f tag=v8.4.3 -f push=true`.
5. Wait for Community moderation; then users: `choco install ax-agent -y`.
