import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const site = join(import.meta.dirname, '..');
const script = join(site, 'scripts', 'write-azure-template.mjs');
const source = JSON.parse(readFileSync(join(site, '..', 'slim', 'azure', 'azuredeploy.json'), 'utf8'));
const guide = readFileSync(join(site, 'src', 'content', 'docs', 'guides', 'headless-linux.md'), 'utf8');

function generate(latest: string) {
	const dir = mkdtempSync(join(tmpdir(), 'ax-azure-'));
	mkdirSync(join(dir, 'public', 'releases'), { recursive: true });
	writeFileSync(join(dir, 'public', 'releases', 'latest.txt'), `${latest}\n`);
	execFileSync(process.execPath, [script], { cwd: dir, stdio: 'pipe' });
	return JSON.parse(readFileSync(join(dir, 'public', 'deploy', 'azure-slim.json'), 'utf8'));
}

test('published template defaults to the newest release', () => {
	const out = generate('v12.3.4');
	assert.equal(out.parameters.axVersion.defaultValue, '12.3.4');
	assert.deepEqual({ ...out, parameters: { ...out.parameters, axVersion: undefined } },
		{ ...source, parameters: { ...source.parameters, axVersion: undefined } });
});

test('generator rejects a malformed latest version', () => {
	assert.throws(() => generate('latest'));
});

test('template runs the ubuntu image with ax-index-repo and keeps the repo URL secret', () => {
	assert.equal(source.parameters.repoUrl.type, 'securestring');
	const job = source.resources.find((r: any) => r.type === 'Microsoft.App/jobs');
	assert.ok(job);
	assert.equal(job.properties.configuration.triggerType, 'Manual');
	const container = job.properties.template.containers[0];
	assert.deepEqual(container.command, ['ax-index-repo']);
	assert.match(source.variables.image, /ax-slim:', parameters\('axVersion'\), '-ubuntu'/);
	assert.deepEqual(source.variables.baseEnv[0], { name: 'AX_REPO_URL', secretRef: 'ax-repo-url' });
	for (const cpu of source.parameters.cpu.allowedValues) assert.ok(source.variables.memoryForCpu[cpu], cpu);
});

test('guide links the Deploy to Azure button to the served template', () => {
	const url = 'https://getax.wenneker.io/deploy/azure-slim.json';
	assert.ok(guide.includes(`https://portal.azure.com/#create/Microsoft.Template/uri/${encodeURIComponent(url)}`));
});
