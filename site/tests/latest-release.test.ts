import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { handler } from '../netlify/functions/latest-release.ts';

async function inDir(files: Record<string, string>, accept: string) {
	const dir = mkdtempSync(join(tmpdir(), 'latest-release-'));
	for (const [rel, body] of Object.entries(files)) {
		mkdirSync(join(dir, rel, '..'), { recursive: true });
		writeFileSync(join(dir, rel), body);
	}
	const before = process.cwd();
	process.chdir(dir);
	try {
		return await (handler as any)({ headers: { accept } });
	} finally {
		process.chdir(before);
	}
}

test('reads the version from the included_files path under site/', async () => {
	const res = await inDir({ 'site/public/releases/latest.txt': 'v6.0.1\n' }, '*/*');
	assert.equal(res.statusCode, 200);
	assert.equal(res.body, 'v6.0.1\n');
});

test('reads the version from public/ when run from the site folder', async () => {
	const res = await inDir({ 'public/releases/latest.txt': 'v6.0.1\n' }, 'text/html');
	assert.equal(res.statusCode, 200);
	assert.match(res.body, /<title>ax latest release — v6\.0\.1<\/title>/);
});

test('answers 503 instead of a made-up version when no file is found', async () => {
	const res = await inDir({}, '*/*');
	assert.equal(res.statusCode, 503);
	assert.equal(res.body, 'latest release unknown\n');
	assert.equal(res.headers['Cache-Control'], 'no-store');
});
