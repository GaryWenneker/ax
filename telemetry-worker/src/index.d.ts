/**
 * ax telemetry ingest — telemetry.getax.wenneker.io
 *
 * This file is public on purpose: it is the exact code that receives ax's
 * anonymous usage telemetry, so anyone can audit what is (and is not) stored.
 * The schema contract lives in docs/design/telemetry.md.
 *
 * Guarantees enforced here:
 * - strict allowlist: unknown events are dropped, unknown properties are stripped
 * - the client IP is never read, logged, or forwarded
 * - per-machine rate limiting, bounded body/batch sizes
 * - forwarding happens off the response path (ctx.waitUntil); bodies are never logged
 */
declare const _default: ExportedHandler<Env>;
export default _default;
