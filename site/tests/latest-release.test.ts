import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHandler } from '../netlify/functions/latest-release.ts';

const script = join(import.meta.dirname, '..', 'scripts', 'write-latest-version.mjs');

async function call(version: string, accept: string) {
	return (await (createHandler(version) as any)({ headers: { accept } })) as {
		statusCode: number;
		headers: Record<string, string>;
		body: string;
	};
}

test('installers get the version as plain text', async () => {
	const res = await call('v6.0.1', '*/*');
	assert.equal(res.statusCode, 200);
	assert.equal(res.body, 'v6.0.1\n');
});

test('browsers get an HTML page with the version', async () => {
	const res = await call('v6.0.1', 'text/html');
	assert.equal(res.statusCode, 200);
	assert.match(res.body, /<title>ax latest release — v6\.0\.1<\/title>/);
});

test('answers 503 instead of a made-up version when the version is empty', async () => {
	const res = await call('', '*/*');
	assert.equal(res.statusCode, 503);
	assert.equal(res.body, 'latest release unknown\n');
	assert.equal(res.headers['Cache-Control'], 'no-store');
});

function siteDir(latest?: string) {
	const dir = mkdtempSync(join(tmpdir(), 'latest-version-'));
	mkdirSync(join(dir, 'netlify'));
	if (latest !== undefined) {
		mkdirSync(join(dir, 'public', 'releases'), { recursive: true });
		writeFileSync(join(dir, 'public', 'releases', 'latest.txt'), latest);
	}
	return dir;
}

test('the build step writes the version from latest.txt', () => {
	const dir = siteDir('v6.0.1\n');
	execFileSync('node', [script], { cwd: dir });
	const json = JSON.parse(readFileSync(join(dir, 'netlify', 'latest-version.json'), 'utf8'));
	assert.deepEqual(json, { version: 'v6.0.1' });
});

test('the build step fails when latest.txt is empty or missing', () => {
	for (const dir of [siteDir('  \n'), siteDir()]) {
		assert.throws(() => execFileSync('node', [script], { cwd: dir, stdio: 'pipe' }));
	}
});
