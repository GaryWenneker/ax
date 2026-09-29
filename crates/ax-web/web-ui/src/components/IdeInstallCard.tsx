import { useCallback, useEffect, useState } from 'react';
import { fetchAgentStatus, type AgentTargetStatus } from '../agentApi';
import {
  IDE_STATE_LABEL,
  type LoadState,
  connectAllText,
  connectIdes,
  disconnectIdes,
  foundNotConnected,
  ideResultText,
  ideState,
  panelLabel,
} from '../ideInstall';

/** Settings → IDEs & agents: connect ax (MCP server and hooks) to every IDE on this machine. */
export default function IdeInstallCard() {
  const [targets, setTargets] = useState<AgentTargetStatus[]>([]);
  const [readonly, setReadonly] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [load, setLoad] = useState<LoadState>('loading');

  const refresh = useCallback(async () => {
    const data = await fetchAgentStatus();
    if (!data.ok) throw new Error('Could not load IDEs.');
    setTargets(data.targets ?? []);
    setReadonly(!!data.readonly);
    setLoad('ready');
  }, []);

  useEffect(() => {
    refresh().catch((e: unknown) => {
      setLoad('error');
      setError(e instanceof Error ? e.message : String(e));
    });
  }, [refresh]);

  async function run(key: string, verb: 'Connected' | 'Disconnected', ids: string[]) {
    setBusy(key);
    setError(null);
    setResult(null);
    try {
      const reports = await (verb === 'Connected' ? connectIdes(ids) : disconnectIdes(ids));
      setResult(ideResultText(verb, reports));
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  const found = foundNotConnected(targets);

  return (
    <section className="settings-card" aria-labelledby="ide-install-title">
      <div className="settings-card-header">
        <h2 id="ide-install-title">IDEs &amp; agents</h2>
        <p>Connect ax to the IDEs and coding agents on this machine. Connect writes the ax MCP server and hooks into their config.</p>
      </div>
      <div className="settings-card-body">
        <div className="settings-row">
          <div className="settings-row-label">
            <span className="settings-row-title">Connect all found</span>
            <span className="settings-row-desc">
              {connectAllText(load, found.length)}
            </span>
          </div>
          <div className="settings-row-control settings-row-control--actions">
            <button
              type="button"
              className="btn btn-subtle"
              disabled={readonly || !!busy || found.length === 0}
              onClick={() => void run('all', 'Connected', found)}
            >
              {busy === 'all' ? 'Connecting…' : 'Connect all found'}
            </button>
          </div>
        </div>
        {result && (
          <p className="settings-row-desc ide-install-result" role="status">
            {result}
          </p>
        )}
        {error && (
          <p className="settings-row-desc vault-folders-error" role="alert">
            {error}
          </p>
        )}
        <ul className="ide-install-list" aria-label="IDEs and agents">
          {targets.map((t) => {
            const state = ideState(t);
            const connected = state === 'connected';
            return (
              <li key={t.id} className="ide-install-row" data-id={t.id} data-state={state}>
                <div className="vault-folder-text">
                  <span className="settings-row-title">{t.display_name}</span>
                  {connected && t.config_paths.length > 0 && (
                    <span className="settings-row-desc mono" title={t.config_paths.join('\n')}>
                      {t.config_paths.join(', ')}
                    </span>
                  )}
                </div>
                <div className="vault-folder-actions">
                  <span className={`ide-install-badge ide-install-badge--${state}`}>{IDE_STATE_LABEL[state]}</span>
                  {panelLabel(t) && (
                    <span className={`ide-install-badge ide-install-badge--${t.panel ? 'connected' : 'missing'}`} title="ax Command Center inside this IDE">
                      {panelLabel(t)}
                    </span>
                  )}
                  <button
                    type="button"
                    className="btn btn-subtle"
                    aria-label={`${connected ? 'Disconnect' : 'Connect'} ${t.display_name}`}
                    disabled={readonly || !!busy}
                    onClick={() => void run(t.id, connected ? 'Disconnected' : 'Connected', [t.id])}
                  >
                    {busy === t.id ? 'Working…' : connected ? 'Disconnect' : 'Connect'}
                  </button>
                </div>
              </li>
            );
          })}
        </ul>
      </div>
    </section>
  );
}
