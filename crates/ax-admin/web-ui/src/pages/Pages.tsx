import { useEffect, useMemo, useState, type FormEvent, type ReactNode } from 'react';
import { FilterBar, PageCard, PageEmpty, PageHero, PageShell, PageStack } from '@cc/components/ui/PageLayout';
import Codicon from '@cc/components/Codicon';
import type { Tool } from '../api';
import type { PageId } from '../pages';
import { argumentsFromValues, fieldsFromSchema, type FormValues } from '../schemaForm';
import { useTool } from '../useTool';
import Badge from '../components/Badge';
import ToolResultView from '../components/ToolResult';

export interface PageProps {
  tools: Tool[];
}

const has = (tools: Tool[], name: string) => tools.some((tool) => tool.name === name);

function Frame({ title, about, children }: { title: string; about: string; children: ReactNode }) {
  return (
    <PageShell>
      <PageHero title={title} subtitle={about} />
      <PageStack>{children}</PageStack>
    </PageShell>
  );
}

/** One text box, one tool. */
function QueryCard({
  title,
  description,
  tool,
  argument,
  placeholder,
  idle,
  extra,
}: {
  title: string;
  description: string;
  tool: string;
  argument: string;
  placeholder: string;
  idle: string;
  extra?: Record<string, unknown>;
}) {
  const { state, run } = useTool();
  const [value, setValue] = useState('');
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (value.trim()) void run(tool, { ...extra, [argument]: value.trim() });
  };
  return (
    <PageCard title={title} description={description}>
      <form onSubmit={submit} role="search">
        <FilterBar>
          <input
            className="settings-input settings-input--grow"
            type="search"
            aria-label={title}
            placeholder={placeholder}
            value={value}
            onChange={(e) => setValue(e.target.value)}
          />
          <button type="submit" className="btn" disabled={state.loading || !value.trim()}>Run</button>
        </FilterBar>
      </form>
      <ToolResultView state={state} idle={idle} />
    </PageCard>
  );
}

