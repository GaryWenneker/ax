# SPEC — ratatouille

Status: autonomous run (spec approval not obtained). The request was to make the menu.
Tier: 2.
Isolation: none. The launcher lives in this checkout.

## What it is

`./ratatouille` at the repo root opens a Ratatui menu. It does not invent a second release pipeline. Each item runs the scripts already in this repo.

## Actions

1. **Website — build and run.** In `site/`: `npm ci` when `node_modules` is missing, then `npm run build`, then `npm run preview` on 127.0.0.1:4321. The preview stays in the foreground until it stops.
2. **Website — start.** In `site/`: `npm ci` when `node_modules` is missing, then `npm run start` (`astro dev`) on 127.0.0.1:4321. It does not run the production build. The server stays in the foreground until it stops.
3. **ax — build and run locally.** macOS/Linux: `bash scripts/reinstall-cli.sh`. Windows: `powershell -File scripts/reinstall-cli.ps1`. Then `target-dev/release/ax web --port 7070`. It does not kill other ax processes.
4. **Deploy website to Netlify.** `pwsh -File scripts/deploy-netlify.ps1`. Asks for confirmation. The script still requires a clean `main` that matches origin and a green CI run.
5. **Pull main and rebuild locally.** `git pull --ff-only origin main`, then the same local reinstall as action 3. It does not start the server.
6. **Publish release and deploy the website.** Asks for confirmation, then shows the version decision below. When that decision is to keep the published version, or main is already a newer unpublished version, it runs `pwsh -File scripts/release-tag.ps1 -Wait` and then the Netlify deploy. It does not pass `-Bump` or `-Force`. When the decision is a new micro, minor, or major number, it does not tag: `release-tag.ps1` rejects a local bump, and the new number has to reach `main` through a PR first. The website version is the GitHub release tag; the deploy script writes it. This menu does not keep a second number in the site source.

## Version level

Publish reads `origin/main` after `git fetch origin main`, the published GitHub release tag, and `crates/ax-cli/Cargo.toml` on `origin/main`. It does not read the dirty working tree. Micro means the patch number.

The decision uses commits reachable from `origin/main` and not from the published tag. Merge commits are ignored. Only files that ship in the ax download count: `crates/` except `crates/ratatouille/`, the workspace `Cargo.toml` and `Cargo.lock`, `install.sh`, and `install.ps1`. These do not count: `site/`, `docs/`, `benchmarks/`, `test-smoke/`, `.github/`, and test paths (`/tests/`, `/test/`, `*_test.rs`, `*.test.ts`, `*.test.tsx`, `*.spec.ts`, `*.spec.tsx`).

| Situation | Decision |
|---|---|
| `origin/main` is the published tag commit, or no shipping file changed | Keep the published version and publish that |
| The Cargo version on `origin/main` is already greater than the published tag | Do not bump again; publish the version already on main |
| A shipping change has `BREAKING CHANGE`, a conventional breaking mark (`feat!:`, `fix(scope)!:`), a deleted `crates/ax-cli/src/commands/*.rs` other than `mod.rs`, or a subject that removes an endpoint, command, API, or flag as a whole word (`subcommand` counts; `capital` does not) | Major: `8.0.0` → `9.0.0` |
| Otherwise a shipping change is `feat` or `feat(scope)`, or it adds a command file under `crates/ax-cli/src/commands/` | Minor: `8.0.0` → `8.1.0` |
| Otherwise a shipping file changed | Micro: `8.0.0` → `8.0.1` |

The highest level in the range wins. A rename of a command file onto another command file is not a deletion. A site-only `feat` does not bump the ax version.

## Must not

- Do not tag, push, or deploy just by opening the menu.
- Do not kill the running ax MCP process as part of a local rebuild.
- Do not treat every bump as a patch, and do not tag the published version when the changes call for a new number.
