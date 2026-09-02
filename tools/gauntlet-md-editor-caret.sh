#!/usr/bin/env bash
# Markdown editor caret/overlay metric gauntlet (Command Center).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

WEB_UI="$ROOT/crates/ax-web/web-ui"
PORT="${AX_WEB_PORT:-7070}"
URL="http://127.0.0.1:${PORT}"
export AX_WEB_URL="$URL"

echo "== tsc =="
(cd "$WEB_UI" && npx tsc --noEmit)

echo "== ensure ax web =="
STARTED_WEB=0
if ! curl -fsS -m 2 "$URL/" >/dev/null; then
  echo "starting ax web on ${PORT}"
  (cd "$ROOT" && ax web --port "$PORT" >/tmp/ax-web-caret-gauntlet.log 2>&1) &
  STARTED_WEB=1
  for _ in $(seq 1 40); do
    if curl -fsS -m 1 "$URL/" >/dev/null 2>&1; then break; fi
    sleep 0.5
  done
fi
curl -fsS -m 3 "$URL/" >/dev/null

echo "== playwright md-editor-caret (desktop-chrome) =="
(cd "$WEB_UI" && npx playwright test e2e/md-editor-caret.spec.ts --project=desktop-chrome)

echo "== manual mutant: overlay code font-size 14px must desync metrics =="
(cd "$WEB_UI" && node --input-type=module <<'EOF'
import { chromium } from 'playwright';
const url = process.env.AX_WEB_URL ?? 'http://127.0.0.1:7070';
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
const list = await page.request.get(url + '/api/policy/rules');
if (!list.ok()) throw new Error('GET /api/policy/rules failed');
const payload = await list.json();
const ids = (payload.rules ?? []).map((r) => r.id);
const id = ids.includes('agent-workflow') ? 'agent-workflow' : ids[0];
await page.goto(`${url}/policy/rules/edit?id=${encodeURIComponent(id)}&mode=edit`);
await page.locator('textarea.w-md-editor-text-input').waitFor({ timeout: 20000 });
await page.addStyleTag({
  content:
    '.md-editor-wrap .w-md-editor-text-pre > code { font-size: 14px !important; line-height: 18px !important; }',
});
const metrics = await page.evaluate(() => {
  const code = document.querySelector('.w-md-editor-text-pre > code');
  const ta = document.querySelector('textarea.w-md-editor-text-input');
  const cs = getComputedStyle(code);
  const ts = getComputedStyle(ta);
  return { code: cs.fontSize, ta: ts.fontSize };
});
await browser.close();
if (metrics.code === metrics.ta) {
  console.error('mutant survived: overlay and textarea font-size still match', metrics);
  process.exit(1);
}
console.log('mutant killed: overlay', metrics.code, 'textarea', metrics.ta);
EOF
)

if [ "$STARTED_WEB" = "1" ]; then
  echo "(left ax web running on ${PORT})"
fi

echo "gauntlet-md-editor-caret: ok"
