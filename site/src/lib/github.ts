/**
 * Build-time GitHub star count. Fetched once when the site is built; memoized
 * for the build process so every page share a single API call.
 *
 * Always renders the live `stargazers_count` (never a marketing placeholder).
 * If the API is unreachable, falls back to an empty label so the UI can omit
 * a fake number rather than inventing one.
 */
function format(n: number): string {
	// Exact count — no rounded "22k"-style marketing labels.
	return new Intl.NumberFormat('en-US').format(n);
}

async function fetchStars(fallback: string): Promise<string> {
	try {
		const controller = new AbortController();
		const timeout = setTimeout(() => controller.abort(), 5000);
		const headers: Record<string, string> = {
			Accept: 'application/vnd.github+json',
			'User-Agent': 'ax-site',
			'X-GitHub-Api-Version': '2022-11-28',
		};
		const token =
			process.env.GITHUB_TOKEN?.trim() ||
			process.env.GH_TOKEN?.trim() ||
			process.env.NETLIFY_GITHUB_TOKEN?.trim();
		if (token) {
			headers.Authorization = `Bearer ${token}`;
		}
		const res = await fetch('https://api.github.com/repos/GaryWenneker/ax', {
			headers,
			signal: controller.signal,
		});
		clearTimeout(timeout);
		if (!res.ok) return fallback;
		const data = (await res.json()) as { stargazers_count?: number };
		return typeof data.stargazers_count === 'number'
			? format(data.stargazers_count)
			: fallback;
	} catch {
		return fallback;
	}
}

let cached: Promise<string> | null = null;

/** Live star label, e.g. `"12"` or `"1,234"`. Empty string if fetch failed. */
export function getStarsLabel(fallback = ''): Promise<string> {
	cached ??= fetchStars(fallback);
	return cached;
}
