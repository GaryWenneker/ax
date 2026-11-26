import { useState } from 'react';
import { FilterBar, PageCard } from '@cc/components/ui/PageLayout';
import type { Tool } from '../api';
import { AutoCard, Frame, QueryCard, TabbedCard } from '../components/cards';
import Tabs from '../components/Tabs';
import ToolResultView from '../components/ToolResult';
import { useTool } from '../useTool';

const has = (tools: readonly Tool[], name: string) => tools.some((tool) => tool.name === name);

export function StatusPage({ tools }: { tools: readonly Tool[] }) {
  return (
    <Frame title="Status" about="What this environment has indexed, and how fresh it is.">
      {has(tools, 'ax_status') && <AutoCard title="Index" description="Files, symbols and the last index run." tool="ax_status" />}
      {has(tools, 'ax_cache_status') && <AutoCard title="Caches" description="Context cache and file-token cache counts." tool="ax_cache_status" />}
    </Frame>
  );
}

export function SearchPage() {
  return (
    <Frame title="Search" about="Full-text search across the indexed symbols of this environment.">
      <QueryCard title="Symbol search" description="Functions, classes, methods and other symbols." tool="ax_search" argument="query" placeholder="Search symbols…" idle="Type a name and run the search" />
    </Frame>
  );
}

const NODE_VIEWS = [
  { tool: 'ax_node', argument: 'name', label: 'Source' },
  { tool: 'ax_callers', argument: 'symbol', label: 'Callers' },
  { tool: 'ax_callees', argument: 'symbol', label: 'Callees' },
  { tool: 'ax_impact', argument: 'symbol', label: 'Impact' },
] as const;

export function NodesPage({ tools }: { tools: readonly Tool[] }) {
  const views = NODE_VIEWS.filter((view) => has(tools, view.tool));
  const { state, run } = useTool();
  const [symbol, setSymbol] = useState('');
  const [chosen, setChosen] = useState<string | null>(null);
  const active = views.find((view) => view.tool === chosen) ?? views[0];
  if (!active) return null;
  const open = (tool: string) => {
    const view = views.find((v) => v.tool === tool);
    setChosen(tool);
    if (view && symbol.trim()) void run(view.tool, { [view.argument]: symbol.trim() });
  };
  return (
    <Frame title="Nodes" about="Open one symbol: its source, who calls it, what it calls and what a change would touch.">
      <PageCard title="Symbol" description="Enter a symbol name, then choose what to look at.">
        <form onSubmit={(e) => { e.preventDefault(); open(active.tool); }} role="search">
          <FilterBar>
            <input className="settings-input settings-input--grow" type="search" aria-label="Symbol name" placeholder="Symbol name, for example parse_config" value={symbol} onChange={(e) => setSymbol(e.target.value)} />
            <button type="submit" className="btn" disabled={state.status === 'loading' || !symbol.trim()}>Open</button>
          </FilterBar>
        </form>
        <Tabs name="node" label="View" tabs={views.map((view) => ({ id: view.tool, label: view.label }))} active={active.tool} onChange={open}>
          <ToolResultView state={state} idle="Enter a symbol name" />
        </Tabs>
      </PageCard>
    </Frame>
  );
}

export function ExplorePage() {
  return (
    <Frame title="Explore" about="Ask how something works. The answer is the relevant source with its call path.">
      <QueryCard title="Question or symbols" description="Semantic search plus graph traversal." tool="ax_explore" argument="query" placeholder="How is the configuration loaded?" idle="Ask a question about the code" />
    </Frame>
  );
}

const INSIGHT_TABS = [
  { tool: 'ax_insights', label: 'Insights' },
  { tool: 'ax_report', label: 'Architecture report' },
] as const;

export function InsightsPage({ tools }: { tools: readonly Tool[] }) {
  return (
    <Frame title="Insights" about="Subsystems, the most connected symbols and surprising links across the whole graph.">
      <TabbedCard name="insights" title="Whole-graph view" description="Computed by the environment from its current index." tabs={INSIGHT_TABS.filter((tab) => has(tools, tab.tool))} />
    </Frame>
  );
}
