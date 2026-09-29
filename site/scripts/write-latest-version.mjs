// Bake public/releases/latest.txt into the latest-release function bundle.
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const source = join(process.cwd(), 'public', 'releases', 'latest.txt');
const version = readFileSync(source, 'utf8').trim();
if (!version) {
	console.error(`${source} is empty`);
	process.exit(1);
}
writeFileSync(join(process.cwd(), 'netlify', 'latest-version.json'), `${JSON.stringify({ version })}\n`);
console.log(`latest-version.json: ${version}`);
