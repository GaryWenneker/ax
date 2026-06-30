# npm publish (`@garywenneker/ax`)

The npm package is a **launcher only** — it downloads the native `ax` binary from GitHub Releases. There is no JavaScript API.

## Prepare

1. Edit `docs/npm/README.md` if install/docs copy changes.
2. Tag a GitHub Release with assets `ax-<platform>-<arch>.tar.gz` / `.zip` (same names as `install.sh` / `install.ps1`).
3. Bump `crates/ax-cli/Cargo.toml` version (npm version follows this).

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
