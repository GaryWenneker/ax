// Publish the "Deploy to Azure" templates for the headless (slim) build.
// Copies each ../slim/azure template to public/deploy/ with the axVersion default set to the
// newest release from public/releases/latest.txt, so the buttons on Guides → Headless Linux
// build always offer the current image.
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

export const templates = {
	'azure-slim.json': 'azuredeploy.json',
	'azure-slim-server.json': 'azuredeploy-server.json',
};

const sourceDir = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'slim', 'azure');
const latestFile = join(process.cwd(), 'public', 'releases', 'latest.txt');
const targetDir = join(process.cwd(), 'public', 'deploy');

const version = readFileSync(latestFile, 'utf8').trim().replace(/^v/, '');
if (!/^\d+\.\d+\.\d+$/.test(version)) {
	console.error(`${latestFile}: expected vX.Y.Z, got "${version}"`);
	process.exit(1);
}
mkdirSync(targetDir, { recursive: true });
for (const [published, source] of Object.entries(templates)) {
	const path = join(sourceDir, source);
	const template = JSON.parse(readFileSync(path, 'utf8'));
	if (!template.parameters?.axVersion) {
		console.error(`${path} has no axVersion parameter`);
		process.exit(1);
	}
	template.parameters.axVersion.defaultValue = version;
	writeFileSync(join(targetDir, published), `${JSON.stringify(template, null, 2)}\n`);
	console.log(`deploy/${published}: ax-slim ${version}`);
}
