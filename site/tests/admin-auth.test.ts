import { test, beforeEach } from 'node:test';
import assert from 'node:assert/strict';
import { SignJWT, decodeJwt } from 'jose';
import { handler as login } from '../netlify/functions/admin-auth.ts';
import { handler as telemetry } from '../netlify/functions/admin-telemetry.ts';

const SECRET = 'test-admin-secret-0123456789abcdef';
const PASSWORD = 'correct horse battery staple';
let ipCounter = 0;

beforeEach(() => {
	process.env.ADMIN_PASSWORD = PASSWORD;
	process.env.ADMIN_SECRET = SECRET;
	process.env.POSTHOG_PERSONAL_KEY = 'phx_test';
	process.env.POSTHOG_PROJECT_ID = '1';
	process.env.POSTHOG_API_HOST = 'http://127.0.0.1:9';
});

function freshIp(): string {
	ipCounter += 1;
	return `198.51.100.${ipCounter}`;
}

const HOST = 'getax.wenneker.io';
type Res = { statusCode: number; headers: Record<string, string>; body: string };

async function attempt(password: string, ip: string, extra: Record<string, string> = {}) {
	return (await (login as any)({
		httpMethod: 'POST',
		headers: { 'x-nf-client-connection-ip': ip, host: HOST, origin: `https://${HOST}`, ...extra },
		body: JSON.stringify({ password }),
	})) as Res;
}

/** The admin token from a login's Set-Cookie header. */
function cookieToken(res: Res): string {
	const cookie = res.headers['Set-Cookie'] ?? '';
	const token = cookie.match(/^ax_admin=([^;]+)/)?.[1];
	assert.ok(token, `login sets the ax_admin cookie: ${cookie}`);
	return token;
}

async function dashboard(token: string, extra: Record<string, string> = {}) {
	const realFetch = globalThis.fetch;
	globalThis.fetch = (async () =>
		new Response(JSON.stringify({ results: [], columns: [] }), { status: 200 })) as typeof fetch;
	try {
		return (await (telemetry as any)({
			httpMethod: 'GET',
			headers: { cookie: `other=1; ax_admin=${token}`, host: HOST, ...extra },
			queryStringParameters: { type: 'dashboard', days: '7' },
		})) as Res;
	} finally {
		globalThis.fetch = realFetch;
	}
}

test('the right password sets an HttpOnly admin cookie that opens the dashboard', async () => {
	const res = await attempt(PASSWORD, freshIp());
	assert.equal(res.statusCode, 200);
	const cookie = res.headers['Set-Cookie'];
	for (const attr of ['HttpOnly', 'Secure', 'SameSite=Strict', 'Path=/v1/admin']) {
		assert.ok(cookie.split(/;\s*/).includes(attr), `cookie has ${attr}: ${cookie}`);
	}
	const token = cookieToken(res);
	assert.ok(!res.body.includes(token), 'the token never reaches page scripts');
	assert.equal(res.headers['Access-Control-Allow-Origin'], undefined);
	const claims = decodeJwt(token);
	assert.equal(claims.admin, true);
	assert.equal(claims.iss, 'getax-admin');
	assert.equal(claims.aud, 'getax-admin-dashboard');
	assert.equal((await dashboard(token)).statusCode, 200);
});

test('the dashboard API needs the cookie: a bearer header alone or no credentials get 401', async () => {
	const token = cookieToken(await attempt(PASSWORD, freshIp()));
	const realFetch = globalThis.fetch;
	globalThis.fetch = (async () => new Response('{"results":[]}', { status: 200 })) as typeof fetch;
	try {
		for (const headers of [{ authorization: `Bearer ${token}`, host: HOST }, { host: HOST }]) {
			const res = (await (telemetry as any)({ httpMethod: 'GET', headers, queryStringParameters: {} })) as Res;
			assert.equal(res.statusCode, 401);
		}
	} finally {
		globalThis.fetch = realFetch;
	}
});

test('requests from another origin are refused before the password or cookie is checked', async () => {
	const evil = { origin: 'https://evil.example' };
	assert.equal((await attempt(PASSWORD, freshIp(), evil)).statusCode, 403);
	assert.equal((await attempt(PASSWORD, freshIp(), { 'sec-fetch-site': 'cross-site' })).statusCode, 403);
	const token = cookieToken(await attempt(PASSWORD, freshIp()));
	assert.equal((await dashboard(token, evil)).statusCode, 403);
	assert.equal((await dashboard(token, { 'sec-fetch-site': 'same-site' })).statusCode, 403);
	assert.equal((await dashboard(token, { 'sec-fetch-site': 'same-origin' })).statusCode, 200);
});

test('signing out clears the admin cookie', async () => {
	const res = (await (login as any)({
		httpMethod: 'DELETE',
		headers: { host: HOST, origin: `https://${HOST}` },
	})) as Res;
	assert.equal(res.statusCode, 204);
	const cookie = res.headers['Set-Cookie'];
	assert.match(cookie, /^ax_admin=;/);
	assert.ok(cookie.split(/;\s*/).includes('Max-Age=0'), cookie);
	assert.ok(cookie.split(/;\s*/).includes('Path=/v1/admin'), cookie);
});

test('a wrong password is refused', async () => {
	const res = await attempt('wrong', freshIp());
	assert.equal(res.statusCode, 401);
});

test('repeated failures from one address are locked out, even with the right password', async () => {
	const ip = freshIp();
	for (let i = 0; i < 5; i++) {
		assert.equal((await attempt(`guess-${i}`, ip)).statusCode, 401, `attempt ${i + 1}`);
	}
	const locked = await attempt(PASSWORD, ip);
	assert.equal(locked.statusCode, 429);
	assert.ok(Number(locked.headers['Retry-After']) > 0);
	// Another address is not affected.
	assert.equal((await attempt(PASSWORD, freshIp())).statusCode, 200);
});

test('a successful login clears the failure count for that address', async () => {
	const ip = freshIp();
	for (let i = 0; i < 4; i++) await attempt(`guess-${i}`, ip);
	assert.equal((await attempt(PASSWORD, ip)).statusCode, 200);
	for (let i = 0; i < 4; i++) {
		assert.equal((await attempt(`again-${i}`, ip)).statusCode, 401);
	}
});

test('a token signed with the secret but without the admin claims is rejected', async () => {
	const key = new TextEncoder().encode(SECRET);
	const plain = await new SignJWT({ purpose: 'other' })
		.setProtectedHeader({ alg: 'HS256' })
		.setIssuedAt()
		.setExpirationTime('1h')
		.sign(key);
	assert.equal((await dashboard(plain)).statusCode, 401);

	const noAudience = await new SignJWT({ admin: true })
		.setProtectedHeader({ alg: 'HS256' })
		.setIssuer('getax-admin')
		.setIssuedAt()
		.setExpirationTime('1h')
		.sign(key);
	assert.equal((await dashboard(noAudience)).statusCode, 401);
});
