// Shared by netlify/functions/admin-auth.ts and admin-telemetry.ts. Kept outside
// netlify/functions so Netlify does not deploy it as a function of its own.
import { randomBytes, scryptSync, timingSafeEqual } from 'node:crypto';
import { SignJWT, jwtVerify } from 'jose';

export const ADMIN_ISSUER = 'getax-admin';
export const ADMIN_AUDIENCE = 'getax-admin-dashboard';
export const ADMIN_TOKEN_TTL = '8h';

/** The cookie that carries the admin token. HttpOnly, so page scripts never see it. */
export const ADMIN_COOKIE = 'ax_admin';
/** Only the admin API receives the cookie, never the documentation pages. */
export const ADMIN_COOKIE_PATH = '/v1/admin';
const ADMIN_COOKIE_MAX_AGE = 8 * 60 * 60;

/** Set-Cookie value that hands the browser the admin token for ADMIN_TOKEN_TTL. */
export function adminCookie(token: string): string {
	return `${ADMIN_COOKIE}=${token}; Path=${ADMIN_COOKIE_PATH}; HttpOnly; Secure; SameSite=Strict; Max-Age=${ADMIN_COOKIE_MAX_AGE}`;
}

/** Set-Cookie value that removes the admin cookie (sign out). */
export function clearedAdminCookie(): string {
	return `${ADMIN_COOKIE}=; Path=${ADMIN_COOKIE_PATH}; HttpOnly; Secure; SameSite=Strict; Max-Age=0`;
}

/** The admin token from the request's Cookie header, if any. */
export function adminCookieToken(headers: Record<string, string | undefined>): string | undefined {
	for (const part of (headers.cookie ?? '').split(';')) {
		const eq = part.indexOf('=');
		if (eq > 0 && part.slice(0, eq).trim() === ADMIN_COOKIE) return part.slice(eq + 1).trim() || undefined;
	}
	return undefined;
}

/**
 * False when the browser says the request comes from another site: an Origin
 * whose host differs from the request's Host, or a Sec-Fetch-Site other than
 * same-origin or none. Requests without either header (curl, tests) pass;
 * they carry no ambient cookie unless the caller sets one.
 */
export function sameOrigin(headers: Record<string, string | undefined>): boolean {
	const site = headers['sec-fetch-site'];
	if (site && site !== 'same-origin' && site !== 'none') return false;
	const origin = headers.origin;
	if (!origin) return true;
	const hosts = [headers.host, headers['x-forwarded-host']].filter(Boolean);
	try {
		return hosts.includes(new URL(origin).host);
	} catch {
		return false;
	}
}

/** Failed logins allowed per client address within one window. */
export const MAX_FAILED_LOGINS = 5;
/** Length of the failure window, and of the lockout that follows it. */
export const LOGIN_WINDOW_MS = 15 * 60 * 1000;

/** Random salt for this function instance; the derived keys never leave memory. */
const PASSWORD_SALT = randomBytes(16);

/**
 * Compares two passwords in constant time. Both go through scrypt with the same
 * salt, so the comparison is between equal-length keys and a fast hash is never
 * applied to a password.
 */
export function passwordMatches(given: string, expected: string): boolean {
	const a = scryptSync(given, PASSWORD_SALT, 32);
	const b = scryptSync(expected, PASSWORD_SALT, 32);
	return timingSafeEqual(a, b);
}

export async function signAdminToken(secret: string): Promise<string> {
	return new SignJWT({ admin: true })
		.setProtectedHeader({ alg: 'HS256' })
		.setIssuer(ADMIN_ISSUER)
		.setAudience(ADMIN_AUDIENCE)
		.setIssuedAt()
		.setExpirationTime(ADMIN_TOKEN_TTL)
		.sign(new TextEncoder().encode(secret));
}

/** True only for an unexpired HS256 token from the admin login with `admin: true`. */
export async function verifyAdminToken(token: string | undefined, secret: string): Promise<boolean> {
	if (!token) return false;
	try {
		const { payload } = await jwtVerify(token, new TextEncoder().encode(secret), {
			algorithms: ['HS256'],
			issuer: ADMIN_ISSUER,
			audience: ADMIN_AUDIENCE,
		});
		return payload.admin === true;
	} catch {
		return false;
	}
}

/**
 * Counts failed logins per client address. After MAX_FAILED_LOGINS failures in
 * LOGIN_WINDOW_MS the address is locked out until the window ends. The state
 * lives in the function instance, so it slows guessing on each warm instance;
 * it is not a global counter.
 */
export class LoginLimiter {
	private readonly failures = new Map<string, { count: number; since: number }>();
	private readonly max: number;
	private readonly windowMs: number;
	private readonly maxEntries: number;

	constructor(max = MAX_FAILED_LOGINS, windowMs = LOGIN_WINDOW_MS, maxEntries = 10_000) {
		this.max = max;
		this.windowMs = windowMs;
		this.maxEntries = maxEntries;
	}

	/** Seconds until `key` may try again, or 0 when it is not locked out. */
	retryAfter(key: string, now = Date.now()): number {
		const entry = this.failures.get(key);
		if (!entry) return 0;
		const end = entry.since + this.windowMs;
		if (now >= end) {
			this.failures.delete(key);
			return 0;
		}
		return entry.count >= this.max ? Math.ceil((end - now) / 1000) : 0;
	}

	fail(key: string, now = Date.now()): void {
		const entry = this.failures.get(key);
		if (entry && now < entry.since + this.windowMs) {
			entry.count += 1;
			return;
		}
		if (this.failures.size >= this.maxEntries) this.prune(now);
		this.failures.set(key, { count: 1, since: now });
	}

	succeed(key: string): void {
		this.failures.delete(key);
	}

	private prune(now: number): void {
		for (const [key, entry] of this.failures) {
			if (now >= entry.since + this.windowMs) this.failures.delete(key);
		}
		// Still full: drop the oldest entries so memory stays bounded.
		for (const key of this.failures.keys()) {
			if (this.failures.size < this.maxEntries) break;
			this.failures.delete(key);
		}
	}
}

/** The caller's address as Netlify reports it. */
export function clientAddress(headers: Record<string, string | undefined>): string {
	const direct = headers['x-nf-client-connection-ip'];
	if (direct) return direct.trim();
	const forwarded = headers['x-forwarded-for']?.split(',')[0]?.trim();
	return forwarded || 'unknown';
}
