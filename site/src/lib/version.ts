import { readFileSync } from 'node:fs';
import path from 'node:path';

/** Current ax release tag from public/releases/latest.txt (e.g. v2.0.0). */
export const AX_VERSION = readFileSync(
	path.join(process.cwd(), 'public', 'releases', 'latest.txt'),
	'utf8',
).trim();
