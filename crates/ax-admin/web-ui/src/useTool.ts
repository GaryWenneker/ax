import { useCallback, useRef, useState } from 'react';
import { ApiError, callTool, type ToolResult } from './api';

export interface ToolState {
  loading: boolean;
  label?: string;
  error: string | null;
  result: ToolResult | null;
}

const IDLE: ToolState = { loading: false, error: null, result: null };

/** Runs one tool at a time and keeps only the answer to the latest request. */
export function useTool() {
  const [state, setState] = useState<ToolState>(IDLE);
  const latest = useRef(0);
  const run = useCallback(async (name: string, args: Record<string, unknown> = {}, label?: string) => {
    const ticket = ++latest.current;
    setState({ loading: true, label, error: null, result: null });
    try {
      const result = await callTool(name, args);
      if (ticket === latest.current) setState({ loading: false, error: null, result });
      return result;
    } catch (e) {
      const message = e instanceof ApiError || e instanceof Error ? e.message : 'The call failed';
      if (ticket === latest.current) setState({ loading: false, error: message, result: null });
      return null;
    }
  }, []);
  const reset = useCallback(() => {
    latest.current += 1;
    setState(IDLE);
  }, []);
  return { state, run, reset };
}
