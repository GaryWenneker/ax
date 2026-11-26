// Publish the "Deploy to Azure" template for the headless (slim) build.
// Copies ../slim/azure/azuredeploy.json to public/deploy/azure-slim.json with the axVersion
// default set to the newest release from public/releases/latest.txt, so the button on
// Guides → Headless Linux build always offers the current image.
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const source = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'slim', 'azure', 'azuredeploy.json');
const latestFile = join(process.cwd(), 'public', 'releases', 'latest.txt');
const target = join(process.cwd(), 'public', 'deploy', 'azure-slim.json');

const version = readFileSync(latestFile, 'utf8').trim().replace(/^v/, '');
if (!/^\d+\.\d+\.\d+$/.test(version)) {
	console.error(`${latestFile}: expected vX.Y.Z, got "${version}"`);
	process.exit(1);
}
const template = JSON.parse(readFileSync(source, 'utf8'));
if (!template.parameters?.axVersion) {
	console.error(`${source} has no axVersion parameter`);
	process.exit(1);
}
template.parameters.axVersion.defaultValue = version;
mkdirSync(dirname(target), { recursive: true });
writeFileSync(target, `${JSON.stringify(template, null, 2)}\n`);
console.log(`deploy/azure-slim.json: ax-slim ${version}`);