/** A tool without input that runs when the page opens. */
function AutoCard({ title, description, tool, args }: { title: string; description: string; tool: string; args?: Record<string, unknown> }) {
  const { state, run } = useTool();
  useEffect(() => {
    void run(tool, args ?? {});
    // Runs once per page visit; Refresh repeats it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tool]);
  return (
    <PageCard
      title={title}
      description={description}
      footer={<button type="button" className="btn btn-secondary" disabled={state.loading} onClick={() => void run(tool, args ?? {})}><Codicon name="refresh" /> Refresh</button>}
    >
      <ToolResultView state={state} />
    </PageCard>
  );
}

/** Several tools without input, one shown at a time. */
function TabbedCard({ title, description, tabs }: { title: string; description: string; tabs: { tool: string; label: string; args?: Record<string, unknown> }[] }) {
  const { state, run } = useTool();
  const [active, setActive] = useState(tabs[0]?.tool ?? '');
  const current = tabs.find((tab) => tab.tool === active) ?? tabs[0];
  useEffect(() => {
    if (current) void run(current.tool, current.args ?? {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [current?.tool]);
  if (!current) return null;
  return (
    <PageCard title={title} description={description}>
      <div className="admin-tabs" role="tablist" aria-label={title}>
        {tabs.map((tab) => (
          <button key={tab.tool} type="button" role="tab" className="admin-tab" aria-selected={tab.tool === current.tool} onClick={() => setActive(tab.tool)}>
            {tab.label}
          </button>
        ))}
      </div>
      <ToolResultView state={state} />
    </PageCard>
  );
}

/** A form generated from the tool's own input schema. */
function SchemaCard({ tool, title, description, submit = 'Run' }: { tool: Tool; title?: string; description?: string; submit?: string }) {
  const fields = useMemo(() => fieldsFromSchema(tool.inputSchema), [tool]);
  const [values, setValues] = useState<FormValues>({});
  const [problem, setProblem] = useState<string | null>(null);
  const { state, run, reset } = useTool();
  useEffect(() => {
    setValues({});
    setProblem(null);
    reset();
  }, [tool.name, reset]);
  const onSubmit = (event: FormEvent) => {
    event.preventDefault();
    try {
      const args = argumentsFromValues(fields, values);
      setProblem(null);
      void run(tool.name, args);
    } catch (e) {
      setProblem(e instanceof Error ? e.message : 'Check the form');
    }
  };
  return (
    <PageCard title={title ?? tool.name} description={description ?? tool.description}>
      <form className="admin-form" onSubmit={onSubmit} aria-label={title ?? tool.name}>
        {problem && <div className="settings-toast settings-toast--err" role="alert">{problem}</div>}
        {fields.length === 0 && <p className="admin-note">This tool takes no input.</p>}
        {fields.map((field) => {
          const id = `field-${tool.name}-${field.name}`;
          const label = <>{field.name}{field.required ? ' *' : ''}</>;
          if (field.kind === 'boolean') {
            return (
              <label key={field.name} className="admin-choice" title={field.description}>
                <input type="checkbox" checked={values[field.name] === true} onChange={(e) => setValues({ ...values, [field.name]: e.target.checked })} />
                {label}
              </label>
            );
          }
          const text = typeof values[field.name] === 'string' ? (values[field.name] as string) : '';
          const change = (value: string) => setValues({ ...values, [field.name]: value });
          return (
            <div key={field.name} className="admin-field">
              <label htmlFor={id}>{label}</label>
              {field.kind === 'enum' ? (
                <select id={id} className="settings-input" value={text} onChange={(e) => change(e.target.value)}>
                  <option value="">{field.required ? 'Choose…' : '(not set)'}</option>
                  {field.options.map((option) => <option key={option} value={option}>{option}</option>)}
                </select>
              ) : field.kind === 'json' ? (
                <textarea id={id} className="settings-input" placeholder="JSON" spellCheck={false} value={text} onChange={(e) => change(e.target.value)} />
              ) : (
                <input id={id} className="settings-input" type={field.kind === 'number' ? 'number' : 'text'} value={text} onChange={(e) => change(e.target.value)} />
              )}
              {field.description && <span className="admin-field-hint">{field.description}</span>}
            </div>
          );
        })}
        <div className="admin-actions">
          <button type="submit" className="btn" disabled={state.loading}>{submit}</button>
        </div>
      </form>
      <ToolResultView state={state} />
    </PageCard>
  );
}

function StatusPage({ tools }: PageProps) {
  return (
    <Frame title="Status" about="What this environment has indexed, and how fresh it is.">
      {has(tools, 'ax_status') && <AutoCard title="Index" description="Files, symbols and the last index run." tool="ax_status" />}
      {has(tools, 'ax_cache_status') && <AutoCard title="Caches" description="Context cache and file-token cache counts." tool="ax_cache_status" />}
    </Frame>
  );
}

function SearchPage() {
  return (
    <Frame title="Search" about="Full-text search across the indexed symbols of this environment.">
      <QueryCard title="Symbol search" description="Functions, classes, methods and other symbols." tool="ax_search" argument="query" placeholder="Search symbols…" idle="Type a name and run the search" />
    </Frame>
  );
}

function NodesPage({ tools }: PageProps) {
  const actions = [
    { tool: 'ax_node', argument: 'name', label: 'Source' },
    { tool: 'ax_callers', argument: 'symbol', label: 'Callers' },
    { tool: 'ax_callees', argument: 'symbol', label: 'Callees' },
    { tool: 'ax_impact', argument: 'symbol', label: 'Impact' },
  ].filter((action) => has(tools, action.tool));
  const { state, run } = useTool();
  const [symbol, setSymbol] = useState('');
  const [active, setActive] = useState(actions[0]?.tool ?? '');
  const go = (tool: string) => {
    const action = actions.find((a) => a.tool === tool);
    setActive(tool);
    if (action && symbol.trim()) void run(action.tool, { [action.argument]: symbol.trim() });
  };
  return (
    <Frame title="Nodes" about="Open one symbol: its source, who calls it, what it calls and what a change would touch.">
      <PageCard title="Symbol" description="Enter a symbol name, then choose what to look at.">
        <form onSubmit={(e) => { e.preventDefault(); go(active); }} role="search">
          <FilterBar>
            <input className="settings-input settings-input--grow" type="search" aria-label="Symbol name" placeholder="Symbol name, for example parse_config" value={symbol} onChange={(e) => setSymbol(e.target.value)} />
            <button type="submit" className="btn" disabled={state.loading || !symbol.trim()}>Open</button>
          </FilterBar>
        </form>
        <div className="admin-tabs" role="tablist" aria-label="View">
          {actions.map((action) => (
            <button key={action.tool} type="button" role="tab" className="admin-tab" aria-selected={action.tool === active} onClick={() => go(action.tool)}>{action.label}</button>
          ))}
        </div>
        <ToolResultView state={state} idle="Enter a symbol name" />
      </PageCard>
    </Frame>
  );
}

function ExplorePage() {
  return (
    <Frame title="Explore" about="Ask how something works. The answer is the relevant source with its call path.">
      <QueryCard title="Question or symbols" description="Semantic search plus graph traversal." tool="ax_explore" argument="query" placeholder="How is the configuration loaded?" idle="Ask a question about the code" />
    </Frame>
  );
}

function InsightsPage({ tools }: PageProps) {
  const tabs = [
    { tool: 'ax_insights', label: 'Insights' },
    { tool: 'ax_report', label: 'Architecture report' },
  ].filter((tab) => has(tools, tab.tool));
  return (
    <Frame title="Insights" about="Subsystems, the most connected symbols and surprising links across the whole graph.">
      <TabbedCard title="Whole-graph view" description="Computed by the environment from its current index." tabs={tabs} />
    </Frame>
  );
}

function RulesPage() {
  return (
    <Frame title="Rules" about="The policy rules this environment gives to agents.">
      <AutoCard title="Rules" description="All rules, with their level and scope." tool="ax_rules" />
      <QueryCard title="Match a prompt" description="Which rules would apply to this request?" tool="ax_rules" argument="prompt" placeholder="A request an agent might receive" idle="Enter a prompt to see the matching rules" />
    </Frame>
  );
}

function SkillsPage() {
  return (
    <Frame title="Skills" about="Named workflows the environment can hand to an agent.">
      <QueryCard title="Open a skill" description="Loads the skill's full text by name." tool="ax_skill" argument="name" placeholder="Skill name, for example startup" idle="Enter a skill name" />
    </Frame>
  );
}

function MemoryPage({ tools }: PageProps) {
  const remember = tools.find((tool) => tool.name === 'ax_remember');
  return (
    <Frame title="Memory" about="Decisions, fixes and conventions stored in this environment's memory vault.">
      {has(tools, 'ax_recall') && <QueryCard title="Recall" description="Search memories by text." tool="ax_recall" argument="query" placeholder="What was decided about…" idle="Search the memory vault" />}
      {has(tools, 'ax_history') && <QueryCard title="History" description="Dated turns and commits that touched a file, symbol or topic." tool="ax_history" argument="query" placeholder="File path, symbol or topic" idle="Look up when something changed" />}
      {remember && <SchemaCard tool={remember} title="Add memory" description="Stores a memory in the environment. Shown because this connection has write access." submit="Store memory" />}
    </Frame>
  );
}

function SessionsPage({ tools }: PageProps) {
  const durable = tools.find((tool) => tool.name === 'ax_durable');
  return (
    <Frame title="Sessions" about="Working context and durable transcripts kept by the environment for this connection.">
      {has(tools, 'ax_session') && <AutoCard title="Working context" description="Objective, facts, decisions and open questions of this connection's chat." tool="ax_session" args={{ action: 'get' }} />}
      {durable && <SchemaCard tool={durable} title="Durable store" description="Transcript, documents and tasks. Choose an action and fill in what it needs." />}
    </Frame>
  );
}

function Operation({ tool, title, about, args, confirm, button }: { tool: string; title: string; about: string; args: Record<string, unknown>; confirm?: string; button: string }) {
  const { state, run } = useTool();
  const [asking, setAsking] = useState(false);
  const start = () => {
    setAsking(false);
    void run(tool, args, `${title} is running. This can take minutes on a large project.`);
  };
  return (
    <section className="admin-op" aria-label={title}>
      <h3>{title}</h3>
      <p>{about}</p>
      {asking ? (
        <div className="admin-actions" role="alertdialog" aria-label={`Confirm ${title}`}>
          <span className="admin-note">{confirm}</span>
          <button type="button" className="btn btn-danger-subtle" onClick={start}>Yes, {button.toLowerCase()}</button>
          <button type="button" className="btn btn-subtle" onClick={() => setAsking(false)}>Cancel</button>
        </div>
      ) : (
        <div className="admin-actions">
          <button type="button" className="btn" disabled={state.loading} onClick={() => (confirm ? setAsking(true) : start())}>{button}</button>
        </div>
      )}
      <ToolResultView state={state} />
    </section>
  );
}

function OperationsPage({ tools }: PageProps) {
  return (
    <Frame title="Operations" about="Changes the environment's index. Available because this connection has the admin grant.">
      <PageCard title="Index and policy" description="One index-changing operation runs at a time per project; a second one waits.">
        <div className="admin-ops">
          {has(tools, 'ax_sync') && <Operation tool="ax_sync" title="Sync" about="Re-indexes only the files that changed since the last run." args={{}} button="Sync" />}
          {has(tools, 'ax_index') && (
            <Operation
              tool="ax_index"
              title="Rebuild index"
              about="Clears the index and indexes every file again."
              args={{ force: true }}
              button="Rebuild"
              confirm="Rebuilding clears the index first. Agents get incomplete answers until it finishes."
            />
          )}
          {has(tools, 'ax_policy_index') && <Operation tool="ax_policy_index" title="Reload policy" about="Reads rules and skills from the project's policy files into the environment." args={{}} button="Reload" />}
        </div>
      </PageCard>
    </Frame>
  );
}

function FilesPage() {
  return (
    <Frame title="Files" about="The files this environment has indexed.">
      <AutoCard title="Indexed files" description="As listed by the environment." tool="ax_files" />
    </Frame>
  );
}

function UsagePage({ tools }: PageProps) {
  const tabs = [
    { tool: 'ax_cost', label: 'Cost' },
    { tool: 'ax_tool_economics', label: 'Tool economics' },
    { tool: 'ax_budget', label: 'Budget' },
    { tool: 'ax_optimization_advice', label: 'Advice' },
  ].filter((tab) => has(tools, tab.tool));
  return (
    <Frame title="Usage" about="What agents spent and saved through this environment.">
      <TabbedCard title="Usage reports" description="Computed by the environment from its own records." tabs={tabs} />
    </Frame>
  );
}

interface LogEntry {
  id: number;
  at: number;
  tool: string;
  transport: string;
  ok: boolean;
  durationMs: number | null;
  responseTokens: number;
}

function LoggingPage() {
  const { state, run } = useTool();
  const [filter, setFilter] = useState('');
  const [cursor, setCursor] = useState<number[]>([]);
  const load = (before?: number) => {
    const args: Record<string, unknown> = { limit: 100 };
    if (filter.trim()) args.tool = filter.trim();
    if (before !== undefined) args.before = before;
    void run('ax_logs', args);
  };
  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const data = (state.result?.structuredContent ?? null) as { entries?: LogEntry[]; nextBefore?: number | null } | null;
  const entries = Array.isArray(data?.entries) ? data.entries : null;
  const next = typeof data?.nextBefore === 'number' ? data.nextBefore : null;
  return (
    <Frame title="Logging" about="MCP calls handled by this environment for this project. It shows what was called and how it went, never the content.">
      <PageCard
        title="MCP call log"
        description="Newest first."
        footer={
          <div className="admin-actions">
            <button type="button" className="btn btn-secondary" disabled={state.loading || cursor.length === 0} onClick={() => { setCursor([]); load(); }}>Newest</button>
            <button type="button" className="btn btn-secondary" disabled={state.loading || next === null} onClick={() => { if (next !== null) { setCursor([...cursor, next]); load(next); } }}>Older</button>
          </div>
        }
      >
        <form onSubmit={(e) => { e.preventDefault(); setCursor([]); load(); }} role="search">
          <FilterBar>
            <input className="settings-input settings-input--grow" type="search" aria-label="Filter by tool" placeholder="Only this tool, for example ax_explore" value={filter} onChange={(e) => setFilter(e.target.value)} />
            <button type="submit" className="btn" disabled={state.loading}>Show</button>
          </FilterBar>
        </form>
        {!entries || state.loading || state.error ? (
          <ToolResultView state={state} />
        ) : entries.length === 0 ? (
          <PageEmpty title="No calls recorded" />
        ) : (
          <div className="admin-result">
            <div className="admin-table-wrap" tabIndex={0}>
              <table className="admin-table" aria-label="MCP calls">
                <thead>
                  <tr><th scope="col">Time</th><th scope="col">Tool</th><th scope="col">Result</th><th scope="col">Transport</th><th scope="col" className="num">Duration</th><th scope="col" className="num">Tokens</th></tr>
                </thead>
                <tbody>
                  {entries.map((entry) => (
                    <tr key={entry.id}>
                      <td>{new Date(entry.at).toLocaleString()}</td>
                      <td><code>{entry.tool}</code></td>
                      <td>{entry.ok ? <Badge icon="info" label="ok" tone="ok" /> : <Badge icon="error" label="error" tone="danger" />}</td>
                      <td>{entry.transport}</td>
                      <td className="num">{entry.durationMs === null ? '-' : `${entry.durationMs} ms`}</td>
                      <td className="num">{entry.responseTokens}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        )}
      </PageCard>
    </Frame>
  );
}

function ToolsPage({ tools }: PageProps) {
  const sorted = useMemo(() => [...tools].sort((a, b) => a.name.localeCompare(b.name)), [tools]);
  const [selected, setSelected] = useState(sorted[0]?.name ?? '');
  const tool = sorted.find((t) => t.name === selected) ?? sorted[0];
  return (
    <Frame title="Tools" about="Every tool the environment offers to this connection, including tools this application has no page for.">
      <PageCard title={`${sorted.length} tools`} description="Choose a tool to see its input and run it.">
        {sorted.length === 0 ? (
          <PageEmpty title="The environment offers no tools to this connection" />
        ) : (
          <div className="admin-split">
            <div className="admin-list" role="listbox" aria-label="Tools">
              {sorted.map((t) => (
                <button key={t.name} type="button" role="option" aria-selected={t.name === tool?.name} className={`admin-row${t.name === tool?.name ? ' admin-row--selected' : ''}`} onClick={() => setSelected(t.name)}>
                  <span className="admin-row-main">
                    <span className="admin-row-title">{t.name}</span>
                    <span className="admin-row-sub">{t.description ?? ''}</span>
                  </span>
                  {t.annotations?.readOnlyHint === false && <span className="admin-row-badges"><Badge icon="warning" label="changes data" tone="warn" /></span>}
                </button>
              ))}
            </div>
            {tool && <SchemaCard tool={tool} />}
          </div>
        )}
      </PageCard>
    </Frame>
  );
}

export function PageView({ page, tools }: { page: PageId; tools: Tool[] }) {
  switch (page) {
    case 'status': return <StatusPage tools={tools} />;
    case 'search': return <SearchPage />;
    case 'nodes': return <NodesPage tools={tools} />;
    case 'explore': return <ExplorePage />;
    case 'insights': return <InsightsPage tools={tools} />;
    case 'rules': return <RulesPage />;
    case 'skills': return <SkillsPage />;
    case 'memory': return <MemoryPage tools={tools} />;
    case 'sessions': return <SessionsPage tools={tools} />;
    case 'operations': return <OperationsPage tools={tools} />;
    case 'files': return <FilesPage />;
    case 'usage': return <UsagePage tools={tools} />;
    case 'logging': return <LoggingPage />;
    default: return <ToolsPage tools={tools} />;
  }
}
