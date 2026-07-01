# npm publish (`@garywenneker/ax`)

The npm package is a **launcher only** — it downloads the native `ax` binary from GitHub Releases. There is no JavaScript API.

## Prepare

1. Edit `docs/npm/README.md` and `site/src/content/docs/getting-started/installation.md` if install/docs copy changes.
2. Tag a GitHub Release with **all six** assets (same names as `install.sh` / `install.ps1`):
   - `ax-win32-x64.zip`, `ax-win32-arm64.zip`
   - `ax-linux-x64.tar.gz`, `ax-linux-arm64.tar.gz` (WSL2)
   - `ax-darwin-x64.tar.gz`, `ax-darwin-arm64.tar.gz`
3. Run `bash scripts/verify-release-assets.sh dist/` before upload or getax deploy.
4. Bump `crates/ax-cli/Cargo.toml` version (npm version follows this — **2.0.0** as of the policy-engine release).

## Pack

```bash
# macOS / Linux / Git Bash
bash scripts/pack-npm.sh

# Windows
powershell -File scripts/pack-npm.ps1
```

Output: `release/npm/main/` with `package.json`, `npm-shim.js`, `README.md`.

`scripts/sync-npm-docs.js` runs automatically and rejects `@colbymchenry` / `codegraph` strings in the npm readme.

## Publish

```bash
cd release/npm/main
npm publish --access public
```

Dry-run: `npm pack` in that directory.

## Scope

- Package: `@garywenneker/ax`
- Repo: `GaryWenneker/ax`
- Docs site: https://getax.wenneker.io

Do **not** reuse CodeGraph npm package names or copy CodeGraph docs verbatim into this readme.
