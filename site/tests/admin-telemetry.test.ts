import { test, beforeEach } from 'node:test';
import assert from 'node:assert/strict';
import { signAdminToken } from '../netlify/admin-token.ts';
import { handler as telemetry } from '../netlify/functions/admin-telemetry.ts';

const SECRET = 'test-admin-secret-0123456789abcdef';

beforeEach(() => {
	process.env.ADMIN_SECRET = SECRET;
	process.env.POSTHOG_PERSONAL_KEY = 'phx_test';
	process.env.POSTHOG_PROJECT_ID = '1';
	process.env.POSTHOG_API_HOST = 'http://127.0.0.1:9';
});

type Res = { statusCode: number; body: string };

/** Calls the handler with a stubbed PostHog and returns the response plus every HogQL query sent. */
async function call(params: Record<string, string>, authorization?: string): Promise<{ res: Res; queries: string[] }> {
	const queries: string[] = [];
	const realFetch = globalThis.fetch;
	globalThis.fetch = (async (_url: unknown, init?: RequestInit) => {
		queries.push(JSON.parse(String(init?.body)).query.query);
		return new Response(JSON.stringify({ results: [], columns: [] }), { status: 200 });
	}) as typeof fetch;
	try {
		const headers: Record<string, string> = {};
		if (authorization) headers.authorization = authorization;
		const res = (await (telemetry as any)({ httpMethod: 'GET', headers, queryStringParameters: params })) as Res;
		return { res, queries };
	} finally {
		globalThis.fetch = realFetch;
	}
}

async function bearer(): Promise<string> {
	return `Bearer ${await signAdminToken(SECRET)}`;
}

test('a non-numeric days value falls back to 30 days instead of NaN', async () => {
	const { res, queries } = await call({ type: 'dashboard', days: 'x' }, await bearer());
	assert.equal(res.statusCode, 200);
	assert.ok(queries.length > 0);
	for (const q of queries) assert.ok(!q.includes('NaN'), q);
	const body = JSON.parse(res.body);
	assert.equal(body.overview.days, 30);
	assert.equal(body.timeline.installs.length, 30);
});

test('days is clamped to 1..90', async () => {
	const high = JSON.parse((await call({ days: '500' }, await bearer())).res.body);
	assert.equal(high.overview.days, 90);
	const low = JSON.parse((await call({ days: '-3' }, await bearer())).res.body);
	assert.equal(low.overview.days, 1);
});

test('an unknown type is refused before any PostHog query', async () => {
	const { res, queries } = await call({ type: 'foo' }, await bearer());
	assert.equal(res.statusCode, 400);
	assert.equal(queries.length, 0);
});

test('an unauthenticated caller does not learn which secrets are missing', async () => {
	delete process.env.POSTHOG_PERSONAL_KEY;
	delete process.env.POSTHOG_PROJECT_ID;
	const { res } = await call({ type: 'dashboard' });
	assert.equal(res.statusCode, 401);
	assert.ok(!res.body.includes('POSTHOG'), res.body);

	delete process.env.ADMIN_SECRET;
	const bare = (await call({ type: 'dashboard' })).res;
	assert.equal(bare.statusCode, 503);
	assert.ok(!bare.body.includes('missing_env'), bare.body);
	assert.ok(!bare.body.includes('ADMIN_SECRET'), bare.body);
});

test('a signed-in admin is told which PostHog settings are missing', async () => {
	delete process.env.POSTHOG_PROJECT_ID;
	const { res, queries } = await call({ type: 'dashboard' }, await bearer());
	assert.equal(res.statusCode, 503);
	assert.deepEqual(JSON.parse(res.body).missing_env, ['POSTHOG_PROJECT_ID']);
	assert.equal(queries.length, 0);
});
