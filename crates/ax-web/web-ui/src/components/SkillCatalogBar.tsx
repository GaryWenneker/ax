import { useEffect, useRef, useState } from 'react';
import Codicon from './Codicon';
import { agentBrief, messageFrom, runAudit, summarizeAudit } from '../lib/policyAudit';

type Hit = {
  id?: string;
  name: string;
  source: string;
  installs: number;
  install_url?: string | null;
  summary?: string | null;
  stars?: number | null;
  stars_label?: string | null;
  first_seen?: string | null;
  audits?: { name: string; status: string; url: string }[];
};

function catalogId(hit: Hit): string {
  const id = (hit.id || '').trim();
  if (id.split('/').filter(Boolean).length >= 3) return id;
  const source = (hit.source || '').replace(/\/+$/, '');
  if (!source || source === hit.name) return source || hit.name;
  return source.endsWith(`/${hit.name}`) ? source : `${source}/${hit.name}`;
}

function starLabel(hit: Hit): string | null {
  if (hit.stars_label) return hit.stars_label;
  if (typeof hit.stars !== 'number') return null;
  if (hit.stars >= 1_000_000) return compactCount(hit.stars, 1_000_000, 'M');
  if (hit.stars >= 1_000) return compactCount(hit.stars, 1_000, 'K');
  return String(hit.stars);
}

function compactCount(count: number, unit: number, suffix: string): string {
  const scaled = Math.round((count / unit) * 10) / 10;
  return Number.isInteger(scaled) ? `${scaled}${suffix}` : `${scaled.toFixed(1)}${suffix}`;
}

