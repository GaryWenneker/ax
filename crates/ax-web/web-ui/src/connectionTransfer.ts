export interface ConnectionDetails { kind: 'axRemoteConnection'; version: 1; projectMcpUrl: string; nativeClientId: string | null }
export function parseConnectionDetails(text: string): ConnectionDetails {
  if (text.length > 16 * 1024) throw new Error('Connection details are too large.');
  let value: unknown;
  try { value = JSON.parse(text); } catch { throw new Error('Paste the connection details copied from Ax, starting with { and ending with }.'); }
  if (typeof value !== 'object' || value === null || Array.isArray(value)) throw new Error('Expected Ax connection details.');
  const fields: [string, unknown][] = Object.entries(value);
  if (fields.some(([key]) => !['kind', 'version', 'projectMcpUrl', 'nativeClientId'].includes(key))) throw new Error('Use Copy connection details in Ax. This file contains other settings.');
  const kind = fields.find(([key]) => key === 'kind')?.[1];
  const version = fields.find(([key]) => key === 'version')?.[1];
  const url = fields.find(([key]) => key === 'projectMcpUrl')?.[1];
  const id = fields.find(([key]) => key === 'nativeClientId')?.[1];
  if (kind !== 'axRemoteConnection' || version !== 1 || typeof url !== 'string' || !(id === null || typeof id === 'string')) throw new Error('Use the connection details copied from Ax, rather than a private server file or IDE configuration.');
  let endpoint: URL;
  try { endpoint = new URL(url); } catch { throw new Error('Connection details contain an invalid MCP URL.'); }
  if (endpoint.protocol !== 'https:' || endpoint.username || endpoint.password || endpoint.search || endpoint.hash || !/^\/projects\/[a-zA-Z0-9_-]{1,64}\/mcp$/.test(endpoint.pathname)) throw new Error('Expected an HTTPS project MCP URL without credentials or additional parameters.');
  if (typeof id === 'string' && (!id.trim() || id.length > 512 || /[\u0000-\u001f\u007f]/.test(id))) throw new Error('Connection details contain an invalid public Ax client ID.');
  return { kind, version, projectMcpUrl: endpoint.href, nativeClientId: id };
}
