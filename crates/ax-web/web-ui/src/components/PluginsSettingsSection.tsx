import { useCallback, useEffect, useState } from 'react';

type PluginRow = {
  name: string;
  extensions: string[];
  mode: 'wasm' | 'process' | string;
  command?: string | null;
  wasm?: string | null;
};

/** Settings → Plugins: list extractors from GET /api/plugins. */
export default function PluginsSettingsSection() {
  const [plugins, setPlugins] = useState<PluginRow[]>([]);
  const [pluginsDir, setPluginsDir] = useState('.ax/plugins');
  const [err, setErr] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [tick, setTick] = useState(0);

  const load = useCallback(() => {
    setTick((t) => t + 1);
  }, []);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    fetch('/api/plugins')
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.json();
      })
      .then(
        (d: {
          plugins?: PluginRow[];
          pluginsDir?: string;
          count?: number;
        }) => {
          if (cancelled) return;
          const list = Array.isArray(d.plugins) ? d.plugins : [];
          setPlugins(list);
          if (d.pluginsDir) setPluginsDir(d.pluginsDir);
          setErr(null);
        },
      )
      .catch((e: unknown) => {
        if (!cancelled) {
          setErr(e instanceof Error ? e.message : 'Failed to load plugins');
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [tick]);

  return (
    <>
      <div className="settings-row">
        <div className="settings-row-label">
          <span className="settings-row-title">Extractor plugins</span>
          <span className="settings-row-desc">
            Process and WASM extractors under <code>{pluginsDir}</code>. Matching extensions run
            before the built-in tree-sitter pool during <code>ax index</code> /{' '}
            <code>ax sync</code>.
            {loading ? ' Loading…' : ` ${plugins.length} loaded.`}
          </span>
        </div>
        <div className="settings-row-control">
          <button type="button" className="btn btn-subtle" disabled={loading} onClick={load}>
            {loading ? 'Refreshing…' : 'Refresh'}
          </button>
        </div>
      </div>

      {err && (
        <p className="settings-inline-err" role="alert">
          {err}{' '}
          <button type="button" className="status-panel-link" onClick={load}>
            Retry
          </button>
        </p>
      )}

      {!loading && !err && plugins.length === 0 ? (
        <p className="settings-callout settings-plugins-empty">
          No plugins discovered. Add <code>.ax/plugins/&lt;name&gt;/plugin.toml</code> — see the{' '}
          <a href="https://getax.wenneker.io/guides/plugins/" target="_blank" rel="noreferrer">
            plugins guide
          </a>
          .
        </p>
      ) : null}

      {plugins.length > 0 && (
        <div className="page-table-wrap settings-plugins-table-wrap">
          <table className="page-table">
            <thead>
              <tr>
                <th>Name</th>
                <th>Mode</th>
                <th>Extensions</th>
                <th>Entry</th>
              </tr>
            </thead>
            <tbody>
              {plugins.map((p) => (
                <tr key={p.name}>
                  <td className="mono">{p.name}</td>
                  <td>
                    <span className={`settings-plugin-mode settings-plugin-mode--${p.mode}`}>
                      {p.mode}
                    </span>
                  </td>
                  <td className="mono">
                    {(p.extensions ?? []).length ? p.extensions.join(', ') : '—'}
                  </td>
                  <td
                    className="mono settings-plugin-entry"
                    title={p.command || p.wasm || ''}
                  >
                    {p.mode === 'wasm' ? p.wasm || '—' : p.command || '—'}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}
