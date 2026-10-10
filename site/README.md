# ax docs site (getax.wenneker.io)

Starlight + Astro site for [ax](https://github.com/GaryWenneker/ax) documentation. The header, landing page, and generated share cards read the **source version** from `../crates/ax-cli/Cargo.toml`. Production documentation is built from `main`. Download links and the latest-release function use `public/releases/latest.txt`, which changes only when binary assets are published. The landing page explains any difference between the two versions.

## Commands

```bash
cd site
npm ci
npm run dev      # http://localhost:4321
npm run build    # output in site/dist
```

## Deploy

Production: https://getax.wenneker.io (Netlify).

```bash
netlify deploy --prod --dir=dist
```

Or run from repo root after a release:

```powershell
.\scripts\publish-getax-releases.ps1 -Tag v2.0.0
```

Requires Netlify CLI linked to the getax site (`netlify link` in `site/`).

## Content

Markdown lives in `src/content/docs/`. Sidebar order is configured in `astro.config.mjs`.

When cutting a new ax release, update `public/releases/latest.txt` only after all six platform binaries are on the CDN — see [docs/PRODUCTION.md](../docs/PRODUCTION.md).
