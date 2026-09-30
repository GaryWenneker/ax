import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { pageLockScript } from '../src/lib/page-lock.ts';

test('page text cannot be selected and the browser menu is blocked', () => {
	const script = pageLockScript();
	assert.match(script, /contextmenu/);
	assert.match(script, /preventDefault/);
	assert.match(script, /selectstart/);
	assert.doesNotMatch(script, /stopPropagation/);
	assert.match(script, /input, textarea, select/);

	const css = readFileSync(new URL('../src/styles/theme.css', import.meta.url), 'utf8');
	assert.match(css, /user-select:\s*none/);
	assert.match(css, /-webkit-touch-callout:\s*none/);
});
