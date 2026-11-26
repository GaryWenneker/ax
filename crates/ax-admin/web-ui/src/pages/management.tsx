import { useCallback, useEffect, useState, type FormEvent } from 'react';
import { FilterBar, ItemList, ItemRow, PageCard, PageEmpty } from '@cc/components/ui/PageLayout';
import type { Tool } from '../api';
import { useActiveConnection } from '../activeConnection';
import Badge from '../components/Badge';
import { AutoCard, Frame, TabbedCard } from '../components/cards';
import ConfirmDialog from '../components/ConfirmDialog';
import ToolResultView from '../components/ToolResult';
import { logPageFrom } from '../remoteData';
import { useTool } from '../useTool';

const has = (tools: readonly Tool[], name: string) => tools.some((tool) => tool.name === name);

/** What the environment is told before each index-changing tool runs. */
export const CHANGING_TOOLS: Readonly<Record<string, string>> = {
  ax_sync: 'Re-indexes the files that changed. Agents may get slower answers while it runs.',
  ax_index: 'Rebuilding clears the index first. Agents get incomplete answers until it finishes.',
  ax_policy_index: "Reads rules and skills from the project's policy files into the environment, replacing what it holds.",
};

const NO_ARGS: Record<string, unknown> = {};

function Operation({ tool, title, about, args = NO_ARGS, button }: { tool: string; title: string; about: string; args?: Record<string, unknown>; button: string }) {
  const connection = useActiveConnection();
  const { state, run } = useTool();
  const [asking, setAsking] = useState(false);
  return (
    <section className="admin-op" aria-label={title}>
      <h3>{title}</h3>
      <p>{about}</p>
      <div className="admin-actions">
        {/* Stays focusable while running, so focus returning from the question has a place to land. */}
        <button type="button" className="btn" aria-disabled={state.status === 'loading'} onClick={() => { if (state.status !== 'loading') setAsking(true); }}>{button}</button>
      </div>
      <ToolResultView state={state} />
      {asking && (
        <ConfirmDialog
          title={`${title} on ${connection.name}?`}
          message={CHANGING_TOOLS[tool] ?? about}
          confirmLabel={`${button} on ${connection.name}`}
          onCancel={() => setAsking(false)}
          onConfirm={() => {
            setAsking(false);
            void run(tool, args, `${title} is running on ${connection.name}. This can take minutes on a large project.`);
          }}
        />
      )}
    </section>
  );
}

const REBUILD = { force: true };

export function OperationsPage({ tools }: { tools: readonly Tool[] }) {
  return (
    <Frame title="Operations" about="Changes the environment's index. Available because this connection has the admin grant.">
      <PageCard title="Index and policy" description="Each operation asks first. One runs at a time per project; a second one waits.">
        <div className="admin-ops">
          {has(tools, 'ax_sync') && <Operation tool="ax_sync" title="Sync" about="Re-indexes only the files that changed since the last run." button="Sync" />}
          {has(tools, 'ax_index') && <Operation tool="ax_index" title="Rebuild index" about="Clears the index and indexes every file again." args={REBUILD} button="Rebuild" />}
          {has(tools, 'ax_policy_index') && <Operation tool="ax_policy_index" title="Reload policy" about="Reads rules and skills from the project's policy files into the environment." button="Reload" />}
        </div>
      </PageCard>
    </Frame>
  );
}

export function FilesPage() {
  return (
    <Frame title="Files" about="The files this environment has indexed.">
      <AutoCard title="Indexed files" description="As listed by the environment." tool="ax_files" />
    </Frame>
  );
}

const USAGE_TABS = [
  { tool: 'ax_cost', label: 'Cost' },
  { tool: 'ax_tool_economics', label: 'Tool economics' },
  { tool: 'ax_budget', label: 'Budget' },
  { tool: 'ax_optimization_advice', label: 'Advice' },
] as const;

export function UsagePage({ tools }: { tools: readonly Tool[] }) {
  return (
    <Frame title="Usage" about="What agents spent and saved through this environment.">
      <TabbedCard name="usage" title="Usage reports" description="Computed by the environment from its own records." tabs={USAGE_TABS.filter((tab) => has(tools, tab.tool))} />
    </Frame>
  );
}

// "When" sorts on the time of the call, not on its text.
const LOG_COLUMNS = [{ label: 'Tool' }, { label: 'When' }, { label: 'Result' }] as const;

export function LoggingPage() {
  const { state, run } = useTool();
  const [draft, setDraft] = useState('');
  // The filter the shown page was loaded with; "Older" continues with it.
  const [applied, setApplied] = useState('');
  const [older, setOlder] = useState(false);

  const load = useCallback((tool: string, before: number | null) => {
    setApplied(tool);
    setOlder(before !== null);
    void run('ax_logs', { limit: 100, ...(tool ? { tool } : {}), ...(before === null ? {} : { before }) });
  }, [run]);
  useEffect(() => {
    load('', null);
  }, [load]);

  const page = state.status === 'done' && !state.result.isError ? logPageFrom(state.result.structuredContent) : null;
  const busy = state.status === 'loading';
  const submit = (event: FormEvent) => {
    event.preventDefault();
    load(draft.trim(), null);
  };
  return (
    <Frame title="Logging" about="MCP calls handled by this environment for this project. It shows what was called and how it went, never the content.">
      <PageCard
        title="MCP call log"
        description={applied ? `The latest calls of ${applied}.` : 'The latest calls.'}
        footer={
          <div className="admin-actions">
            <button type="button" className="btn btn-secondary" disabled={busy || !older} onClick={() => load(applied, null)}>Newest</button>
            <button type="button" className="btn btn-secondary" disabled={busy || !page || page.nextBefore === null} onClick={() => { if (page?.nextBefore != null) load(applied, page.nextBefore); }}>Older</button>
          </div>
        }
      >
        <form onSubmit={submit} role="search">
          <FilterBar>
            <input className="settings-input settings-input--grow" type="search" aria-label="Filter by tool" placeholder="Only this tool, for example ax_explore" value={draft} onChange={(e) => setDraft(e.target.value)} />
            <button type="submit" className="btn" disabled={busy}>Show</button>
          </FilterBar>
        </form>
        {!page ? (
          <ToolResultView state={state} />
        ) : page.entries.length === 0 ? (
          <PageEmpty title="No calls recorded" />
        ) : (
          <div className="admin-list admin-list--tall" data-testid="call-log">
            <ItemList className="calm-list" listId="ax-admin-call-log" columns={LOG_COLUMNS}>
              {page.entries.map((entry) => (
                <ItemRow
                  key={entry.id}
                  variant="graph"
                  static
                  title={entry.tool}
                  listValues={[entry.tool, entry.at, entry.ok ? 'ok' : 'error']}
                  subtitle={`${new Date(entry.at).toLocaleString()} · ${entry.transport || 'mcp'} · ${entry.durationMs === null ? 'no duration' : `${entry.durationMs} ms`} · ${entry.responseTokens} tokens`}
                  badges={entry.ok ? <Badge icon="info" label="ok" tone="ok" /> : <Badge icon="error" label="error" tone="danger" />}
                />
              ))}
            </ItemList>
          </div>
        )}
      </PageCard>
    </Frame>
  );
}
