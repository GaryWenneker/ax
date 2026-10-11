import { handleTelemetryIngest } from '../lib/ingest';
const handler = async (event) => {
    let path = event.path;
    if (path.includes('telemetry-ingest')) {
        path = event.httpMethod === 'GET' ? '/telemetry' : '/v1/events';
    }
    const res = await handleTelemetryIngest(event.httpMethod, path, event.body, event.headers['content-length'] ?? null);
    return {
        statusCode: res.statusCode,
        body: res.body ?? '',
        headers: res.headers,
    };
};
export { handler };
//# sourceMappingURL=telemetry-ingest.js.map