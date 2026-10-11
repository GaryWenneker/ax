# EVIDENCE — Dynamic share cards

Spec: `/Users/gary/io/ax/docs/specs/dynamic-share-cards.md`
Spec approval: not obtained (autonomous run).
Tier: 2.
Source: working tree, not a commit.

## Behaviors

| Behavior | Check |
| --- | --- |
| One image path per page, home is `/og/home.png` | `ogImagePath` tests |
| Pages no longer point at the v5 card | source test on `astro.config.mjs` and `index.astro` |
| Card paints the release from `latest.txt` and escapes titles | `cardHtml` / `releaseLabel` tests; render log `share cards: v7.0.0, 38 pages` |
| Bad release labels throw | `releaseLabel` test |
| Build writes a 1200×630 PNG for each published `og:image` | `node scripts/render-og.mjs` after `astro build`, 38 files, sharp rejects the wrong size |
| Inset is that page, docs chrome hidden | `prepareShareShot` plus visual check of home, Policy Engine, and Command Center cards |

## Gauntlet

Final run after the last edit to the renderer (path join) and card layout:

- `cd site && node --test tests/` — 26 pass, 0 fail
- `cd site && node scripts/render-og.mjs` — `share cards: v7.0.0, 38 pages`, exit 0
- `astro build` in the same session, before that render — 40 pages, exit 0
- Manual mutants, restored after each: home path killed, release regex killed, unescaped `<` killed (3/3)

## Review

| Round | Findings | Result |
| --- | --- | --- |
| 1 | Docs screenshots cropped the left of the title (`object-fit: cover` centered). Static file server joined an absolute URL path. | Fixed. Cards re-rendered. Policy Engine and Command Center titles are intact. |
| 2 | No further finding. | Stop. |

## Limits

- A shared URL with a hash still previews the page, not the scrolled section. Crawlers do not send the hash.
- Cards are produced by `npm run build`, from `public/releases/latest.txt`. They update on the next site deploy, not on an already-open share cache.
- Admin pages do not publish an `og:image`.
- On this Mac, Puppeteer's Chrome for Testing cache was missing a framework library. The build uses the installed Google Chrome. CI installs Chrome for Testing with its system libraries.
