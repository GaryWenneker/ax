import type { Handler } from '@netlify/functions';
import latest from '../latest-version.json' with { type: 'json' };

/** True only when the client explicitly prefers HTML (browsers / axe), not wildcard Accept. */
function wantsHtml(acceptHeader: string): boolean {
	const accept = acceptHeader.toLowerCase();
	if (!accept || accept === '*/*') return false;
	const html = accept.match(/text\/html\s*(?:;\s*q=([0-9.]+))?/);
	if (!html) return false;
	const htmlQ = html[1] !== undefined ? Number(html[1]) : 1;
	if (Number.isNaN(htmlQ) || htmlQ <= 0) return false;
	const plain = accept.match(/text\/plain\s*(?:;\s*q=([0-9.]+))?/);
	if (plain) {
		const plainQ = plain[1] !== undefined ? Number(plain[1]) : 1;
		if (!Number.isNaN(plainQ) && plainQ > htmlQ) return false;
	}
	return true;
}

/** Prefer Accept: text/html (browsers / axe) → HTML with lang + title; else plain text for installers. */
export function createHandler(version: string): Handler {
	return async (event) => {
		if (!version) {
			return {
				statusCode: 503,
				headers: {
					'Content-Type': 'text/plain; charset=utf-8',
					'Cache-Control': 'no-store',
				},
				body: 'latest release unknown\n',
			};
		}
		const accept = event.headers.accept ?? event.headers.Accept ?? '';
		const asHtml = wantsHtml(accept);

		const commonHeaders = {
			// Critical: without Vary, the edge caches HTML and serves it to curl/installers.
			Vary: 'Accept',
			'Cache-Control': 'public, max-age=60, must-revalidate',
		};

		if (asHtml) {
			const safe = version.replace(/[<>&"]/g, '');
			const body = `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>ax latest release — ${safe}</title>
</head>
<body>
<pre>${safe}
</pre>
</body>
</html>
`;
			return {
				statusCode: 200,
				headers: {
					...commonHeaders,
					'Content-Type': 'text/html; charset=utf-8',
				},
				body,
			};
		}

		return {
			statusCode: 200,
			headers: {
				...commonHeaders,
				'Content-Type': 'text/plain; charset=utf-8',
			},
			body: `${version}\n`,
		};
	};
}

export const handler = createHandler(latest.version);