export default function SkillCatalogBar({ onInstalled }: { onInstalled: () => void }) {
  const [query, setQuery] = useState('');
  const [hits, setHits] = useState<Hit[]>([]);
  const [open, setOpen] = useState(false);
  const [panel, setPanel] = useState(false);
  const [error, setError] = useState('');
  const [note, setNote] = useState('');
  const [busy, setBusy] = useState(false);
  const [installed, setInstalled] = useState<Set<string>>(new Set());
  const [installingId, setInstallingId] = useState<string | null>(null);
  const [auditingId, setAuditingId] = useState<string | null>(null);
  const [auditNotes, setAuditNotes] = useState<Record<string, string>>({});
  const [auditBriefs, setAuditBriefs] = useState<Record<string, string>>({});
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const [reportInstalls, setReportInstalls] = useState(false);
  const [publishRemote, setPublishRemote] = useState('');

  async function loadInstalled() {
    const res = await fetch('/api/policy/skills/catalog/installed');
    if (!res.ok) return;
    const ids = (await res.json()) as unknown;
    if (!Array.isArray(ids)) return;
    setInstalled(new Set(ids.map((id) => String(id))));
  }

  useEffect(() => {
    void loadInstalled().catch(() => undefined);
    void fetch('/api/policy/skills/catalog/settings')
      .then((res) => res.json())
      .then((body: { reportInstalls?: boolean; publishRemote?: string }) => {
        setReportInstalls(Boolean(body.reportInstalls));
        setPublishRemote(body.publishRemote ?? '');
      })
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    function onPointer(event: PointerEvent) {
      if (!rootRef.current?.contains(event.target as Node)) {
        setOpen(false);
        setPanel(false);
      }
    }
    function onKey(event: KeyboardEvent) {
      if (event.key === 'Escape') {
        setOpen(false);
        setPanel(false);
      }
    }
    document.addEventListener('pointerdown', onPointer);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('pointerdown', onPointer);
      document.removeEventListener('keydown', onKey);
    };
  }, []);

  useEffect(() => {
    if (panel) inputRef.current?.focus();
  }, [panel]);

  async function saveSettings(nextReport: boolean, remote: string) {
    setReportInstalls(nextReport);
    setPublishRemote(remote);
    await fetch('/api/policy/skills/catalog/settings', {
      method: 'PUT',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ reportInstalls: nextReport, publishRemote: remote }),
    });
  }

  async function search() {
    setError('');
    setNote('');
    setHits([]);
    setOpen(true);
    setBusy(true);
    try {
      const res = await fetch(`/api/policy/skills/catalog/search?q=${encodeURIComponent(query)}`);
      const text = await res.text();
      if (!res.ok) throw new Error(messageFrom(text, 'Search failed'));
      const rows = JSON.parse(text) as Hit[];
      setHits(rows);
      setAuditNotes({});
      await loadInstalled().catch(() => undefined);
      setNote(rows.length === 0 ? 'No skills match that search.' : '');
      setOpen(true);
    } catch (err) {
      setHits([]);
      setError(err instanceof Error ? err.message : 'Search failed');
      setOpen(true);
    } finally {
      setBusy(false);
    }
  }

  async function auditHit(hit: Hit) {
    const id = catalogId(hit);
    if (!installed.has(id)) return;
    setError('');
    setAuditingId(id);
    try {
      const body = await runAudit('skills', hit.name);
      const { warns, fails, detail, securityLine } = summarizeAudit(body, hit.audits ?? []);
      const brief = agentBrief(body, hit.audits ?? []);
      setAuditNotes((current) => ({
        ...current,
        [id]: `Audit of ${id}: ${warns} warn, ${fails} fail.${detail}${securityLine}`,
      }));
      setAuditBriefs((current) => ({ ...current, [id]: brief }));
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Audit failed';
      setAuditNotes((current) => ({ ...current, [id]: `Audit of ${id} failed. ${message}` }));
    } finally {
      setAuditingId(null);
    }
  }

  async function install(hit: Hit) {
    const id = catalogId(hit);
    setError('');
    setNote('');
    setInstallingId(id);
    try {
      const source = hit.install_url || hit.source;
      const res = await fetch('/api/policy/skills/catalog/install', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ source, skill: hit.name, yes: false }),
      });
      const text = await res.text();
      if (!res.ok) throw new Error(messageFrom(text, 'Install failed'));
      setInstalled((current) => new Set(current).add(id));
      setNote(`Installed ${id}.`);
      onInstalled();
    } catch (err) {
      setError(`Could not install ${id}. ${err instanceof Error ? err.message : 'Install failed'}`);
    } finally {
      setInstallingId(null);
    }
  }

  return (
    <div className="skill-catalog" ref={rootRef}>
      <button
        type="button"
        className={`btn skill-catalog-launch${panel ? ' skill-catalog-launch--open' : ''}`}
        aria-expanded={panel}
        aria-label={panel ? 'Hide skill search' : 'Search more online'}
        title={panel ? 'Hide skill search' : 'Search more online'}
        onClick={() => {
          setPanel((current) => !current);
          if (panel) setOpen(false);
        }}
      >
        <Codicon name="search" />
        <span>Search more online</span>
      </button>
      {panel && (
        <div className="skill-catalog-slide" aria-label="Skill catalog">
          <div className="skill-catalog-slide-row">
          <label className={`skill-catalog-field${busy ? ' skill-catalog-field--busy' : ''}`}>
            <span className="sr-only">Search more online</span>
            <input
              ref={inputRef}
              className="settings-input"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') void search();
              }}
              placeholder="Search more online"
              aria-label="Search more online"
              aria-busy={busy}
            />
            {busy ? <span className="ax-spinner ax-spinner--sm skill-catalog-field-spinner" role="status" aria-label="Searching" /> : null}
          </label>
          <button type="button" className="btn btn-subtle" disabled={busy || query.trim().length < 2} onClick={() => void search()}>
            {busy ? 'Searching…' : 'Search'}
          </button>
          <label className="policy-storage-default" title="Report installs to skills.sh">
            <span className="sr-only">Report installs to skills.sh</span>
            <button
              type="button"
              role="switch"
              aria-checked={reportInstalls}
              aria-label="Report installs to skills.sh"
              className={`settings-toggle${reportInstalls ? ' on' : ''}`}
              onClick={() => void saveSettings(!reportInstalls, publishRemote)}
            >
              <span className="settings-toggle-thumb" />
            </button>
          </label>
          </div>
      {open && (
        <div className="skill-catalog-drop" role="region" aria-label="Skill search results">
          {busy && !error ? <p className="skill-catalog-note" role="status">Searching…</p> : null}
          {error && <p className="skill-catalog-note" role="alert">{error}</p>}
          {note && <p className="skill-catalog-note">{note}</p>}
          {hits.length > 0 && (
            <ul className="skill-catalog-hits">
              {hits.map((hit) => {
                const stars = starLabel(hit);
                const id = catalogId(hit);
                const already = installed.has(id);
                return (
                <li key={id} className="skill-catalog-hit">
                  <div className="skill-catalog-hit-head">
                    <span className="skill-catalog-hit-id">
                      <strong>{hit.name}</strong>
                      <span className="muted">{hit.source || hit.id}</span>
                    </span>
                    <span className="skill-catalog-actions">
                    <button
                      type="button"
                      className="btn btn-subtle"
                      disabled={!already || auditingId === id}
                      title={already ? 'Audit the installed skill' : 'Install the skill before auditing it'}
                      onClick={() => void auditHit(hit)}
                    >
                      {auditingId === id ? 'Auditing…' : 'Audit'}
                    </button>
                    <button type="button" className="btn btn-subtle" disabled={installingId === id || already} onClick={() => void install(hit)}>
                      {already ? 'Installed' : installingId === id ? 'Installing…' : 'Install'}
                    </button>
                    </span>
                  </div>
                  {hit.summary ? <span className="skill-catalog-summary">{hit.summary}</span> : null}
                  <span className="skill-catalog-meta">
                      <span className="muted">{hit.installs.toLocaleString()} installs</span>
                      {stars ? (
                        <span className="skill-catalog-stars">
                          <span className="skill-catalog-star" aria-hidden="true">★</span>
                          <span>{stars}</span>
                          <span className="sr-only">GitHub stars</span>
                        </span>
                      ) : null}
                      {hit.first_seen ? <span className="muted">First seen {hit.first_seen}</span> : null}
                    </span>
                    {hit.audits && hit.audits.length > 0 ? (
                      <span className="skill-catalog-audits">
                        {hit.audits.map((audit) => (
                          <a
                            key={audit.url}
                            className={`skill-audit skill-audit-${audit.status}`}
                            href={audit.url}
                            target="_blank"
                            rel="noreferrer"
                          >
                            <span>{audit.name}</span>
                            <span className="skill-audit-status">{audit.status}</span>
                          </a>
                        ))}
                      </span>
                    ) : null}
                    {auditNotes[id] ? (
                      <span className="skill-catalog-row-foot">
                        <span className="skill-catalog-row-note" role="status">{auditNotes[id]}</span>
                        {auditBriefs[id] ? (
                          <button
                            type="button"
                            className="btn btn-subtle skill-catalog-copy"
                            onClick={() => {
                              void navigator.clipboard.writeText(auditBriefs[id]).then(() => setCopiedId(id));
                            }}
                          >
                            {copiedId === id ? 'Copied' : 'Copy for agent'}
                          </button>
                        ) : null}
                      </span>
                    ) : null}
                </li>
                );
              })}
            </ul>
          )}
        </div>
      )}
        </div>
      )}
    </div>
  );
}
