import { memo } from 'react';
import { PageEmpty, PageLoading } from '@cc/components/ui/PageLayout';
import type { ToolResult } from '../api';
import type { ToolState } from '../useTool';
import JsonTree from './JsonTree';
import Markdown from './Markdown';

function hasData(data: unknown): boolean {
  if (data === null || data === undefined) return false;
  return typeof data !== 'object' || Object.keys(data).length > 0;
}

const Result = memo(function Result({ result }: { result: ToolResult }) {
  return (
    <div className={`admin-result${result.isError ? ' admin-result--error' : ''}`} data-testid="tool-result" data-outcome={result.isError ? 'error' : 'ok'}>
      {result.isError && <div className="settings-toast settings-toast--err" role="alert">The server reported an error for this call.</div>}
      {result.text ? <Markdown text={result.text} /> : <PageEmpty title="The server returned no text" />}
      {hasData(result.structuredContent) && (
        <details className="admin-json">
          <summary>Structured data</summary>
          <JsonTree value={result.structuredContent} />
        </details>
      )}
    </div>
  );
});

/** The outcome of one tool call: progress, an error, or the text and data it returned. */
export default function ToolResultView({ state, idle }: { state: ToolState; idle?: string }) {
  switch (state.status) {
    case 'loading':
      return <PageLoading label={state.label ?? 'Asking the server…'} />;
    case 'error':
      return <div className="admin-result"><div className="settings-toast settings-toast--err" role="alert">{state.message}</div></div>;
    case 'done':
      return <Result result={state.result} />;
    case 'idle':
      return idle ? <PageEmpty title={idle} /> : null;
    default: {
      const unknown: never = state;
      return unknown;
    }
  }
}
