import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { cloudDeploys } from '../src/lib/cloud-deploy.mjs';

const site = join(import.meta.dirname, '..');
const repo = join(site, '..');
const script = join(site, 'scripts', 'write-azure-template.mjs');
const read = (...p: string[]) => readFileSync(join(...p), 'utf8');
const job = JSON.parse(read(repo, 'slim', 'azure', 'azuredeploy.json'));
const server = JSON.parse(read(repo, 'slim', 'azure', 'azuredeploy-server.json'));
const docs = join(site, 'src', 'content', 'docs');
const guide = read(docs, 'guides', 'headless-linux.md');
const installation = read(docs, 'getting-started', 'installation.md');
const byId = Object.fromEntries(cloudDeploys.map((d) => [d.id, d]));

function generate(latest: string) {
	const dir = mkdtempSync(join(tmpdir(), 'ax-azure-'));
	mkdirSync(join(dir, 'public', 'releases'), { recursive: true });
	writeFileSync(join(dir, 'public', 'releases', 'latest.txt'), `${latest}\n`);
	execFileSync(process.execPath, [script], { cwd: dir, stdio: 'pipe' });
	const out = (name: string) => JSON.parse(readFileSync(join(dir, 'public', 'deploy', name), 'utf8'));
	return { job: out('azure-slim.json'), server: out('azure-slim-server.json') };
}

const withoutVersion = (t: any) => ({ ...t, parameters: { ...t.parameters, axVersion: undefined } });

test('published templates default to the newest release and are otherwise unchanged', () => {
	const out = generate('v12.3.4');
	for (const [published, source] of [[out.job, job], [out.server, server]]) {
		assert.equal(published.parameters.axVersion.defaultValue, '12.3.4');
		assert.deepEqual(withoutVersion(published), withoutVersion(source));
	}
});

test('generator rejects a malformed latest version', () => {
	assert.throws(() => generate('latest'));
});

test('job template runs the ubuntu image with ax-index-repo and keeps the repo URL secret', () => {
	assert.equal(job.parameters.repoUrl.type, 'securestring');
	const resource = job.resources.find((r: any) => r.type === 'Microsoft.App/jobs');
	assert.equal(resource.properties.configuration.triggerType, 'Manual');
	assert.deepEqual(resource.properties.template.containers[0].command, ['ax-index-repo']);
	assert.match(job.variables.image, /ax-slim:', parameters\('axVersion'\), '-ubuntu'/);
	assert.deepEqual(job.variables.baseEnv[0], { name: 'AX_REPO_URL', secretRef: 'ax-repo-url' });
	for (const cpu of job.parameters.cpu.allowedValues) assert.ok(job.variables.memoryForCpu[cpu], cpu);
});

test('server template runs the server image behind HTTPS ingress with secrets for URL and key', () => {
	assert.equal(server.parameters.repoUrl.type, 'securestring');
	assert.equal(server.parameters.apiKey.type, 'securestring');
	assert.equal(server.parameters.apiKey.minLength, 32);
	const app = server.resources.find((r: any) => r.type === 'Microsoft.App/containerApps');
	const { ingress, secrets } = app.properties.configuration;
	assert.deepEqual([ingress.external, ingress.targetPort, ingress.allowInsecure], [true, 8080, false]);
	assert.deepEqual(secrets.map((s: any) => s.name).sort(), ['ax-api-key', 'ax-repo-url']);
	const container = app.properties.template.containers[0];
	assert.equal(container.command, undefined, 'the -server image entry point is ax-serve-repo');
	assert.match(server.variables.image, /ax-slim:', parameters\('axVersion'\), '-server'/);
	assert.ok(server.variables.baseEnv.some((e: any) => e.name === 'AX_API_KEY' && e.secretRef === 'ax-api-key'));
	assert.ok(server.variables.baseEnv.some((e: any) => e.name === 'PORT' && e.value === '8080'));
	assert.match(container.env, /AX_PUBLIC_URL/);
	for (const cpu of server.parameters.cpu.allowedValues) assert.ok(server.variables.memoryForCpu[cpu], cpu);
});

test('Render blueprint and Koyeb link use the server image without a command override', () => {
	const blueprint = read(repo, 'render.yaml');
	assert.match(blueprint, /url: ghcr\.io\/garywenneker\/ax-slim:latest-server/);
	assert.match(blueprint, /key: AX_API_KEY\n\s+generateValue: true/);
	assert.match(blueprint, /key: AX_REPO_URL\n\s+sync: false/);
	assert.doesNotMatch(blueprint, /dockerCommand/);
	const koyeb = new URL(byId.koyeb.url);
	assert.equal(koyeb.searchParams.get('image'), 'ghcr.io/garywenneker/ax-slim:latest-server');
	assert.equal(koyeb.searchParams.get('command'), null);
	assert.equal(koyeb.searchParams.get('ports'), '8080;http;/');
	assert.ok((koyeb.searchParams.get('env[AX_API_KEY]') ?? '').length < 32, 'placeholder key must be rejected');
});

test('docs link every deploy button to the same URLs the homepage uses', () => {
	// The docs write the buttons as HTML links, so "&" in a URL is "&amp;" there.
	const href = (id: string) => `href="${byId[id].url.replaceAll('&', '&amp;')}"`;
	for (const id of ['azure-server', 'render', 'koyeb']) {
		assert.ok(guide.includes(href(id)), `guide: ${id}`);
		assert.ok(installation.includes(href(id)), `installation: ${id}`);
	}
	assert.ok(guide.includes(href('azure-job')), 'guide: azure-job');
	assert.match(guide, /^## Deploy to the cloud$/m);
});

test('homepage shows the server deploy buttons', () => {
	const home = read(site, 'src', 'components', 'MinimalHome.astro');
	const component = read(site, 'src', 'components', 'CloudDeploy.astro');
	assert.match(home, /<MinimalDownload \/>\s*<CloudDeploy \/>/);
	assert.match(component, /cloudDeploys\.filter\(\(d\) => d\.kind === 'MCP server'\)/);
	assert.match(component, /#deploy-to-the-cloud/);
});
