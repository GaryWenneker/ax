import { useEffect, useMemo, useState, type FormEvent, type ReactNode } from 'react';
import { FilterBar, PageCard, PageHero, PageShell, PageStack } from '@cc/components/ui/PageLayout';
import Codicon from '@cc/components/Codicon';
import type { Tool } from '../api';
import { useActiveConnection } from '../activeConnection';
import { argumentsFromValues, fieldsFromSchema, type FormValues } from '../schemaForm';
import { useTool } from '../useTool';
import ConfirmDialog from './ConfirmDialog';
import Tabs from './Tabs';
import ToolResultView from './ToolResult';

export function Frame({ title, about, children }: { title: string; about: string; children: ReactNode }) {
  return (
    <PageShell>
      <PageHero title={title} subtitle={about} />
      <PageStack>{children}</PageStack>
    </PageShell>
  );
}

/** One text box, one tool. */
export function QueryCard({ title, description, tool, argument, placeholder, idle }: {
  title: string;
  description: string;
  tool: string;
  argument: string;
  placeholder: string;
  idle: string;
}) {
  const { state, run } = useTool();
  const [value, setValue] = useState('');
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (value.trim()) void run(tool, { [argument]: value.trim() });
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
          <button type="submit" className="btn" disabled={state.status === 'loading' || !value.trim()}>Run</button>
        </FilterBar>
      </form>
      <ToolResultView state={state} idle={idle} />
    </PageCard>
  );
}

/** A tool without input that runs when the page opens. `args` must be a constant. */
export function AutoCard({ title, description, tool, args = NO_ARGS }: {
  title: string;
  description: string;
  tool: string;
  args?: Record<string, unknown>;
}) {
  const { state, run } = useTool();
  useEffect(() => {
    void run(tool, args);
  }, [run, tool, args]);
  return (
    <PageCard
      title={title}
      description={description}
      footer={
        <button type="button" className="btn btn-secondary" disabled={state.status === 'loading'} onClick={() => void run(tool, args)}>
          <Codicon name="refresh" /> Refresh
        </button>
      }
    >
      <ToolResultView state={state} />
    </PageCard>
  );
}

const NO_ARGS: Record<string, unknown> = {};

/** Several tools without input, one shown at a time. */
export function TabbedCard({ name, title, description, tabs }: {
  name: string;
  title: string;
  description: string;
  tabs: readonly { tool: string; label: string }[];
}) {
  const { state, run } = useTool();
  const [chosen, setChosen] = useState<string | null>(null);
  const active = tabs.find((tab) => tab.tool === chosen)?.tool ?? tabs[0]?.tool ?? null;
  useEffect(() => {
    if (active !== null) void run(active);
  }, [run, active]);
  if (active === null) return null;
  return (
    <PageCard title={title} description={description}>
      <Tabs name={name} label={title} tabs={tabs.map((tab) => ({ id: tab.tool, label: tab.label }))} active={active} onChange={setChosen}>
        <ToolResultView state={state} />
      </Tabs>
    </PageCard>
  );
}

/**
 * A form generated from a tool's own input schema. Give it `key={tool.name}`
 * so another tool starts with an empty form. With `confirm`, running asks first.
 */
export function SchemaForm({ tool, submit = 'Run', confirm }: { tool: Tool; submit?: string; confirm?: string }) {
  const connection = useActiveConnection();
  const fields = useMemo(() => fieldsFromSchema(tool.inputSchema), [tool.inputSchema]);
  const [values, setValues] = useState<FormValues>({});
  const [problem, setProblem] = useState<string | null>(null);
  const [asking, setAsking] = useState<Record<string, unknown> | null>(null);
  const { state, run } = useTool();
  const onSubmit = (event: FormEvent) => {
    event.preventDefault();
    try {
      const args = argumentsFromValues(fields, values);
      setProblem(null);
      if (confirm) setAsking(args);
      else void run(tool.name, args);
    } catch (e) {
      setProblem(e instanceof Error ? e.message : 'Check the form');
    }
  };
  return (
    <>
      <form className="admin-form" onSubmit={onSubmit} aria-label={tool.name}>
        {problem && <div className="settings-toast settings-toast--err" role="alert">{problem}</div>}
        {fields.length === 0 && <p className="admin-note">This tool takes no input.</p>}
        {fields.map((field) => {
          const id = `field-${tool.name}-${field.name}`;
          const label = <>{field.name}{field.required ? ' *' : ''}</>;
          const value = values[field.name];
          if (field.kind === 'boolean') {
            return (
              <label key={field.name} className="admin-choice" title={field.description}>
                <input type="checkbox" checked={value === true} onChange={(e) => setValues({ ...values, [field.name]: e.target.checked })} />
                {label}
              </label>
            );
          }
          const text = typeof value === 'string' ? value : '';
          const change = (next: string) => setValues({ ...values, [field.name]: next });
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
          <button type="submit" className="btn" disabled={state.status === 'loading'}>{submit}</button>
        </div>
      </form>
      <ToolResultView state={state} />
      {asking && confirm && (
        <ConfirmDialog
          title={`Run ${tool.name} on ${connection.name}?`}
          message={confirm}
          confirmLabel={`Run on ${connection.name}`}
          onCancel={() => setAsking(null)}
          onConfirm={() => {
            const args = asking;
            setAsking(null);
            void run(tool.name, args);
          }}
        />
      )}
    </>
  );
}
