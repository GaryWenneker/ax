import { useEffect, useState } from 'react';

import AgentTerminal from '../components/agent/AgentTerminal';
import { PageHero, PageShell } from '../components/ui/PageLayout';
import { usePageContext } from '../context/UiContext';

export default function AgentPage() {
  const [maximized, setMaximized] = useState(() => localStorage.getItem('ax-agent-max') === '1');

  usePageContext('Agent', maximized ? 'Fullscreen' : 'Chat');

  useEffect(() => {
    localStorage.setItem('ax-agent-max', maximized ? '1' : '0');
    document.body.classList.toggle('agent-maximized-active', maximized);
    return () => document.body.classList.remove('agent-maximized-active');
  }, [maximized]);

  return (
    <PageShell className="agent-page">
      {!maximized && (
        <PageHero
          title="Agent"
          subtitle="Chat with your AI agent — switch projects, profiles, and accounts from the toolbar."
        />
      )}
      <AgentTerminal maximized={maximized} onToggleMaximize={() => setMaximized((m) => !m)} />
    </PageShell>
  );
}
