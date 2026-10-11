/**
 * Shared telemetry ingest logic (mirrors telemetry-worker/src/index.ts).
 * Used by Netlify Functions at getax.wenneker.io/v1/events
 */
export declare const INFO_TEXT = "ax anonymous-telemetry ingest.\n\nWhat gets collected (and what never does) is documented field-by-field:\nhttps://github.com/GaryWenneker/ax/blob/main/docs/TELEMETRY.md\n\nDisable any time: ax telemetry off  |  AX_TELEMETRY=0  |  DO_NOT_TRACK=1\n";
export interface TelemetryResponse {
    statusCode: number;
    body?: string;
    headers?: Record<string, string>;
}
/** Handle telemetry ingest for Netlify Functions (path /v1/events). */
export declare function handleTelemetryIngest(method: string, path: string, bodyRaw: string | null, contentLengthHeader: string | null): Promise<TelemetryResponse>;
