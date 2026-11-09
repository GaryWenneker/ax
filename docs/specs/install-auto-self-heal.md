# SPEC — Installer automatic self-heal

- Tier: 2
- Spec approval: obtained (user: "implementeer, update website… bump… werkende versie online")
- Setup plan:
  - Tools to install: none
  - Git: work on a branch; commit when requested / at ship
  - Files the gauntlet will add, by path:
    - `docs/specs/install-auto-self-heal.md` (this SPEC)
    - `docs/evidence/install-auto-self-heal.md` (EVIDENCE)
    - `scripts/test_install_self_heal.py` (unit tests for recoverable helpers)
    - edits: `install.ps1`, `site/public/install.ps1`, `install.sh`, `site/public/install.sh` (if present)
  - New dependencies: none

## Goal

Running `irm https://getax.wenneker.io/install.ps1 | iex` (or `install.sh`) must recover a broken Windows/macOS install without the user manually clearing environment variables, stopping processes, or deleting upgrade leftovers.

## Scenarios

Feature: Installer self-heal before download

  Scenario: Clear stuck Windows upgrade status so `ax version` stops nagging
    Given `%LOCALAPPDATA%\ax\upgrade-status.txt` exists with text containing `park current install`
    And `%LOCALAPPDATA%\ax\upgrade-staging-*` directories may exist
    When the user runs `install.ps1`
    Then before resolving a release version the installer removes `upgrade-status.txt`
    And removes every `upgrade-staging-*` directory under the install root
    And stops running `ax` processes (best effort)
    And after a successful install, `ax version` does not print `Windows install did not finish`

  Scenario: Ignore a bad GitHub token without manual unset
    Given `GITHUB_TOKEN` or `GH_TOKEN` is set in the process environment to a value that yields HTTP 401/403 from `api.github.com`
    When the installer queries GitHub Releases for assets
    Then it retries the same request without Authorization
    And for the remainder of this install process it unsets process-scoped `GITHUB_TOKEN` and `GH_TOKEN`
    And it does not require the user to edit User/Machine environment variables first
    And if a public release with the platform asset exists, resolution succeeds

  Scenario: Clear an invalid AX_VERSION pin automatically
    Given `AX_VERSION` is set (Process and/or User) to a tag that has no platform asset
    When the installer resolves the version
    Then it clears that pin from Process and User scopes
    And installs the latest installable release instead
    And does not throw the “unset pinning” fatal path when a newer installable tag exists

  Scenario: Unix installer clears upgrade leftovers too
    Given `~/.ax/upgrade-staging-*` directories exist
    And (if present) an upgrade status/marker file used by ax upgrade recovery
    When the user runs `install.sh`
    Then those staging dirs (and status marker if defined for Unix) are removed before download
    And running `ax` processes are stopped (best effort)

  Scenario: Successful reinstall still verifies the binary
    Given a prior install exists and self-heal ran
    When install finishes
    Then all synced `ax` / `ax.exe` targets report the installed tag
    And project init behavior is unchanged (`AX_SKIP_PROJECT_INIT=1` still skips)

## Must NOT

- Delete the user’s User-scoped `GITHUB_TOKEN` / `GH_TOKEN` permanently unless the API returned 401/403 for that token during this run (process unset always; User/Machine clear only after confirmed auth failure, with a one-line warning).
- Require a second manual script or documented “geheim” env cleanup for the failure modes above.
- Change release asset naming, platform matrix, or the GitHub-API-first resolution order from `install-version-resolution`.
- Use HTTP HEAD against GitHub release CDN URLs.
- Move, rename, or delete `.ax` project databases (install root under `%LOCALAPPDATA%\ax` / `~/.ax` product install tree only).

## Revisions

- 2026-11-09: Initial SPEC from Windows Access-denied / “no downloadable assets” incident; user asked for automatic install-time checks.
