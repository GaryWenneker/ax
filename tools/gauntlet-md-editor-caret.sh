#!/usr/bin/env bash
# Markdown editor caret/overlay metric gauntlet (Command Center).
#
# Tests this checkout's build: it starts AX_BIN (default target-dev/debug/ax) as
# `ax web` on a free port, or on AX_WEB_PORT when set, and stops it on exit. A
# port that already answers is an error, so an installed ax web is never tested
# by mistake (and `ax web` would stop whatever listens there).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

WEB_UI="$ROOT/crates/ax-web/web-ui"
AX_BIN="${AX_BIN:-$ROOT/target-dev/debug/ax}"
[[ -x "$AX_BIN" ]] || { echo "FAIL: $AX_BIN is missing; build it with cargo build -p ax-cli" >&2; exit 1; }
PORT="${AX_WEB_PORT:-$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')}"
URL="http://127.0.0.1:${PORT}"
export AX_WEB_URL="$URL"

echo "== tsc =="
(cd "$WEB_UI" && npx tsc --noEmit)

echo "== start this checkout's ax web =="
if curl -fsS -m 2 "$URL/" >/dev/null 2>&1; then
  echo "FAIL: something already answers on $URL; free the port or set AX_WEB_PORT" >&2
  exit 1
fi
WEB_LOG="$(mktemp "${TMPDIR:-/tmp}/ax-web-caret-gauntlet.XXXXXX")"
"$AX_BIN" web "$ROOT" --port "$PORT" >"$WEB_LOG" 2>&1 &
WEB_PID=$!
stop_web() {
  kill "$WEB_PID" 2>/dev/null || true
  wait "$WEB_PID" 2>/dev/null || true
  rm -f "$WEB_LOG"
}
trap stop_web EXIT
echo "started $AX_BIN web on ${PORT} (pid $WEB_PID)"
for _ in $(seq 1 40); do
  kill -0 "$WEB_PID" 2>/dev/null || { cat "$WEB_LOG" >&2; echo "FAIL: ax web exited" >&2; exit 1; }
  if curl -fsS -m 1 "$URL/" >/dev/null 2>&1; then break; fi
  sleep 0.5
done
curl -fsS -m 3 "$URL/" >/dev/null || { cat "$WEB_LOG" >&2; echo "FAIL: ax web did not answer on $URL" >&2; exit 1; }

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

echo "gauntlet-md-editor-caret: ok"
