import { useCallback, useEffect, useRef, useState } from 'react';
import { ApiError, callTool, type ToolResult } from './api';
import { useActiveConnection } from './activeConnection';

export type ToolState =
  | { status: 'idle' }
  | { status: 'loading'; label: string | null }
  | { status: 'error'; message: string }
  | { status: 'done'; result: ToolResult };

const IDLE: ToolState = { status: 'idle' };

/**
 * Runs tools for the environment the page shows and keeps only the answer to
 * the latest request. An answer that arrives after the page is gone, or after
 * a newer request, is dropped.
 */
export function useTool() {
  const connection = useActiveConnection();
  const [state, setState] = useState<ToolState>(IDLE);
  const latest = useRef(0);
  useEffect(() => () => { latest.current = -1; }, []);

  const { id, changed } = connection;
  const run = useCallback(async (name: string, args: Record<string, unknown> = {}, label: string | null = null) => {
    const ticket = ++latest.current;
    setState({ status: 'loading', label });
    try {
      const result = await callTool(id, name, args);
      if (ticket === latest.current) setState({ status: 'done', result });
    } catch (e) {
      if (e instanceof ApiError && e.code === 'connection_changed') changed();
      if (ticket === latest.current) {
        setState({ status: 'error', message: e instanceof Error ? e.message : 'The call failed' });
      }
    }
  }, [id, changed]);

  return { state, run };
}
