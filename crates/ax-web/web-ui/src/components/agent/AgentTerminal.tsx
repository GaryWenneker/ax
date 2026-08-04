import { useCallback, useEffect, useRef, useState } from 'react';

import {
  fetchAgentStatus,
  isCliReady,
  saveAgentConfig,
  setActiveProfile,
  streamAgentChat,
  terminalAgentOptions,
  TERMINAL_EXTERNAL_AGENTS,
  type AgentTargetStatus,
  type AgentsConfig,
  type AgentStreamEvent,
  type ProfileEntry,
} from '../../agentApi';
import { AX_LOG_ICON } from '../../lib/mcpTrace';
import AgentMessageBody from './AgentMessageBody';
import AgentPtyTerminal from './AgentPtyTerminal';
import Codicon from '../Codicon';
import WorkspacePicker from '../WorkspacePicker';

type ChatLine =
  | { kind: 'user'; text: string }
  | { kind: 'assistant'; text: string }
  | { kind: 'system'; text: string }
  | { kind: 'tool'; text: string };

interface Props {
  maximized: boolean;
  onToggleMaximize: () => void;
}

function resolveInitialAgent(cfg: AgentsConfig): string {
  if (cfg.last_terminal_agent) return cfg.last_terminal_agent;
  if (cfg.terminal_mode === 'builtin') return 'builtin';
  return cfg.preferred_external ?? 'cursor';
}

function profileForAgent(cfg: AgentsConfig | null, agent: string): string {
  if (!cfg) return 'default';
  return cfg.active_profile?.[agent] ?? cfg.profiles?.[agent]?.[0]?.id ?? 'default';
}

