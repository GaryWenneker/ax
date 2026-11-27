// Shared by netlify/functions/admin-auth.ts and admin-telemetry.ts. Kept outside
// netlify/functions so Netlify does not deploy it as a function of its own.
import { createHash, timingSafeEqual } from 'node:crypto';
import { SignJWT, jwtVerify } from 'jose';

export const ADMIN_ISSUER = 'getax-admin';
export const ADMIN_AUDIENCE = 'getax-admin-dashboard';
export const ADMIN_TOKEN_TTL = '8h';

/** Failed logins allowed per client address within one window. */
export const MAX_FAILED_LOGINS = 5;
/** Length of the failure window, and of the lockout that follows it. */
export const LOGIN_WINDOW_MS = 15 * 60 * 1000;

/** Compares two passwords in constant time (both are hashed to equal-length digests). */
export function passwordMatches(given: string, expected: string): boolean {
	const a = createHash('sha256').update(given, 'utf8').digest();
	const b = createHash('sha256').update(expected, 'utf8').digest();
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
export async function verifyAdminToken(authHeader: string | undefined, secret: string): Promise<boolean> {
	if (!authHeader?.startsWith('Bearer ')) return false;
	try {
		const { payload } = await jwtVerify(authHeader.slice(7), new TextEncoder().encode(secret), {
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
