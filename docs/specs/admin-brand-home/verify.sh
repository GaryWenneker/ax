#!/bin/sh
set -eu
cd "$(dirname "$0")/../../.."
node --experimental-strip-types --test crates/ax-web/web-ui/src/lib/navigation-retirement.test.ts crates/ax-web/web-ui/src/lib/nav.test.ts
npm test --prefix site
npm run build --prefix crates/ax-web/web-ui
node docs/specs/admin-brand-home/ui-check.mjs
git diff --check
