import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { readSourceVersion } from '../src/lib/source-version.mjs';

test('docs use the source version when downloadable binaries lag behind main', () => {
	const root = mkdtempSync(path.join(tmpdir(), 'ax-docs-version-'));
	try {
		const site = path.join(root, 'site');
		mkdirSync(path.join(root, 'crates', 'ax-cli'), { recursive: true });
		mkdirSync(path.join(site, 'public', 'releases'), { recursive: true });
		writeFileSync(path.join(root, 'crates', 'ax-cli', 'Cargo.toml'), '[package]\nname = "ax-cli"\nversion = "8.0.0"\n\n[dependencies]\nother = { version = "9.0.0" }\n');
		const pointer = path.join(site, 'public', 'releases', 'latest.txt');
		writeFileSync(pointer, 'v7.1.0\n');
		assert.equal(readSourceVersion(site), 'v8.0.0');
		assert.equal(readFileSync(pointer, 'utf8'), 'v7.1.0\n');
		writeFileSync(path.join(root, 'crates', 'ax-cli', 'Cargo.toml'), '[package]\nname = "ax-cli"\n\n[dependencies]\nversion = "9.0.0"\n');
		assert.throws(() => readSourceVersion(site), /Missing or invalid CLI package version/);
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
});
