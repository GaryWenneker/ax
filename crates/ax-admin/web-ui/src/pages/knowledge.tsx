import { useState } from 'react';
import ModalShell from '@cc/components/ModalShell';
import { PageCard, PageCardBody } from '@cc/components/ui/PageLayout';
import type { Tool } from '../api';
import { AutoCard, Frame, QueryCard, SchemaForm } from '../components/cards';

const find = (tools: readonly Tool[], name: string) => tools.find((tool) => tool.name === name);

export function RulesPage() {
  return (
    <Frame title="Rules" about="The policy rules this environment gives to agents.">
      <AutoCard title="Rules" description="All rules, with their level and scope." tool="ax_rules" />
      <QueryCard title="Match a prompt" description="Which rules would apply to this request?" tool="ax_rules" argument="prompt" placeholder="A request an agent might receive" idle="Enter a prompt to see the matching rules" />
    </Frame>
  );
}

export function SkillsPage() {
  return (
    <Frame title="Skills" about="Named workflows the environment can hand to an agent.">
      <QueryCard title="Open a skill" description="Loads the skill's full text by name." tool="ax_skill" argument="name" placeholder="Skill name, for example startup" idle="Enter a skill name" />
    </Frame>
  );
}

/** A card with one button that opens a tool's form in a dialog. */
function FormCard({ tool, title, description, button, submit }: { tool: Tool; title: string; description: string; button: string; submit: string }) {
  const [open, setOpen] = useState(false);
  return (
    <PageCard title={title} description={description}>
      <PageCardBody>
        <button type="button" className="btn" onClick={() => setOpen(true)}>{button}</button>
      </PageCardBody>
      {open && (
        <ModalShell title={title} subtitle={tool.description} onClose={() => setOpen(false)} size="lg">
          <SchemaForm tool={tool} submit={submit} />
        </ModalShell>
      )}
    </PageCard>
  );
}

export function MemoryPage({ tools }: { tools: readonly Tool[] }) {
  const remember = find(tools, 'ax_remember');
  return (
    <Frame title="Memory" about="Decisions, fixes and conventions stored in this environment's memory vault.">
      {find(tools, 'ax_recall') && <QueryCard title="Recall" description="Search memories by text." tool="ax_recall" argument="query" placeholder="What was decided about…" idle="Search the memory vault" />}
      {find(tools, 'ax_history') && <QueryCard title="History" description="Dated turns and commits that touched a file, symbol or topic." tool="ax_history" argument="query" placeholder="File path, symbol or topic" idle="Look up when something changed" />}
      {remember && <FormCard tool={remember} title="Add memory" description="Stores a memory in the environment. Shown because this connection has write access." button="Add memory" submit="Store memory" />}
    </Frame>
  );
}

const WORKING_CONTEXT = { action: 'get' };

export function SessionsPage({ tools }: { tools: readonly Tool[] }) {
  const durable = find(tools, 'ax_durable');
  return (
    <Frame title="Sessions" about="Working context and durable transcripts kept by the environment for this connection.">
      {find(tools, 'ax_session') && <AutoCard title="Working context" description="Objective, facts, decisions and open questions of this connection's chat." tool="ax_session" args={WORKING_CONTEXT} />}
      {durable && <FormCard tool={durable} title="Durable store" description="Transcript, documents and tasks kept for this connection." button="Run a durable action" submit="Run" />}
    </Frame>
  );
}
