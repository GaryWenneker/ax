import { useMemo, useState } from 'react';
import { PageCard, PageEmpty } from '@cc/components/ui/PageLayout';
import type { Tool } from '../api';
import Badge from '../components/Badge';
import { Frame, SchemaForm } from '../components/cards';
import OptionList from '../components/OptionList';
import { CHANGING_TOOLS } from './management';

export function ToolsPage({ tools }: { tools: readonly Tool[] }) {
  const sorted = useMemo(() => [...tools].sort((a, b) => a.name.localeCompare(b.name)), [tools]);
  const [selected, setSelected] = useState<string | null>(null);
  const tool = sorted.find((t) => t.name === selected) ?? sorted[0];
  const confirm = tool ? CHANGING_TOOLS[tool.name] : undefined;
  return (
    <Frame title="Tools" about="Every tool the environment offers to this connection, including tools this application has no page for.">
      <PageCard title={`${sorted.length} tools`} description="Choose a tool to see its input and run it.">
        {!tool ? (
          <PageEmpty title="The environment offers no tools to this connection" />
        ) : (
          <div className="admin-split">
            <OptionList label="Tools" items={sorted} keyOf={(t) => t.name} selected={tool.name} onSelect={setSelected}>
              {(t) => (
                <>
                  <span className="admin-row-main">
                    <span className="admin-row-title">{t.name}</span>
                    <span className="admin-row-sub">{t.description}</span>
                  </span>
                  {!t.readOnly && <span className="admin-row-badges"><Badge icon="warning" label="changes data" tone="warn" /></span>}
                </>
              )}
            </OptionList>
            <section aria-label={tool.name}>
              <header className="admin-tool-head">
                <h3>{tool.name}</h3>
                {tool.description && <p>{tool.description}</p>}
              </header>
              {/* The key gives every tool its own empty form and result. */}
              <SchemaForm key={tool.name} tool={tool} {...(confirm ? { confirm } : {})} />
            </section>
          </div>
        )}
      </PageCard>
    </Frame>
  );
}
