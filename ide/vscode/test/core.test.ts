import { test } from 'node:test';
import assert from 'node:assert/strict';
import { PanelHost, commandCenterUrl, webviewHtml } from '../src/core.ts';

test('webview html has csp frame-src for port only', () => {
  const html = webviewHtml(7071, 'n0nce');
  assert.equal(commandCenterUrl(7071), 'http://127.0.0.1:7071/?embed=1');
  assert.match(html, /<iframe src="http:\/\/127\.0\.0\.1:7071\/\?embed=1"/);
  const csp = /http-equiv="Content-Security-Policy" content="([^"]+)"/.exec(html)?.[1] ?? '';
  assert.match(csp, /default-src 'none'/);
  assert.match(csp, /frame-src http:\/\/127\.0\.0\.1:7071;/);
  assert.match(csp, /style-src 'nonce-n0nce'/);
  assert.doesNotMatch(csp, /\*|https?:\/\/(?!127\.0\.0\.1:7071)/);
});

test('webview html rejects a port that is not a number in range', () => {
  assert.throws(() => webviewHtml(0, 'n'), /port/);
  assert.throws(() => webviewHtml(70700, 'n'), /port/);
  assert.throws(() => webviewHtml(7070.5, 'n'), /port/);
});

test('panel is reused on second open', () => {
  let created = 0;
  let revealed = 0;
  let dispose: () => void = () => {};
  const host = new PanelHost(() => {
    created++;
    return {
      reveal: () => revealed++,
      onDispose: (fn) => (dispose = fn),
    };
  });
  host.open();
  host.open();
  assert.equal(created, 1);
  assert.equal(revealed, 1);
  dispose();
  host.open();
  assert.equal(created, 2);
});
