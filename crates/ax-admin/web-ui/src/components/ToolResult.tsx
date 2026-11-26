import { PageEmpty, PageLoading } from '@cc/components/ui/PageLayout';
import type { ToolState } from '../useTool';
import Markdown from './Markdown';

/** The outcome of one tool call: progress, an error, or the text and data it returned. */
export default function ToolResultView({ state, idle }: { state: ToolState; idle?: string }) {
  if (state.loading) return <PageLoading label={state.label ?? 'Asking the server…'} />;
  if (state.error) {
    return <div className="admin-result"><div className="settings-toast settings-toast--err" role="alert">{state.error}</div></div>;
  }
  const result = state.result;
  if (!result) return idle ? <PageEmpty title={idle} /> : null;
  const data = result.structuredContent;
  const hasData = data !== null && data !== undefined && !(typeof data === 'object' && Object.keys(data as object).length === 0);
  return (
    <div className={`admin-result${result.isError ? ' admin-result--error' : ''}`} data-testid="tool-result">
      {result.isError && <div className="settings-toast settings-toast--err" role="alert">The server reported an error for this call.</div>}
      {result.text ? <Markdown text={result.text} /> : <PageEmpty title="The server returned no text" />}
      {hasData && (
        <details className="admin-json">
          <summary>Structured data</summary>
          <div className="admin-md"><pre tabIndex={0}><code>{JSON.stringify(data, null, 2)}</code></pre></div>
        </details>
      )}
    </div>
  );
}