export default function AgentTerminal({ maximized, onToggleMaximize }: Props) {
  const [config, setConfig] = useState<AgentsConfig | null>(null);
  const [catalog, setCatalog] = useState<AgentTargetStatus[]>([]);
  const [agent, setAgent] = useState('builtin');
  const [profileId, setProfileId] = useState('default');
  const [lines, setLines] = useState<ChatLine[]>([]);
  const [input, setInput] = useState('');
  const [busy, setBusy] = useState(false);
  const [sessionId, setSessionId] = useState<string | undefined>();
  const abortRef = useRef<AbortController | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const assistantBuf = useRef('');
  const userPickedAgentRef = useRef(false);

  const refreshConfig = useCallback(async () => {
    const data = await fetchAgentStatus();
    if (data.config) setConfig(data.config);
    setCatalog(data.catalog ?? data.targets ?? []);
    return data;
  }, []);

  const agentOptions = terminalAgentOptions(
    catalog.length > 0
      ? catalog
      : TERMINAL_EXTERNAL_AGENTS.map((a) => ({
          id: a.id,
          display_name: a.label,
          detected: false,
          cli_on_path: false,
          cli_installable: true,
          configured: false,
          config_paths: [],
          runnable: true,
        })),
  );

  const pushSystem = useCallback((text: string) => {
    setLines((l) => [...l, { kind: 'system', text }]);
  }, []);

  const labelFor = useCallback(
    (id: string) =>
      catalog.find((t) => t.id === id)?.display_name ??
      TERMINAL_EXTERNAL_AGENTS.find((a) => a.id === id)?.label ??
      id,
    [catalog],
  );

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const data = await refreshConfig();
      if (cancelled || !data.config) return;
      if (userPickedAgentRef.current) return;

      const initialAgent = resolveInitialAgent(data.config);
      const pid = profileForAgent(data.config, initialAgent);
      setAgent(initialAgent);
      setProfileId(pid);

      if (initialAgent === 'builtin') return;

      const target = (data.catalog ?? data.targets ?? []).find((t) => t.id === initialAgent);
      const label = labelFor(initialAgent);
      if (target && !isCliReady(target) && target.cli_installable !== false) {
        pushSystem(
          `${label} CLI not installed — install from Settings → AI Agents, then select ${label} again.`,
        );
      } else {
        pushSystem(`Interactive ${label} CLI · profile ${pid}`);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [refreshConfig, pushSystem, labelFor]);

  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [lines, busy]);

  const profiles = config?.profiles?.[agent] ?? [];

  async function persistAgentChoice(nextAgent: string, cfg: AgentsConfig) {
    const next: AgentsConfig = {
      ...cfg,
      last_terminal_agent: nextAgent,
      preferred_external: nextAgent !== 'builtin' ? nextAgent : cfg.preferred_external,
    };
    setConfig(next);
    const res = await saveAgentConfig(next);
    if (!res.ok) pushSystem(res.error ?? 'Could not save agent preference');
  }

  async function onAgentChange(nextAgent: string) {
    if (nextAgent === agent) return;
    userPickedAgentRef.current = true;
    const pid = profileForAgent(config, nextAgent);
    setAgent(nextAgent);
    setProfileId(pid);
    if (config) void persistAgentChoice(nextAgent, config);
    if (nextAgent === 'builtin') {
      pushSystem(`[ax] ${AX_LOG_ICON} Built-in ax chat mode`);
    } else {
      pushSystem(`Interactive ${labelFor(nextAgent)} CLI · profile ${pid}`);
    }
  }

  async function onSend() {
    const prompt = input.trim();
    if (!prompt || busy) return;

    setInput('');
    setLines((l) => [...l, { kind: 'user', text: prompt }]);
    setBusy(true);
    assistantBuf.current = '';
    setLines((l) => [...l, { kind: 'assistant', text: '' }]);

    abortRef.current?.abort();
    const ac = new AbortController();
    abortRef.current = ac;

    await streamAgentChat(
      prompt,
      { sessionId, agent, profileId },
      (ev: AgentStreamEvent) => {
        if (ev.type === 'system') {
          setLines((l) => [...l, { kind: 'system', text: ev.text }]);
        }
        if (ev.type === 'line') {
          setLines((l) => [...l, { kind: 'system', text: ev.text }]);
        }
        if (ev.type === 'tool_start') {
          const prefix = ev.name.startsWith('ax_') ? `${AX_LOG_ICON} ` : '';
          setLines((l) => [...l, { kind: 'tool', text: `${prefix}▶ ${ev.name}` }]);
        }
        if (ev.type === 'tool_end') {
          const prefix = ev.name.startsWith('ax_') ? `${AX_LOG_ICON} ` : '';
          const preview = ev.preview?.trim();
          const text = preview ? `${prefix}✓ ${ev.name}\n${preview}` : `${prefix}✓ ${ev.name}`;
          setLines((l) => [...l, { kind: 'tool', text }]);
        }
        if (ev.type === 'token') {
          assistantBuf.current += ev.text;
          const text = assistantBuf.current;
          setLines((l) => {
            const copy = [...l];
            for (let i = copy.length - 1; i >= 0; i--) {
              if (copy[i].kind === 'assistant') {
                copy[i] = { kind: 'assistant', text };
                break;
              }
            }
            return copy;
          });
        }
        if (ev.type === 'error') {
          setLines((l) => [...l, { kind: 'system', text: ev.message }]);
        }
        if (ev.type === 'done' && ev.session_id) setSessionId(ev.session_id);
      },
      ac.signal,
    );
    setBusy(false);
  }

  async function onProfileChange(id: string) {
    setProfileId(id);
    await setActiveProfile(agent, id);
    pushSystem(`Switched profile to ${id} (${agent})`);
    void refreshConfig();
  }

  function onKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === 'Escape' && maximized) {
      onToggleMaximize();
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      void onSend();
    }
  }

  const isExternal = agent !== 'builtin';

  return (
    <div className={`agent-terminal${maximized ? ' agent-terminal--maximized' : ''}`}>
      <div className="agent-terminal-toolbar">
        <WorkspacePicker compact />
        <label className="agent-terminal-select-wrap">
          <span className="sr-only">Agent</span>
          <select
            className="settings-input agent-terminal-select"
            value={agent}
            onChange={(e) => void onAgentChange(e.target.value)}
          >
            {agentOptions.map((o) => (
              <option key={o.id} value={o.id}>{o.label}</option>
            ))}
          </select>
        </label>
        <label className="agent-terminal-select-wrap">
          <span className="sr-only">Profile</span>
          <select
            className="settings-input agent-terminal-select"
            value={profileId}
            onChange={(e) => void onProfileChange(e.target.value)}
          >
            {profiles.length === 0 && <option value="default">default</option>}
            {profiles.map((p: ProfileEntry) => (
              <option key={p.id} value={p.id}>{p.label}</option>
            ))}
          </select>
        </label>
        {isExternal && (
          <span className="agent-terminal-status">{labelFor(agent)} · Interactive CLI</span>
        )}
        <button type="button" className="btn btn-subtle agent-terminal-max" onClick={onToggleMaximize} title={maximized ? 'Exit fullscreen' : 'Maximize'}>
          <Codicon name={maximized ? 'screen-normal' : 'screen-full'} />
        </button>
      </div>

      {isExternal ? (
        <AgentPtyTerminal key={`${agent}:${profileId}`} agent={agent} profileId={profileId} />
      ) : (
        <>
          <div className="agent-terminal-messages" ref={scrollRef}>
            {lines.length === 0 && (
              <div className="agent-terminal-empty">
                Ask the built-in ax agent about your codebase, run quality checks, or explore symbols.
                <div className="agent-terminal-chips">
                  {['Explore ship pipeline', 'What changed vs main?', 'Run evaluate'].map((c) => (
                    <button key={c} type="button" className="agent-terminal-chip" onClick={() => setInput(c)}>
                      {c}
                    </button>
                  ))}
                </div>
              </div>
            )}
            {lines.map((line, i) => (
              <div key={i} className={`agent-msg agent-msg--${line.kind}`}>
                <AgentMessageBody
                  text={line.text || (line.kind === 'assistant' && busy ? '…' : '')}
                  kind={line.kind}
                />
              </div>
            ))}
          </div>

          <div className="agent-terminal-composer">
            <textarea
              className="agent-terminal-input"
              rows={maximized ? 3 : 2}
              placeholder="Message the agent… (Ctrl+Enter to send)"
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={onKeyDown}
              disabled={busy}
            />
            <button type="button" className="btn primary" onClick={() => void onSend()} disabled={busy || !input.trim()}>
              {busy ? '…' : 'Send'}
            </button>
          </div>
        </>
      )}
    </div>
  );
}
