# SPEC: PHP runtime uses the latest stable version

Tier 2. The PHP runtime is the exception to the seven-day release-age gate.

Spec approval: not obtained (autonomous run). The request is an operational order during the 21 October 2026 critical PHP security release (https://www.php.net/, scheduled 14:00 UTC). Cargo, npm, and GitHub Actions keep the 168-hour gate.

Isolation: the current working tree. A worktree would omit the uncommitted lifecycle docs this change has to stay consistent with.

## Behaviors

1. `latest_stable_php` reads the php.net releases document and returns the highest `x.y.z` that is not museum and not tagged alpha, beta, or RC. Older majors stay in the document and must not win. A document with no such version raises `PolicyError`.
2. `evaluate("php", ...)` does not call the seven-day publication check. The latest stable version passes even when it was published today.
3. Any other PHP runtime version fails, including a patch older than seven days and an older minor. The failure names the required latest version and states that the 7-day minimum does not apply.
4. A pin of the latest stable minor (`8.5` while latest is `8.5.11`) passes, because setup-php and image tags of that form install that minor's current patch. An exact older patch (`8.5.10`) fails. A Docker suffix (`8.5.11-cli-alpine`) is the same version as `8.5.11`.
5. Workflow `php-version: ${{ steps.php_runtime.outputs.version }}` is the resolved-latest pin and is not a stale pin. Concrete `php-version` values and `FROM php:` tags are checked.
6. CLI: `--php-runtime VERSION --php-latest VERSION` exits 1 when VERSION is not current and 0 when it is. `--php-scan DIR` checks workflow files and Dockerfiles under that directory. `--print-latest-php` prints the version from php.net.
7. The PHP security-audit scripts that run Composer or PHP fail when the running `PHP_VERSION`, a workflow pin, or a Docker image tag is not that latest stable version. A `composer.lock` with no PHP binary fails closed.

## Must not change

- npm and Cargo additions younger than seven days still fail (`test_seven_day_boundary_and_immature_release`).
- Unverifiable non-registry packages still fail.
- No new dependency.
- The floating Docker tag `latest` is still not accepted as a PHP pin.

## Setup

- No new packages.
- Tests: `python3 -m unittest discover -s scripts -p 'test_dependency_age.py'`
- Files: `scripts/check_dependency_age.py`, `scripts/test_dependency_age.py`, `docs/DEPENDENCY_LIFECYCLE.md`, `site/src/content/docs/guides/dependency-lifecycle.md`, `README.md`, `.agents/skills/security-audit/SKILL.md`, `.agents/skills/security-audit/evals/evals.json`, `.agents/skills/security-audit/scripts/php-runtime-guard.sh`, `.agents/skills/security-audit/scripts/security-audit.sh`, `.agents/skills/security-audit/scripts/scanners/php.sh`, `.agents/skills/security-audit/references/ci-security-pipeline.md`, `.agents/skills/security-audit/references/supply-chain-security.md`
