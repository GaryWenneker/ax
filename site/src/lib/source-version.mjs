import { readFileSync } from 'node:fs';
import path from 'node:path';

/** Version of the CLI source used to build these docs, independent of downloads. */
export function readSourceVersion(siteDir) {
	const manifest = path.resolve(siteDir, '..', 'crates', 'ax-cli', 'Cargo.toml');
	const cargo = readFileSync(manifest, 'utf8');
	const packageSection = cargo.match(/^\[package\]\s*\n([\s\S]*?)(?=^\[|$(?![\s\S]))/m)?.[1];
	const version = packageSection?.match(/^version\s*=\s*"(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)"\s*$/m)?.[1];
	if (!version) throw new Error(`Missing or invalid CLI package version in ${manifest}`);
	return `v${version}`;
}
