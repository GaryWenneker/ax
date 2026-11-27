import type { Handler } from '@netlify/functions';
import {
  LoginLimiter, adminCookie, clearedAdminCookie, clientAddress, passwordMatches, sameOrigin, signAdminToken,
} from '../admin-token.ts';

const limiter = new LoginLimiter();

// Same-origin only: the admin pages live on this site, and the token travels in
// an HttpOnly cookie, so no cross-origin (CORS) access is offered.
const CORS_HEADERS = { 'Cache-Control': 'no-store' };

const handler: Handler = async (event) => {
  if (!sameOrigin(event.headers ?? {})) {
    return {
      statusCode: 403,
      headers: { ...CORS_HEADERS, 'Content-Type': 'application/json' },
      body: JSON.stringify({ error: 'Cross-origin request refused' }),
    };
  }
  // Sign out: the cookie is HttpOnly, so only the server can remove it.
  if (event.httpMethod === 'DELETE') {
    return { statusCode: 204, headers: { ...CORS_HEADERS, 'Set-Cookie': clearedAdminCookie() }, body: '' };
  }
  if (event.httpMethod !== 'POST') {
    return { statusCode: 405, headers: { ...CORS_HEADERS, Allow: 'POST, DELETE' }, body: 'Method Not Allowed' };
  }

  const adminPassword = process.env.ADMIN_PASSWORD;
  const adminSecret = process.env.ADMIN_SECRET;

  if (!adminPassword || !adminSecret) {
    console.error('ADMIN_PASSWORD or ADMIN_SECRET not configured');
    return {
      statusCode: 500,
      headers: { ...CORS_HEADERS, 'Content-Type': 'application/json' },
      body: JSON.stringify({ error: 'Server configuration error' }),
    };
  }

  let body: { password?: string };
  try {
    body = JSON.parse(event.body || '{}');
  } catch {
    return {
      statusCode: 400,
      headers: { ...CORS_HEADERS, 'Content-Type': 'application/json' },
      body: JSON.stringify({ error: 'Invalid JSON' }),
    };
  }

  const client = clientAddress(event.headers ?? {});
  const wait = limiter.retryAfter(client);
  if (wait > 0) {
    return {
      statusCode: 429,
      headers: { ...CORS_HEADERS, 'Content-Type': 'application/json', 'Retry-After': String(wait) },
      body: JSON.stringify({ error: 'Too many failed attempts. Try again later.' }),
    };
  }

  if (typeof body.password !== 'string' || !body.password || !passwordMatches(body.password, adminPassword)) {
    limiter.fail(client);
    return {
      statusCode: 401,
      headers: { ...CORS_HEADERS, 'Content-Type': 'application/json' },
      body: JSON.stringify({ error: 'Invalid password' }),
    };
  }

  limiter.succeed(client);
  const token = await signAdminToken(adminSecret);

  return {
    statusCode: 200,
    headers: { ...CORS_HEADERS, 'Content-Type': 'application/json', 'Set-Cookie': adminCookie(token) },
    body: JSON.stringify({ ok: true }),
  };
};

export { handler };
