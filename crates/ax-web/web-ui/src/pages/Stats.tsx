import { useEffect, useState } from 'react';
import { fetchStats } from '../api';
import { usePageContext } from '../context/UiContext';
import type { Stats } from '../types';

export default function StatsPage() {
  const [stats, setStats] = useState<Stats | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetchStats()
      .then(setStats)
      .catch((e: Error) => setError(e.message));
  }, []);

  usePageContext('Stats', stats ? `${stats.languages.length} languages` : undefined);

  if (error) {
    return (
      <div className="state-msg">
        <strong>Error loading stats</strong>
        {error}
      </div>
    );
  }

  if (!stats) {
    return <div className="loading-row">Loading stats…</div>;
  }

  const maxLang = Math.max(...stats.languages.map((l) => l.count), 1);

  return (
    <>
      <div className="page-header">
        <h1 className="page-title">Graph Stats</h1>
      </div>

      <p className="stats-summary">
        Index summary is shown in the status bar. Language breakdown below.
      </p>

      <div>
        <div className="detail-section-title" style={{ marginBottom: '8px' }}>
          Language breakdown
        </div>
        <table className="lang-table">
          <thead>
            <tr>
              <th>Language</th>
              <th>Nodes</th>
              <th style={{ width: '40%' }}>Distribution</th>
            </tr>
          </thead>
          <tbody>
            {stats.languages.map((l) => (
              <tr key={l.language}>
                <td style={{ fontFamily: 'var(--font-mono)', fontSize: 'var(--fs-sm)' }}>{l.language}</td>
                <td style={{ fontVariantNumeric: 'tabular-nums' }}>{l.count.toLocaleString()}</td>
                <td>
                  <div
                    className="lang-bar"
                    style={{ width: `${Math.round((l.count / maxLang) * 100)}%` }}
                  />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </>
  );
}
