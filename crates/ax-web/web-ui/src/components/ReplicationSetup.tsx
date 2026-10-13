import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react';
import { accessJson } from '../lib/accessApi';
import { memoryRowAge } from '../memoryHeadline';
import { useNow } from '../lib/useNow';
import ModalShell from './ModalShell';
import Codicon from './Codicon';
import ServiceStatus, { type ServiceState } from './ServiceStatus';
import SettingsSelect from './ui/SettingsSelect';

type Settings = { config: { node_id: string; role: 'primary' | 'replica'; cluster_id: string; primary_url?: string | null; projects: { id: string }[] }; port: number; advertise?: string | null };
type Candidate = { node_id: string; role: string; advertised_url: string; cluster_id: string; project_ids?: string[] };
type Interface = { name: string; address: string; loopback: boolean };
type Dataset = { origin: string; nodes: number; edges: number; files: number; sourceBytes: number; logFiles: number; logBytes: number; agentSessions: number; historyRows: number };
type State = { active_replicas?: number; runtime_error?: string; datasets?: Dataset[]; sync?: { compatibility_warning?: string | null; status?: string; last_error?: string; last_success_unix_seconds?: number }; settings: Settings | null; running: boolean; active_project: boolean; safety?: { mode: string; memories: number; rules: number; skills: number; review_required?: boolean; sync_failed?: boolean } };

