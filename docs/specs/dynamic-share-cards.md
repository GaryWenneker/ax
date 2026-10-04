# SPEC — Dynamic share cards

Status: spec approval not obtained (autonomous run). The request is the contract: a shared docs URL must preview that page, and the preview must show the current release, produced by the site build rather than a hand-made image.

## Behaviors

1. `ogImagePath('/guides/policy-engine/')` is `/og/guides/policy-engine.png`. `ogImagePath('/')` is `/og/home.png`. A path with `..` or other unexpected characters falls back to `/og/home.png`.
2. Every docs page and the landing page set `og:image` and `twitter:image` to `https://getax.wenneker.io` plus that path. Neither `site/astro.config.mjs` nor `site/src/pages/index.astro` contains `v5.0.0`.
3. The card HTML includes the release label read from `site/public/releases/latest.txt` (today `v7.0.0`) and the page title. A title or description that contains `<` is escaped. The card does not hardcode `5.0.0`.
4. `releaseLabel` accepts only a string like `v7.0.0` and rejects anything else.
5. During `npm run build`, after `astro build`, the site screenshots each built page (docs chrome hidden, landing hero forced visible) and writes a 1200×630 PNG at the path the page’s `og:image` names. A missing screenshot fails the build.
6. The inset is the top of that page (`#hero` on the landing page, otherwise `main`), not a previously chosen Command Center shot.

## Must not change

- `latest.txt` remains the only version source. Installers and `/releases/latest.txt` stay as they are.
- No new runtime service and no hand-authored PNG per page.
- Existing page copy, aside from a short note that link previews are generated at build time.

## Setup

- Isolation: none. The change is the site the user ships; a worktree would not be the tree Netlify builds.
- New devDependency: `puppeteer` — captures each built page during the production build. `sharp` is already a dependency.
- Files: `site/src/lib/og-card.mjs`, `site/src/components/Head.astro`, `site/scripts/render-og.mjs`, `site/tests/og-card.test.ts`, edits to `site/astro.config.mjs`, `site/src/pages/index.astro`, `site/package.json`, `site/scripts/render-social.mjs`, `.github/workflows/deploy-site.yml`, and a short docs note.
- No checkpoint commits unless asked.
