import type { Handler } from '@netlify/functions';
import { LoginLimiter, clientAddress, passwordMatches, signAdminToken } from '../admin-token.ts';

const limiter = new LoginLimiter();

const CORS_HEADERS = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Headers': 'Content-Type',
  'Access-Control-Allow-Methods': 'POST, OPTIONS',
};

const handler: Handler = async (event) => {
  if (event.httpMethod === 'OPTIONS') {
    return { statusCode: 204, headers: CORS_HEADERS, body: '' };
  }
  if (event.httpMethod !== 'POST') {
    return { statusCode: 405, headers: CORS_HEADERS, body: 'Method Not Allowed' };
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
    headers: { ...CORS_HEADERS, 'Content-Type': 'application/json' },
    body: JSON.stringify({ token }),
  };
};

export { handler };