export default function ReplicationSetup({ locked, onChanged }: { locked: boolean; onChanged: () => Promise<void> }) {
  const now = useNow();
  const [state, setState] = useState<State | null>(null);
  const [role, setRole] = useState<'primary' | 'replica'>('primary');
  const [cluster, setCluster] = useState('');
  const [project, setProject] = useState('');
  const [port, setPort] = useState('0');
  const [interfaces, setInterfaces] = useState<Interface[]>([]);
  const [ip, setIp] = useState('');
  const [setupOpen, setSetupOpen] = useState(false);
  const editing = useRef(false);
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
    if (editing.current) return;
    if (next.settings) {
      const saved = next.settings;
      setRole(saved.config.role); setCluster(saved.config.cluster_id);
      setProject(saved.config.projects[0]?.id ?? ''); setPort(String(saved.port));
      setPrimary(saved.config.primary_url ?? '');
      const endpoint = saved.advertise ? new URL(saved.advertise) : null;
      if (endpoint?.protocol === 'http:') { setIp(endpoint.hostname.replace(/^\[|\]$/g, '')); setAdvertise(''); } else setAdvertise(saved.advertise ?? '');
    }
  }, []);
  useEffect(() => {
    mounted.current = true;
    void accessJson<{ interfaces: Interface[] }>('/api/access/nodes/local/interfaces').then(result => { if (mounted.current) { setInterfaces(result.interfaces); setIp(old => old || result.interfaces.find(i => !i.loopback)?.address || result.interfaces[0]?.address || ''); } }).catch(e => mounted.current && setError(e instanceof Error ? e.message : String(e)));
    void load().catch(e => mounted.current && setError(String(e.message ?? e)));
    const interval = window.setInterval(() => { if (!inFlight.current) void load().catch(() => {}); }, 5000);
    return () => { mounted.current = false; ++requestVersion.current; window.clearInterval(interval); };
  }, [load]);
  useEffect(() => { editing.current = setupOpen || restart; }, [setupOpen, restart]);

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
          token: token || null, port: Number(port), advertise: advertise.trim() || (ip ? `http://${ip.includes(':') ? `[${ip}]` : ip}:${Number(port)}` : null) }),
      });
      if (!mounted.current) return;
      setIssued(result.credential?.token ?? (token || null)); setToken(''); setVisible(false); setRestart(false); setSetupOpen(false); editing.current = false;
      setMessage(role === 'primary' ? 'Primary is running.' : 'Replica connected. Graph, source, logs and memory are synchronized. Received policies require review.');
      await load(); await onChanged();
    });
  }
  const disabled = locked || busy || !state || state.running;
  const known = candidates.filter(candidate => candidate.role === 'primary' && (!cluster.trim() || candidate.cluster_id === cluster.trim()));
  const complete = state?.sync?.status?.includes('full_data_synced') ?? false;
  const peers = state?.active_replicas ?? 0;
  const paired = !!state?.running && state.active_project && !state.safety?.sync_failed && (state.settings?.config.role === 'primary' ? peers > 0 : complete);
  const serviceState: ServiceState = error || state?.runtime_error || state?.safety?.sync_failed ? 'error' : !state?.running ? 'stopped' : (state.safety?.review_required || state.sync?.compatibility_warning) ? 'warning' : 'running';
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
    <div className="service-toolbar"><ServiceStatus state={serviceState} label={serviceState === 'error' ? 'Replication needs attention' : state?.running ? 'Replication service running' : saved ? 'Replication stopped' : 'Replication not configured'} active={paired} />
      {state?.running && <button type="button" className="btn" disabled={locked || busy} onClick={() => setConfirmStop(true)}>Stop managed node</button>}
    </div>
    {state?.runtime_error && <p role="alert">{state.runtime_error}</p>}
    {saved && <>
      <div className={`replication-connection${phase === 'connecting' ? ' is-connecting' : ''}${paired ? ' is-connected' : ''}`} aria-label="Replication connection">
        <div><span className="replication-node-mark" aria-hidden="true" /><strong>This Ax</strong><small>{saved.config.role === 'primary' ? 'Primary' : 'Replica'}</small></div>
        <span className="replication-connection-line" aria-hidden="true" />
        <div><span className="replication-node-mark" aria-hidden="true" /><strong>{saved.config.role === 'primary' ? `${peers} active replicas` : saved.config.primary_url}</strong><small>{paired ? 'Graph · memory · logs' : state?.running ? 'Waiting for completed synchronization' : 'Service stopped'}</small></div>
      </div>
      <div className="access-node-local"><h3>Local {saved.config.role === 'primary' ? 'Primary' : 'Replica'}</h3><p>{saved.config.node_id}</p>
        <p>Sync state: {state?.safety?.sync_failed ? 'Failed' : complete ? 'All datasets synchronized' : 'Waiting for synchronization'}</p>
        <p>Last successful sync: {state?.sync?.last_success_unix_seconds ? memoryRowAge(state.sync.last_success_unix_seconds * 1000, now) : 'Never'}</p>
        {state?.sync?.compatibility_warning && <p role="status" className="access-note">{state.sync.compatibility_warning}</p>}
        {state?.sync?.last_error && <p role="alert">{state.sync.last_error}</p>}
        {state?.datasets?.map(dataset => <p key={dataset.origin}><strong>Node {dataset.origin}</strong>: {dataset.nodes} nodes · {dataset.edges} relationships · {dataset.files} files · {dataset.logFiles} log files · {dataset.agentSessions} agent sessions · {dataset.historyRows} conversation and log records</p>)}
      </div>
    </>}
    <div className="replication-data-safety" role="note" aria-label="Existing data protection">
      <strong>Existing data is preserved</strong>
      <p>An empty Primary cannot clear this Ax environment. Pairing exchanges memory, rules, skills, the full graph, indexed source, agent history and MCP logs under separate node identities. Missing records never trigger deletion; conflicting local edits stay intact. Only pair with a Primary you trust with all this project’s knowledge, including private records.</p>
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
      <p className="access-note">The credential is hidden by default and securely stored for automatic startup. Copy this key and the advertised Primary URL to connect a Replica. Cluster and project IDs are obtained automatically.</p>
      <div className="access-actions">
        {!state?.running && <button type="button" className="btn" disabled={locked || busy} onClick={() => setRestart(true)}>{role === 'primary' ? 'Restart Primary' : 'Reconnect Replica'}</button>}
        <button type="button" className="btn danger" disabled={locked || busy} onClick={() => setConfirmRemove(true)}>Remove local configuration</button>
      </div>
    </div> : <button type="button" className="btn primary" disabled={locked || busy} onClick={() => { editing.current = true; setSetupOpen(true); }}>Set up replication</button>}
    {setupOpen && <ModalShell title="Set up replication" onClose={() => { if (!busy) { setSetupOpen(false); editing.current = false; } }}>
      {error && !setupOpen && <p role="alert">{error}</p>}
<form className="access-form" onSubmit={activate} aria-label="Replication setup">
      <label>Local node role<SettingsSelect className="settings-select" aria-label="Local node role" value={role} disabled={disabled} onChange={e => { setRole(e.target.value as 'primary' | 'replica'); setToken(''); setIssued(null); setMessage(null); }}><option value="primary">Primary — publish this project</option><option value="replica">Replica — connect to a Primary</option></SettingsSelect></label>
      <details><summary>Advanced options</summary><p className="access-note">IDs are generated for a Primary and returned by authenticated enrollment for a Replica. Override a project ID only when the Primary serves multiple projects.</p>
      <label>Cluster ID<input className="settings-input" pattern="[A-Za-z0-9_-]+" maxLength={128} placeholder={role === 'primary' ? 'Generated automatically when empty' : 'Primary cluster ID'} value={cluster} disabled={disabled} onChange={e => setCluster(e.target.value)} /></label>
      <label>Replication project ID<input className="settings-input" pattern="[A-Za-z0-9_-]+" maxLength={128} placeholder={role === 'primary' ? 'Generated automatically when empty' : 'Primary project ID'} value={project} disabled={disabled} onChange={e => setProject(e.target.value)} /></label>
      <label>Local service port<input className="settings-input" type="number" min="0" max="65535" value={port} disabled={disabled} onChange={e => setPort(e.target.value)} /></label>
      <p className="access-note">0 allocates a local port automatically. The Primary's port is already part of its URL.</p>
      <label>Public proxy URL (optional)<input className="settings-input" type="url" placeholder="https://this-node.example.com" value={advertise} disabled={disabled} onChange={e => setAdvertise(e.target.value)} /></label>
      </details>
      {role === 'replica' && <>
        {discoveryError && <p role="status" className="access-note">{discoveryError}</p>}
        {phase === 'discovering' && <div className="replication-discovery-progress" role="status"><span aria-hidden="true" />Discovering available services on your LAN…</div>}
        {!!known.length && <div className="replication-candidates" role="group" aria-label="Available Primaries">
          {known.map(c => <button key={`${c.node_id}-${c.advertised_url}`} type="button" className={`replication-candidate${primary === c.advertised_url ? ' is-selected' : ''}`} aria-pressed={primary === c.advertised_url} disabled={disabled} onClick={() => { setPrimary(c.advertised_url); setCluster(c.cluster_id); setProject(c.project_ids?.length === 1 ? c.project_ids[0] : ''); setMessage('Primary selected. Enter its project credential, then connect.'); credentialInput.current?.focus(); }}>
            <span className="replication-node-mark" aria-hidden="true" /><span><strong>{c.node_id}</strong><code>{c.advertised_url}</code><small>Cluster {c.cluster_id} · discovered candidate</small></span><span className="replication-candidate-state">{primary === c.advertised_url ? 'Selected' : 'Select Primary'}</span>
          </button>)}
        </div>}
        <label>Primary URL<input className="settings-input" type="url" required placeholder="https://primary.example.com" value={primary} disabled={disabled} onChange={e => { setPrimary(e.target.value); setCluster(''); setProject(''); }} /></label>
        <p className="access-note">HTTPS or an explicit private LAN IP is supported. Discovery works on the same LAN; the URL field also supports hosted environments. Selecting a candidate does not connect it automatically.</p>
      </>}
      <label>Advertised IP address<SettingsSelect className="settings-select" aria-label="Advertised IP address" value={ip} disabled={disabled} onChange={e => setIp(e.target.value)}>{interfaces.map(i => <option key={i.address} value={i.address}>{i.address} — {i.name}{i.loopback ? ' (this machine only)' : ''}</option>)}</SettingsSelect></label>
      <p className="access-note">This address is used by the service and its LAN broadcast. Choose the address other machines can reach.</p>
      {credentialControl}
      <p className="access-note">{role === 'primary' && !state?.settings ? 'Leave IDs and credential empty to generate them on Start Primary.' : 'Enter the Primary’s project credential. Client access tokens are separate.'} The credential is stored in your OS credential store for automatic startup.</p>
      <button type="submit" className="btn primary" disabled={disabled}>{phase === 'connecting' ? 'Connecting…' : role === 'primary' ? 'Start Primary' : 'Connect to Primary'}</button>
    </form>
    </ModalShell>}
    {restart && <ModalShell title={role === 'primary' ? 'Restart Primary' : 'Reconnect Replica'} onClose={() => !busy && setRestart(false)}><p>Restart with the securely stored credential. Leave the field empty to reuse it, or enter a replacement.</p><form className="access-form" onSubmit={activate}>{credentialControl}<div className="access-actions"><button type="button" className="btn" disabled={busy} onClick={() => setRestart(false)}>Cancel</button><button type="submit" className="btn primary" disabled={disabled}>{role === 'primary' ? 'Start Primary' : 'Connect to Primary'}</button></div></form></ModalShell>}
    {confirmRemove && <ModalShell title="Remove local replication configuration?" onClose={() => !busy && setConfirmRemove(false)}><p>Stops this project's managed service and removes its saved configuration and credential. Existing memories, rules, skills and other servers remain. The setup form will return.</p>{error && <p role="alert">{error}</p>}<div className="access-actions"><button type="button" className="btn" disabled={busy} onClick={() => setConfirmRemove(false)}>Cancel</button><button type="button" className="btn danger" disabled={locked || busy} onClick={() => void action(async () => { await accessJson('/api/access/nodes/local/remove', { method: 'POST' }); if (!mounted.current) return; setConfirmRemove(false); setCluster(''); setProject(''); setPrimary(''); setAdvertise(''); setPort('0'); setRole('primary'); setCandidates([]); setToken(''); setIssued(null); setVisible(false); setMessage('Local replication configuration removed. Existing project data is preserved.'); await load(); await onChanged(); })}>Remove configuration</button></div></ModalShell>}
    {confirmStop && <ModalShell title="Stop managed replication?" onClose={() => !busy && setConfirmStop(false)}><p>Stops this project’s managed service. Stored configuration and staged snapshots remain. Other Ax services are unaffected.</p><div className="access-actions"><button type="button" className="btn" disabled={busy} onClick={() => setConfirmStop(false)}>Cancel</button><button type="button" className="btn danger" disabled={locked || busy} onClick={() => void action(async () => { await accessJson('/api/access/nodes/local/stop', { method: 'POST' }); if (!mounted.current) return; setConfirmStop(false); setMessage('Managed node stopped. Its configuration is preserved. Restart it to enable automatic startup again.'); await load(); await onChanged(); })}>Stop replication</button></div></ModalShell>}
  </div>;
}
