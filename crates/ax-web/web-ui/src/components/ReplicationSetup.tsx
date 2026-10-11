import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react';
import { accessJson } from '../lib/accessApi';
import ModalShell from './ModalShell';

type Settings = { config: { node_id: string; role: 'primary' | 'replica'; cluster_id: string; primary_url?: string | null; projects: { id: string }[] }; port: number; advertise?: string | null };
type Candidate = { node_id: string; role: string; advertised_url: string; cluster_id: string };
type State = { settings: Settings | null; running: boolean; active_project: boolean; safety?: { mode: string; memories: number; rules: number; skills: number; review_required?: boolean; sync_failed?: boolean } };

export default function ReplicationSetup({ locked, onChanged }: { locked: boolean; onChanged: () => Promise<void> }) {
  const [state, setState] = useState<State | null>(null);
  const [role, setRole] = useState<'primary' | 'replica'>('primary');
  const [cluster, setCluster] = useState('');
  const [project, setProject] = useState('');
  const [port, setPort] = useState('9070');
  const [advertise, setAdvertise] = useState('');
  const [primary, setPrimary] = useState('');
  const [token, setToken] = useState('');
  const [candidates, setCandidates] = useState<Candidate[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [issued, setIssued] = useState<string | null>(null);
  const [confirmStop, setConfirmStop] = useState(false);
  const [phase, setPhase] = useState<'idle' | 'discovering' | 'connecting'>('idle');
  const credentialInput = useRef<HTMLInputElement>(null);
  const mounted = useRef(true);
  const inFlight = useRef(false);
  const load = useCallback(async (populate = false) => {
    const next = await accessJson<State>('/api/access/nodes/local');
    if (!mounted.current) return;
    setState(next);
    if (populate && next.settings) {
      const saved = next.settings;
      setRole(saved.config.role); setCluster(saved.config.cluster_id);
      setProject(saved.config.projects[0]?.id ?? ''); setPort(String(saved.port));
      setPrimary(saved.config.primary_url ?? ''); setAdvertise(saved.advertise ?? '');
    }
  }, []);
  useEffect(() => {
    mounted.current = true;
    void load(true).catch(e => mounted.current && setError(String(e.message ?? e)));
    const interval = window.setInterval(() => { if (!inFlight.current) void load().catch(() => {}); }, 5000);
    return () => { mounted.current = false; window.clearInterval(interval); };
  }, [load]);

  async function action(operation: () => Promise<void>) {
    if (locked || inFlight.current) return;
    inFlight.current = true; setBusy(true); setError(null); setMessage(null); setIssued(null);
    try { await operation(); }
    catch (e) { if (mounted.current) setError(e instanceof Error ? e.message : String(e)); }
    finally { inFlight.current = false; if (mounted.current) { setBusy(false); setPhase('idle'); } }
  }
  async function activate(event: FormEvent) {
    event.preventDefault();
    await action(async () => {
      setPhase('connecting');
      const result = await accessJson<{ credential?: { token: string } | null }>('/api/access/nodes/local', {
        method: 'POST', body: JSON.stringify({ role, cluster_id: cluster.trim(), project_id: project.trim(),
          primary_url: role === 'replica' ? primary.trim() : null,
          token: token || null, port: Number(port), advertise: advertise.trim() || null }),
      });
      if (!mounted.current) return;
      setToken(''); setIssued(result.credential?.token ?? null);
      setMessage(role === 'primary' ? 'Primary is running.' : 'Replica connected. Existing data is preserved and knowledge is exchanged. Policies require review.');
      await load(); await onChanged();
    });
  }
  const disabled = locked || busy || !state || state.running;
  const known = candidates.filter(candidate => candidate.role === 'primary');
  const paired = !!state?.running && state.active_project && state.settings?.config.role === 'replica';
  return <div className="access-client replication-setup">
    <div className="access-client-header"><div><h3>Configure this Ax environment</h3><p>Make it a Primary or connect it to a Primary as a Replica.</p></div></div>
    {error && <p role="alert" className="settings-toast settings-toast--err">{error}</p>}
    {message && <p role="status" className="settings-toast settings-toast--ok">{message}</p>}
    {state?.running && <div className="access-note"><strong>Managed node is running{state.active_project ? '' : ' for another project'}.</strong> Stop it before changing roles or choosing another Primary. <button type="button" className="btn" disabled={locked || busy} onClick={() => setConfirmStop(true)}>Stop managed node</button></div>}
    {state && !state.running && state.settings && <p className="access-note">Saved configuration is stopped. Enter the replication credential to restart it. Node identity is preserved.</p>}
    <div className={`replication-connection${phase === 'connecting' ? ' is-connecting' : ''}${paired ? ' is-connected' : ''}`} aria-label="Replication connection">
      <div><span className="replication-node-mark" aria-hidden="true" /><strong>This Ax</strong><small>{role === 'primary' ? 'Primary' : 'Replica'}</small></div>
      <span className="replication-connection-line" aria-hidden="true" />
      <div><span className="replication-node-mark" aria-hidden="true" /><strong>{role === 'primary' ? 'Authorized replicas' : primary || 'Choose a Primary'}</strong><small>{phase === 'connecting' ? 'Authenticating and verifying…' : paired ? 'Exchanging knowledge · local data preserved' : role === 'primary' ? 'Project enrollment required' : 'Explicit pairing required'}</small></div>
    </div>
    <div className="replication-data-safety" role="note" aria-label="Existing data protection">
      <strong>Existing data is preserved</strong>
      <p>An empty Primary cannot clear this Ax environment. Pairing contributes your existing knowledge and receives the cluster’s knowledge under separate IDs. Missing records never trigger deletion; conflicting local edits stay intact. Only pair with a Primary you trust with all this project’s knowledge, including private records.</p>
      {state?.safety && <p><strong>{state.safety.memories}</strong> memories · <strong>{state.safety.rules}</strong> rules · <strong>{state.safety.skills}</strong> skills currently on this node</p>}
      <small>Accepted source revisions are retained on the Primary. Received rules and skills require review before activation.</small>
      {state?.safety?.review_required && <p role="status">Conflicting local edits were preserved. Review the received versions before resolving them.</p>}
      {state?.safety?.sync_failed && <p role="alert">Synchronization needs attention. Existing local data remains intact.</p>}
      {state?.running && <p><a href="/policy/review">Review received rules and skills</a></p>}
    </div>
    <form className="access-form" onSubmit={activate}>
      <label>Local node role<select className="settings-select" aria-label="Local node role" value={role} disabled={disabled} onChange={e => { setRole(e.target.value as 'primary' | 'replica'); setToken(''); setIssued(null); setMessage(null); }}><option value="primary">Primary — publish this project</option><option value="replica">Replica — connect to a Primary</option></select></label>
      <label>Cluster ID<input className="settings-input" required pattern="[A-Za-z0-9_-]+" maxLength={128} value={cluster} disabled={disabled} onChange={e => { setCluster(e.target.value); setCandidates([]); }} /></label>
      <label>Replication project ID<input className="settings-input" required pattern="[A-Za-z0-9_-]+" maxLength={128} value={project} disabled={disabled} onChange={e => setProject(e.target.value)} /></label>
      <p className="access-note">Only the project currently open in Command Center is enrolled. Use the same cluster and project IDs on both nodes.</p>
      {role === 'replica' && <>
        <div className="access-actions"><button type="button" className="btn" disabled={disabled || !cluster.trim()} onClick={() => void action(async () => {
          setPhase('discovering');
          const result = await accessJson<{ candidates: Candidate[] }>('/api/access/nodes/local/discover', { method: 'POST', body: JSON.stringify({ cluster_id: cluster.trim() }) });
          if (!mounted.current) return;
          setCandidates(result.candidates); setMessage(result.candidates.some(c => c.role === 'primary') ? 'Choose a Primary below. Discovery does not establish trust.' : 'No nearby Primaries found. Enter a Primary URL directly.');
        })}>{busy ? 'Working…' : 'Find nearby Primaries'}</button></div>
        {phase === 'discovering' && <div className="replication-discovery-progress" role="status"><span aria-hidden="true" />Searching this cluster on your LAN…</div>}
        {!!known.length && <div className="replication-candidates" role="group" aria-label="Available Primaries">
          {known.map(c => <button key={`${c.node_id}-${c.advertised_url}`} type="button" className={`replication-candidate${primary === c.advertised_url ? ' is-selected' : ''}`} aria-pressed={primary === c.advertised_url} disabled={disabled} onClick={() => { setPrimary(c.advertised_url); setMessage('Primary selected. Enter its project credential, then connect.'); credentialInput.current?.focus(); }}>
            <span className="replication-node-mark" aria-hidden="true" /><span><strong>{c.node_id}</strong><code>{c.advertised_url}</code><small>Cluster {c.cluster_id} · discovered candidate</small></span><span className="replication-candidate-state">{primary === c.advertised_url ? 'Selected' : 'Select Primary'}</span>
          </button>)}
        </div>}
        <label>Primary URL<input className="settings-input" type="url" required placeholder="https://primary.example.com" value={primary} disabled={disabled} onChange={e => setPrimary(e.target.value)} /></label>
        <p className="access-note">HTTPS is required except for localhost. Discovery works on the same LAN; the URL field also supports hosted environments. Selecting a candidate does not connect it automatically.</p>
      </>}
      <label>Local service port<input className="settings-input" type="number" min="1024" max="65535" required value={port} disabled={disabled} onChange={e => setPort(e.target.value)} /></label>
      <label>Advertised URL (optional)<input className="settings-input" type="url" placeholder="https://this-node.example.com" value={advertise} disabled={disabled} onChange={e => setAdvertise(e.target.value)} /></label>
      <p className="access-note">The service listens on loopback. For other machines, configure a TLS reverse proxy to this port and enter its URL to enable LAN announcements. Command Center stays private.</p>
      <label>Replication credential<input ref={credentialInput} className="settings-input" type="password" autoComplete="off" minLength={32} maxLength={4096} required={role === 'replica' || !!state?.settings} value={token} disabled={disabled} onChange={e => setToken(e.target.value)} /></label>
      <p className="access-note">{role === 'primary' && !state?.settings ? 'Leave empty to generate a project credential, shown once after startup.' : 'Enter the Primary’s project credential. Client access tokens are separate.'} Credentials stay in process memory and must be entered again after restart.</p>
      <button type="submit" className="btn primary" disabled={disabled}>{phase === 'connecting' ? 'Connecting…' : role === 'primary' ? 'Start Primary' : 'Connect to Primary'}</button>
    </form>
    {issued && <div className="access-token" role="status"><strong>New replication credential — copy it now</strong><p>Use it on your Replica with this cluster and project ID. Ax does not save it to disk.</p><code>{issued}</code><div className="access-actions"><button type="button" className="btn" onClick={async () => { try { await navigator.clipboard.writeText(issued); setMessage('Replication credential copied.'); } catch { setError('Copy failed. Select the credential and copy it manually.'); } }}>Copy replication credential</button><button type="button" className="btn" onClick={() => setIssued(null)}>Dismiss credential</button></div></div>}
    {confirmStop && <ModalShell title="Stop managed replication?" onClose={() => !busy && setConfirmStop(false)}><p>Stops the node started by this Command Center. Stored configuration and staged snapshots remain. Other Ax services are unaffected.</p><div className="access-actions"><button type="button" className="btn" disabled={busy} onClick={() => setConfirmStop(false)}>Cancel</button><button type="button" className="btn danger" disabled={locked || busy} onClick={() => void action(async () => { await accessJson('/api/access/nodes/local/stop', { method: 'POST' }); if (!mounted.current) return; setConfirmStop(false); setMessage('Managed node stopped. You can now choose another Primary.'); await load(); await onChanged(); })}>Stop replication</button></div></ModalShell>}
  </div>;
}
