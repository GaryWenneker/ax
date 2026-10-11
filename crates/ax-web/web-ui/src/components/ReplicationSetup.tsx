import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react';
import { accessJson } from '../lib/accessApi';
import ModalShell from './ModalShell';
import Codicon from './Codicon';

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
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [restart, setRestart] = useState(false);
  const [visible, setVisible] = useState(false);
  const [inputVisible, setInputVisible] = useState(false);
  const [discoveryError, setDiscoveryError] = useState<string | null>(null);
  const [phase, setPhase] = useState<'idle' | 'discovering' | 'connecting'>('idle');
  const credentialInput = useRef<HTMLInputElement>(null);
  const mounted = useRef(true);
  const inFlight = useRef(false);
  const requestVersion = useRef(0);
  const load = useCallback(async () => {
    const version = ++requestVersion.current;
    const next = await accessJson<State>('/api/access/nodes/local');
    if (!mounted.current || version !== requestVersion.current) return;
    setState(next);
    if (next.settings) {
      const saved = next.settings;
      setRole(saved.config.role); setCluster(saved.config.cluster_id);
      setProject(saved.config.projects[0]?.id ?? ''); setPort(String(saved.port));
      setPrimary(saved.config.primary_url ?? ''); setAdvertise(saved.advertise ?? '');
    }
  }, []);
  useEffect(() => {
    mounted.current = true;
    void load().catch(e => mounted.current && setError(String(e.message ?? e)));
    const interval = window.setInterval(() => { if (!inFlight.current) void load().catch(() => {}); }, 5000);
    return () => { mounted.current = false; ++requestVersion.current; window.clearInterval(interval); };
  }, [load]);

  useEffect(() => {
    if (role !== 'replica' || state?.settings || locked) return;
    let cancelled = false;
    let discovering = false;
    const controller = new AbortController();
    async function discover() {
      if (discovering) return;
      discovering = true;
      setPhase('discovering'); setDiscoveryError(null);
      try {
        const result = await accessJson<{ candidates: Candidate[] }>('/api/access/nodes/local/discover', { method: 'POST', signal: controller.signal, body: JSON.stringify({ cluster_id: '' }) });
        if (!cancelled) setCandidates(result.candidates.filter(c => c.role === 'primary'));
      } catch (e) { if (!cancelled) setDiscoveryError(e instanceof Error ? e.message : String(e)); }
      finally { discovering = false; if (!cancelled) setPhase('idle'); }
    }
    const first = window.setTimeout(() => { void discover(); }, 350);
    const interval = window.setInterval(() => { void discover(); }, 15000);
    return () => { cancelled = true; controller.abort(); window.clearTimeout(first); window.clearInterval(interval); };
  }, [role, state?.settings, locked]);

  async function action(operation: () => Promise<void>) {
    if (locked || inFlight.current) return;
    inFlight.current = true; ++requestVersion.current; setBusy(true); setError(null); setMessage(null);
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
      setIssued(result.credential?.token ?? (token || null)); setToken(''); setVisible(false); setRestart(false);
      setMessage(role === 'primary' ? 'Primary is running.' : 'Replica connected. Existing data is preserved and knowledge is exchanged. Policies require review.');
      await load(); await onChanged();
    });
  }
  const disabled = locked || busy || !state || state.running;
  const known = candidates.filter(candidate => candidate.role === 'primary' && (!cluster.trim() || candidate.cluster_id === cluster.trim()));
  const paired = !!state?.running && state.active_project && state.settings?.config.role === 'replica' && !state?.safety?.sync_failed;
  const saved = state?.settings;
  async function credential() {
    if (issued) return issued;
    const result = await accessJson<{ token: string }>('/api/access/nodes/local/credential', { method: 'POST' });
    if (!result.token) throw new Error('Replication credential is unavailable.');
    setIssued(result.token); return result.token;
  }
  const credentialControl = <label>Replication credential<div className="replication-secret-control">
    <input ref={credentialInput} className="settings-input" type={inputVisible ? 'text' : 'password'} autoComplete="off" minLength={32} maxLength={4096} required={role === 'replica' && !saved} value={token} disabled={disabled} onChange={e => setToken(e.target.value)} />
    <button type="button" className="btn" aria-label={inputVisible ? 'Hide entered replication credential' : 'Show entered replication credential'} aria-pressed={inputVisible} disabled={disabled} onClick={() => setInputVisible(!inputVisible)}><Codicon name={inputVisible ? 'eye-closed' : 'eye'} /></button>
    <button type="button" className="btn" aria-label="Copy entered replication credential" disabled={disabled || !token} onClick={() => void action(async () => { await navigator.clipboard.writeText(token); setMessage('Replication credential copied.'); })}><Codicon name="copy" /></button>
  </div></label>;
  return <div className="access-client replication-setup">
    <div className="access-client-header"><div><h3>{saved ? 'Replication configuration' : 'Configure this Ax environment'}</h3><p>{saved ? 'Manage the Primary or Replica configured for this project.' : 'Make it a Primary or connect it to a Primary as a Replica.'}</p></div></div>
    {error && <p role="alert" className="settings-toast settings-toast--err">{error}</p>}
    {message && <p role="status" className="settings-toast settings-toast--ok">{message}</p>}
    {state?.running && <div className="access-note"><strong>Managed node is running{state.active_project ? '' : ' for another project'}.</strong> Remove the local configuration to set up a different role or Primary. <button type="button" className="btn" disabled={locked || busy} onClick={() => setConfirmStop(true)}>Stop managed node</button></div>}
    {state && !state.running && state.settings && <p className="access-note">Saved configuration is stopped. Restart it with the securely stored credential. Ax also starts it automatically when web or the MCP server starts.</p>}
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
    {saved ? <div aria-label="Saved replication configuration">
      <dl className="replication-config-details">
        <div><dt>Role</dt><dd>{saved.config.role === 'primary' ? 'Primary' : 'Replica'}</dd></div>
        <div><dt>Node ID</dt><dd><code>{saved.config.node_id}</code></dd></div>
        <div><dt>Cluster ID</dt><dd><code>{saved.config.cluster_id}</code></dd></div>
        <div><dt>Replication project ID</dt><dd><code>{saved.config.projects[0]?.id}</code></dd></div>
        {saved.config.primary_url && <div><dt>Primary URL</dt><dd><code>{saved.config.primary_url}</code></dd></div>}
        <div><dt>Local service port</dt><dd>{saved.port}</dd></div>
        {saved.advertise && <div><dt>Advertised URL</dt><dd><code>{saved.advertise}</code></dd></div>}
        <div><dt>Status</dt><dd>{state?.running && state.active_project ? 'Running' : 'Stopped'}</dd></div>
      </dl>
      <label>Replication credential<div className="replication-secret-control">
        <input className="settings-input" readOnly type={visible ? 'text' : 'password'} value={visible ? issued ?? '' : 'x'.repeat(24)} />
        <button type="button" className="btn" aria-label={visible ? 'Hide replication credential' : 'Show replication credential'} aria-pressed={visible} disabled={locked || busy} onClick={() => { if (visible) setVisible(false); else void action(async () => { await credential(); setVisible(true); }); }}><Codicon name={visible ? 'eye-closed' : 'eye'} /></button>
        <button type="button" className="btn" aria-label="Copy replication credential" disabled={locked || busy} onClick={() => void action(async () => { await navigator.clipboard.writeText(await credential()); setMessage('Replication credential copied.'); })}><Codicon name="copy" /></button>
      </div></label>
      <p className="access-note">The credential is hidden by default and securely stored for automatic startup. Copy it together with the cluster and project IDs to connect a Replica.</p>
      <div className="access-actions">
        {!state?.running && <button type="button" className="btn" disabled={locked || busy} onClick={() => setRestart(true)}>{role === 'primary' ? 'Restart Primary' : 'Reconnect Replica'}</button>}
        <button type="button" className="btn danger" disabled={locked || busy} onClick={() => setConfirmRemove(true)}>Remove local configuration</button>
      </div>
    </div> : <form className="access-form" onSubmit={activate} aria-label="Replication setup">
      <label>Local node role<select className="settings-select" aria-label="Local node role" value={role} disabled={disabled} onChange={e => { setRole(e.target.value as 'primary' | 'replica'); setToken(''); setIssued(null); setMessage(null); }}><option value="primary">Primary — publish this project</option><option value="replica">Replica — connect to a Primary</option></select></label>
      <label>Cluster ID<input className="settings-input" required={role === 'replica'} pattern="[A-Za-z0-9_-]+" maxLength={128} placeholder={role === 'primary' ? 'Generated automatically when empty' : 'Primary cluster ID'} value={cluster} disabled={disabled} onChange={e => setCluster(e.target.value)} /></label>
      <label>Replication project ID<input className="settings-input" required={role === 'replica'} pattern="[A-Za-z0-9_-]+" maxLength={128} placeholder={role === 'primary' ? 'Generated automatically when empty' : 'Primary project ID'} value={project} disabled={disabled} onChange={e => setProject(e.target.value)} /></label>
      <p className="access-note">Only the project currently open in Command Center is enrolled. Use the same cluster and project IDs on both nodes.</p>
      {role === 'replica' && <>
        {discoveryError && <p role="status" className="access-note">{discoveryError}</p>}
        {phase === 'discovering' && <div className="replication-discovery-progress" role="status"><span aria-hidden="true" />Discovering available services on your LAN…</div>}
        {!!known.length && <div className="replication-candidates" role="group" aria-label="Available Primaries">
          {known.map(c => <button key={`${c.node_id}-${c.advertised_url}`} type="button" className={`replication-candidate${primary === c.advertised_url ? ' is-selected' : ''}`} aria-pressed={primary === c.advertised_url} disabled={disabled} onClick={() => { setPrimary(c.advertised_url); setCluster(c.cluster_id); setMessage('Primary selected. Enter its project credential, then connect.'); credentialInput.current?.focus(); }}>
            <span className="replication-node-mark" aria-hidden="true" /><span><strong>{c.node_id}</strong><code>{c.advertised_url}</code><small>Cluster {c.cluster_id} · discovered candidate</small></span><span className="replication-candidate-state">{primary === c.advertised_url ? 'Selected' : 'Select Primary'}</span>
          </button>)}
        </div>}
        <label>Primary URL<input className="settings-input" type="url" required placeholder="https://primary.example.com" value={primary} disabled={disabled} onChange={e => setPrimary(e.target.value)} /></label>
        <p className="access-note">HTTPS is required except for localhost. Discovery works on the same LAN; the URL field also supports hosted environments. Selecting a candidate does not connect it automatically.</p>
      </>}
      <label>Local service port<input className="settings-input" type="number" min="1024" max="65535" required value={port} disabled={disabled} onChange={e => setPort(e.target.value)} /></label>
      <label>Advertised URL (optional)<input className="settings-input" type="url" placeholder="https://this-node.example.com" value={advertise} disabled={disabled} onChange={e => setAdvertise(e.target.value)} /></label>
      <p className="access-note">The service listens on loopback. For other machines, configure a TLS reverse proxy to this port and enter its URL for access from other machines. Services are announced automatically. Command Center stays private.</p>
      {credentialControl}
      <p className="access-note">{role === 'primary' && !state?.settings ? 'Leave IDs and credential empty to generate them on Start Primary.' : 'Enter the Primary’s project credential. Client access tokens are separate.'} The credential is stored in your OS credential store for automatic startup.</p>
      <button type="submit" className="btn primary" disabled={disabled}>{phase === 'connecting' ? 'Connecting…' : role === 'primary' ? 'Start Primary' : 'Connect to Primary'}</button>
    </form>}
    {restart && <ModalShell title={role === 'primary' ? 'Restart Primary' : 'Reconnect Replica'} onClose={() => !busy && setRestart(false)}><p>Restart with the securely stored credential. Leave the field empty to reuse it, or enter a replacement.</p><form className="access-form" onSubmit={activate}>{credentialControl}<div className="access-actions"><button type="button" className="btn" disabled={busy} onClick={() => setRestart(false)}>Cancel</button><button type="submit" className="btn primary" disabled={disabled}>{role === 'primary' ? 'Start Primary' : 'Connect to Primary'}</button></div></form></ModalShell>}
    {confirmRemove && <ModalShell title="Remove local replication configuration?" onClose={() => !busy && setConfirmRemove(false)}><p>Stops this project's managed service and removes its saved configuration and credential. Existing memories, rules, skills and other servers remain. The setup form will return.</p>{error && <p role="alert">{error}</p>}<div className="access-actions"><button type="button" className="btn" disabled={busy} onClick={() => setConfirmRemove(false)}>Cancel</button><button type="button" className="btn danger" disabled={locked || busy} onClick={() => void action(async () => { await accessJson('/api/access/nodes/local/remove', { method: 'POST' }); if (!mounted.current) return; setConfirmRemove(false); setCluster(''); setProject(''); setPrimary(''); setAdvertise(''); setPort('9070'); setRole('primary'); setCandidates([]); setToken(''); setIssued(null); setVisible(false); setMessage('Local replication configuration removed. Existing project data is preserved.'); await load(); await onChanged(); })}>Remove configuration</button></div></ModalShell>}
    {confirmStop && <ModalShell title="Stop managed replication?" onClose={() => !busy && setConfirmStop(false)}><p>Stops this project’s managed service. Stored configuration and staged snapshots remain. Other Ax services are unaffected.</p><div className="access-actions"><button type="button" className="btn" disabled={busy} onClick={() => setConfirmStop(false)}>Cancel</button><button type="button" className="btn danger" disabled={locked || busy} onClick={() => void action(async () => { await accessJson('/api/access/nodes/local/stop', { method: 'POST' }); if (!mounted.current) return; setConfirmStop(false); setMessage('Managed node stopped. Its configuration is preserved for automatic startup.'); await load(); await onChanged(); })}>Stop replication</button></div></ModalShell>}
  </div>;
}
