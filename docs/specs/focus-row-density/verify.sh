#!/bin/sh
set -eu
cd "$(dirname "$0")/../../.."
python docs/specs/focus-row-density/mutations.py
npm test --prefix crates/ax-web/web-ui
node --experimental-strip-types --test crates/ax-web/web-ui/src/lib/navigation-retirement.test.ts crates/ax-web/web-ui/src/lib/nav.test.ts crates/ax-web/web-ui/src/lib/themes.test.ts
npm test --prefix site
npm run build --prefix crates/ax-web/web-ui
node docs/specs/focus-row-density/ui-check.mjs
git diff --check -- . ':!crates/ax-web/web-ui/dist'
git diff --cached --check -- . ':!crates/ax-web/web-ui/dist'
