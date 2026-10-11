import { readFileSync } from 'node:fs';
import path from 'node:path';
import { readSourceVersion } from './source-version.mjs';

/** Current source version; builds from main show main's CLI version. */
export const AX_VERSION = readSourceVersion(process.cwd());

/** Latest downloadable release; never advertise unpublished binary assets. */
export const AX_RELEASE_VERSION = readFileSync(
	path.join(process.cwd(), 'public', 'releases', 'latest.txt'),
	'utf8',
).trim();
