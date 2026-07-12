import { useEffect, useState } from 'react';
import { fetchStats } from '../api';
import {
  DataTable,
  DistBar,
  PageCard,
  PageCardBody,
  PageEmpty,
  PageHero,
  PageLoading,
  PageShell,
  PageStack,
  PageToasts,
  StatusPanel,
  StatusPill,
} from '../components/ui/PageLayout';
import { usePageContext } from '../context/UiContext';
import { saveString } from '../lib/uiStorage';
import type { Stats } from '../types';

function browseLanguage(language: string) {
  saveString('nodes-lang', language);
  saveString('nodes-q', '');
  saveString('nodes-kind', '');
  saveString('nodes-offset', '0');
  window.location.hash = 'nodes';
}

function formatBytes(b: number) {
  if (b < 1024) return `${b} B`;
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB`;
  return `${(b / (1024 * 1024)).toFixed(1)} MB`;
}

function formatDate(ts: number) {
  return new Date(ts).toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

export default function StatsPage() {
  const [stats, setStats] = useState<Stats | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetchStats()
      .then(setStats)
      .catch((e: Error) => setError(e.message));
  }, []);

  usePageContext('Stats', stats ? `${stats.languages.length} languages` : undefined);

  if (!stats && !error) {
    return (
      <PageShell>
        <PageHero title="Graph Stats" subtitle="Index overview for the current project." />
        <PageLoading label="Loading stats…" />
      </PageShell>
    );
  }

  const maxLang = Math.max(...(stats?.languages.map((l) => l.count) ?? [1]), 1);

  return (
    <PageShell>
      <PageHero
        title="Graph Stats"
        subtitle={
          stats ? (
            <>
              <strong>{stats.project_name}</strong>
              {stats.readonly ? ' · read-only' : ''} · last indexed {formatDate(stats.last_indexed_at)}
            </>
          ) : (
            'Index overview for the current project.'
          )
        }
      />

      <PageToasts err={error} />

      {stats && (
        <PageStack>
          <PageCard title="Index summary" description="Counts from the ax graph database.">
            <StatusPanel title="Metrics">
              <StatusPill label="Nodes" value={stats.node_count.toLocaleString()} tone="ok" />
              <StatusPill label="Edges" value={stats.edge_count.toLocaleString()} />
              <StatusPill label="Files" value={stats.file_count.toLocaleString()} />
              <StatusPill label="DB size" value={formatBytes(stats.db_size_bytes)} />
              <StatusPill label="Rules" value={String(stats.policy_rules_count)} />
              <StatusPill label="Skills" value={String(stats.policy_skills_count)} />
              {stats.unresolved_ref_count != null && (
                <StatusPill
                  label="Unresolved refs"
                  value={stats.unresolved_ref_count.toLocaleString()}
                  tone={stats.unresolved_ref_count > 0 ? 'warn' : 'neutral'}
                />
              )}
            </StatusPanel>
          </PageCard>

          <PageCard
            title="Language breakdown"
            description={`${stats.languages.length} languages detected in the index.`}
          >
            <PageCardBody>
              {stats.languages.length === 0 ? (
                <PageEmpty title="No languages indexed">Run ax index to populate the graph.</PageEmpty>
              ) : (
                <DataTable>
                  <thead>
                    <tr>
                      <th>Language</th>
                      <th>Nodes</th>
                      <th style={{ width: '40%' }}>Distribution</th>
                    </tr>
                  </thead>
                  <tbody>
                    {stats.languages.map((l) => (
                      <tr
                        key={l.language}
                        className="stats-lang-row"
                        title={`Browse ${l.language} symbols`}
                        onClick={() => browseLanguage(l.language)}
                      >
                        <td className="mono">{l.language}</td>
                        <td className="num">{l.count.toLocaleString()}</td>
                        <td>
                          <DistBar pct={Math.round((l.count / maxLang) * 100)} />
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </DataTable>
              )}
            </PageCardBody>
          </PageCard>
        </PageStack>
      )}
    </PageShell>
  );
}
