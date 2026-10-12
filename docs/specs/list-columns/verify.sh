#!/bin/sh
set -eu
cd "$(dirname "$0")/../../.."
node --check docs/specs/list-columns/ui-check.mjs
python - <<'PYTHON'
from pathlib import Path
p=Path('docs/specs/list-columns/mutations.py')
compile(p.read_text(),str(p),'exec')
PYTHON
python docs/specs/list-columns/mutations.py
npm test --prefix crates/ax-web/web-ui
node --experimental-strip-types --test crates/ax-web/web-ui/src/lib/navigation-retirement.test.ts crates/ax-web/web-ui/src/lib/nav.test.ts crates/ax-web/web-ui/src/lib/themes.test.ts crates/ax-web/web-ui/src/lib/columnModel.test.ts
npm test --prefix site
npm run build --prefix crates/ax-web/web-ui
node docs/specs/list-columns/ui-check.mjs
git diff --check -- . ':!crates/ax-web/web-ui/dist'
git diff --cached --check -- . ':!crates/ax-web/web-ui/dist'
