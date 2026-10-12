import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react';
import { publicationRequest } from '../remoteMcpApi';
import ModalShell from './ModalShell';
import ServiceStatus from './ServiceStatus';

type Host = { status: 'running' | 'stopped' | 'error'; message: string | null; publicUrl: string; localUrl: string; builtInOAuth: boolean; autoStart: boolean };
type Approval = { id: string; clientName: string; scope: string; status: string; expires: number; revoked: boolean; redirectUri: string };
type HostState = { workspaceId: string; host: Host | null };
export default function RemoteHostSetup({ workspaceId, csrf, onChanged }: { workspaceId: string; csrf: string; onChanged: () => Promise<void> }) {
  const [host, setHost] = useState<Host | null>(null);
  const [requests, setRequests] = useState<Approval[]>([]);
  const [ready, setReady] = useState(false);
  const [open, setOpen] = useState(false);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [publicUrl, setPublicUrl] = useState('');
  const [file, setFile] = useState<File | null>(null);
  const [nativeId, setNativeId] = useState('');
  const [cursorId, setCursorId] = useState('');
  const [vscodeId, setVscodeId] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const alive = useRef(true);
  const working = useRef(false);
  const epoch = useRef(0);
  const load = useCallback(async () => {
    const version = ++epoch.current;
    const next = await publicationRequest<HostState>('/host');
    const approvals = await publicationRequest<{ workspaceId: string; requests: Approval[] }>('/host/authorizations');
    if (!alive.current || version !== epoch.current || next.workspaceId !== workspaceId || approvals.workspaceId !== workspaceId) return;
    setHost(next.host); setRequests(approvals.requests); setReady(true);
  }, [workspaceId]);
  useEffect(() => {
    alive.current = true;
    void load().catch(e => alive.current && setError(e instanceof Error ? e.message : String(e)));
    const timer = setInterval(() => { if (!working.current) void load().catch(e => alive.current && setError(e instanceof Error ? e.message : String(e))); }, 5000);
    return () => { alive.current = false; ++epoch.current; clearInterval(timer); };
  }, [load]);
  async function act(operation: () => Promise<void>) {
    if (working.current) return;
    working.current = true; ++epoch.current; setBusy(true); setError('');
    try { await operation(); if (alive.current) await load(); }
    catch (e) { if (alive.current) setError(e instanceof Error ? e.message : String(e)); }
    finally { working.current = false; if (alive.current) setBusy(false); }
  }
  async function setup(event: FormEvent) {
    event.preventDefault();
    await act(async () => {
      let config: unknown;
      if (file) { if (file.size > 256 * 1024) throw new Error('Server configuration exceeds 256 KiB.'); config = JSON.parse(await file.text()) as unknown; }
      await publicationRequest('/host/setup', { method: 'POST', csrf, body: { workspaceId, publicUrl: publicUrl.trim() || null, config: config ?? null, clients: { axProxy: nativeId.trim() || null, cursor: cursorId.trim() || null, vscode: vscodeId.trim() || null } } });
      if (alive.current) setOpen(false);
      await onChanged();
    });
  }
  async function change(operation: string) {
    await publicationRequest('/host/action', { method: 'POST', csrf, body: { workspaceId, operation } });
    if (alive.current) setConfirmRemove(false);
    await onChanged();
  }
  async function decide(request: Approval, decision: string) {
    await publicationRequest(`/host/authorizations/${encodeURIComponent(request.id)}`, { method: 'POST', csrf, body: { workspaceId, decision } });
  }
  return <section className="remote-host-setup" aria-label="Remote MCP host">
    <h3>Host this project</h3>
    <p>Run a remote MCP service on this computer. Ax generates the server file and native client ID, stores its settings and starts it automatically.</p>
    <div className="service-toolbar"><ServiceStatus state={error || host?.status === 'error' ? 'error' : host?.status === 'running' ? 'running' : 'stopped'} label={host?.status === 'running' ? 'Remote MCP running' : host ? 'Remote MCP stopped' : 'Remote MCP not configured'} active={host?.status === 'running'} />
      {!host ? <button type="button" className="btn primary" disabled={!ready || busy} onClick={() => setOpen(true)}>Set up remote MCP</button> : <div className="access-actions">
        <button type="button" className="btn" disabled={busy} onClick={() => void act(() => change(host.status === 'running' ? 'stop' : 'start'))}>{host.status === 'running' ? 'Stop host' : 'Start host'}</button>
        <button type="button" className="btn danger" disabled={busy} onClick={() => setConfirmRemove(true)}>Remove host</button>
      </div>}
    </div>
    {host && <dl className="replication-config-details"><div><dt>Public HTTPS origin</dt><dd>{host.publicUrl}</dd></div><div><dt>Proxy target</dt><dd>{host.localUrl}</dd></div><div><dt>Authentication</dt><dd>{host.builtInOAuth ? 'Built-in OAuth with host approval' : 'External OAuth provider'}</dd></div><div><dt>Automatic startup</dt><dd>{host.autoStart ? 'Enabled' : 'Stopped by owner'}</dd></div></dl>}
    {host?.message && <p role="alert">{host.message}</p>}
    {host?.builtInOAuth && <div aria-label="OAuth authorization requests"><h4>Connection approvals</h4><p>Match the request ID shown on the connecting computer before approving its access to this project.</p>
      {!requests.length && <p className="access-note">No connection requests.</p>}
      {requests.map(request => <article key={request.id} className="access-client"><h4>{request.clientName}</h4><p><code>{request.id}</code></p><p>{request.scope.includes('ax:write') ? 'Read and write access' : 'Read context access'} · {request.revoked ? 'Revoked' : request.status}</p><p className="access-note">Callback: {request.redirectUri}</p><div className="access-actions">
        {request.status === 'pending' && !request.revoked ? <><button type="button" className="btn primary" disabled={busy} onClick={() => void act(() => decide(request, 'approve'))}>{request.scope.includes('ax:write') ? 'Approve read and write' : 'Approve context access'}</button><button type="button" className="btn" disabled={busy} onClick={() => void act(() => decide(request, 'deny'))}>Deny</button></> : !request.revoked && <button type="button" className="btn danger" disabled={busy} onClick={() => void act(() => decide(request, 'revoke'))}>Revoke access</button>}
      </div></article>)}
    </div>}
    {error && !open && <p role="alert">{error}</p>}
    {open && <ModalShell title="Set up remote MCP host" onClose={() => !busy && setOpen(false)}><form className="access-form" onSubmit={setup}>
      <p>Enter the HTTPS address that routes to this computer. Ax creates the configuration and OAuth client ID and starts the service. Route that address to the proxy target shown after setup.</p>
      <label>Public HTTPS origin<input className="settings-input" type="url" placeholder="https://ax.example.com" required={!file} disabled={busy} value={publicUrl} onChange={e => setPublicUrl(e.target.value)} /></label>
      <details><summary>Use an existing server file (optional)</summary><p>Select an Ax server configuration if you already have one. Its project must match the project open on this computer.</p><label>Server configuration file<input type="file" accept=".json,application/json" disabled={busy} onChange={e => setFile(e.target.files?.[0] ?? null)} /></label></details>
      <details><summary>Advanced OAuth options</summary><p>Without a file, Ax supplies its own issuer (the authentication service), JWKS endpoint (used by external providers for signature keys) and native client ID (the app identifier). Built-in access uses project-scoped opaque tokens and does not require signing-key files.</p><p>For external OAuth, issuer and JWKS come from the selected server file. Public client IDs must be registered with that provider; Ax cannot invent its registrations. These identifiers are public, and are not passwords or client secrets.</p>
        <label>Native Ax public client ID<input className="settings-input" disabled={busy || !file} value={nativeId} onChange={e => setNativeId(e.target.value)} /></label>
        <label>Cursor public client ID<input className="settings-input" disabled={busy || !file} value={cursorId} onChange={e => setCursorId(e.target.value)} /></label>
        <label>VS Code public client ID<input className="settings-input" disabled={busy || !file} value={vscodeId} onChange={e => setVscodeId(e.target.value)} /></label>
      </details>
      {error && <p role="alert">{error}</p>}
      <div className="access-actions"><button type="button" className="btn" disabled={busy} onClick={() => setOpen(false)}>Cancel</button><button type="submit" className="btn primary" disabled={busy}>{busy ? 'Starting…' : 'Generate and start host'}</button></div>
    </form></ModalShell>}
    {confirmRemove && <ModalShell title="Remove remote MCP host?" onClose={() => !busy && setConfirmRemove(false)}><p>Stop this managed service, revoke its built-in grants and remove its saved host configuration.</p>{error && <p role="alert">{error}</p>}<div className="access-actions"><button type="button" className="btn" disabled={busy} onClick={() => setConfirmRemove(false)}>Cancel</button><button type="button" className="btn danger" disabled={busy} onClick={() => void act(() => change('remove'))}>Remove host</button></div></ModalShell>}
  </section>;
}
