import { ApiError, request } from '../api.ts';

type ReloadReply = { ok?: boolean; error?: string; startedPid?: number; connected?: boolean; hint?: string };

/** Restart the shared MCP daemon and describe the outcome for a toast. */
export async function reloadMcpDaemon(): Promise<{ kind: 'ok' | 'err'; text: string }> {
  let body: ReloadReply;
  try {
    body = (await request<ReloadReply | undefined>('POST', '/ops/mcp-reload')) ?? {};
  } catch (e) {
    const reply = e instanceof ApiError ? (e.body as ReloadReply | null) : null;
    if (reply?.hint && !reply.error) return { kind: 'err', text: reply.hint };
    return { kind: 'err', text: e instanceof Error ? e.message : 'MCP reload request failed' };
  }
  if (body.ok === false) return { kind: 'err', text: body.error || body.hint || 'MCP reload failed' };
  const pid = body.startedPid ? ` pid ${body.startedPid}` : '';
  return {
    kind: 'ok',
    text: `MCP daemon reloaded${pid}. ${body.hint || 'If the IDE is still DEGRADED, restart MCP servers there.'}`,
  };
}
