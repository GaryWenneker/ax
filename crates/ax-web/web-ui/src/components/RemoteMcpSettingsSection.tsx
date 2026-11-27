import { useEffect, useRef, useState } from 'react';
import RemoteMcpPublishSection from './RemoteMcpPublishSection';
import ModalShell from './ModalShell';
import { parseConnectionDetails } from '../connectionTransfer';

type Connection = { url: string; client_id: string; issuer: string; state: 'stored' | 'expired'; expires_at: number };
/** A failed remote-mcp request; `status` is the HTTP status, or 0 when the server did not answer. */
class RemoteRequestError extends Error {
  constructor(message: string, readonly status: number) {
    super(message);
  }
}

async function request(path: string, body?: unknown, csrf?: string) {
  let response: Response;
  try {
    response = await fetch(`/api/remote-mcp${path}`, {
      method: body ? 'POST' : 'GET',
      headers: body ? { 'Content-Type': 'application/json', 'X-Ax-Csrf': csrf ?? '' } : {},
      body: body ? JSON.stringify(body) : undefined,
    });
  } catch {
    throw new RemoteRequestError('The Command Center server did not answer.', 0);
  }
  const value = await response.json().catch(() => null);
  if (!response.ok || value === null) {
    throw new RemoteRequestError(value?.error ?? `Remote connection request failed (HTTP ${response.status})`, response.status);
  }
  return value;
}

/** Poll failures in a row after which a pending browser sign-in is given up. */
const MAX_POLL_FAILURES = 10;

export default function RemoteMcpSettingsSection() {
  const [showPaste, setShowPaste] = useState(false);
  const [pasted, setPasted] = useState('');
  const [pasteError, setPasteError] = useState('');
  const [url, setUrl] = useState('');
  const [clientId, setClientId] = useState('');
  const [csrf, setCsrf] = useState('');
  const [connections, setConnections] = useState<Connection[]>([]);
  const [attempt, setAttempt] = useState<string | null>(null);
  const [authorizationUrl, setAuthorizationUrl] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function load() {
    const data = await request('/');
    setConnections(data.connections);
    setCsrf(data.csrf);
  }
  useEffect(() => { load().catch(e => setError(String(e.message))); }, []);
  const loadRef = useRef(load);
  loadRef.current = load;
  useEffect(() => {
    if (!attempt) return;
    let stopped = false;
    let inFlight = false;
    let failures = 0;
    function drop(message: string) {
      setAttempt(null); setBusy(false); setAuthorizationUrl(null); setError(message);
      loadRef.current().catch(() => {});
    }
    // One poll at a time; a tick that finds a poll still running is skipped.
    const timer = setInterval(async () => {
      if (inFlight) return;
      inFlight = true;
      try {
        const status = await request(`/login/${encodeURIComponent(attempt)}`);
        failures = 0;
        if (stopped || status.state === 'pending') return;
        setAttempt(null); setBusy(false); setAuthorizationUrl(null);
        if (status.state === 'connected') { setMessage('Connected. Ax verified access to this project.'); await loadRef.current(); }
        else setError(status.message ?? 'Sign-in did not complete');
      } catch (e) {
        if (stopped) return;
        const message = e instanceof Error ? e.message : String(e);
        // The server no longer knows this sign-in: stop now. Any other failure (a dropped
        // connection, a proxy error page) may pass, so keep polling for a while.
        const status = e instanceof RemoteRequestError ? e.status : 0;
        if (status === 404 || status === 410) { drop(message); return; }
        failures++;
        if (failures >= MAX_POLL_FAILURES) drop(`Lost contact with the sign-in: ${message}`);
      } finally {
        inFlight = false;
      }
    }, 1000);
    return () => { stopped = true; clearInterval(timer); };
  }, [attempt]);

  function importConnection() {
    try {
      const details = parseConnectionDetails(pasted);
      setUrl(details.projectMcpUrl); setClientId(details.nativeClientId ?? '');
      setError(null); setPasteError(''); setShowPaste(false); setPasted('');
      setMessage(details.nativeClientId ? 'Connection details loaded. Choose Connect with browser to sign in with your own account.' : 'MCP URL loaded. Ask the server owner for the public native Ax client ID before connecting.');
    } catch (e) { setPasteError(e instanceof Error ? e.message : String(e)); }
  }
  async function connect() {
    setError(null); setMessage(null); setBusy(true);
    // Open synchronously so the browser's popup blocker permits the user gesture.
    const popup = window.open('about:blank', '_blank');
    if (popup) popup.opener = null;
    try {
      const value = await request('/login', { url: url.trim(), client_id: clientId.trim() }, csrf);
      setAttempt(value.id); setAuthorizationUrl(value.authorization_url);
      if (popup) popup.location.href = value.authorization_url;
    } catch (e) { popup?.close(); setBusy(false); setError(e instanceof Error ? e.message : String(e)); }
  }
  async function cancel() {
    if (!attempt) return;
    try {
      await request(`/login/${encodeURIComponent(attempt)}/cancel`, {}, csrf);
      setAttempt(null); setAuthorizationUrl(null); setBusy(false); setMessage('Sign-in cancelled.');
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
  }
  async function disconnect(connection: Connection) {
    setBusy(true); setError(null); setMessage(null);
    try { await request('/logout', { url: connection.url }, csrf); await load(); setMessage('Local credentials removed.'); }
    catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setBusy(false); }
  }

  return <><RemoteMcpPublishSection /><section className="remote-mcp-settings" aria-labelledby="remote-mcp-title">
    <h3 id="remote-mcp-title">Connect to another Ax</h3>
    <p>Use a project hosted on another computer. Ask that computer’s owner to choose <strong>Copy connection details</strong> in Ax, paste them here, then complete authentication. A built-in host asks its owner to approve your request.</p>
    <div className="remote-publish-actions"><button type="button" className="btn btn-primary" disabled={busy} onClick={() => { setPasteError(''); setShowPaste(true); }}>Paste connection details</button></div>
    {showPaste && <ModalShell title="Paste Ax connection details" onClose={() => setShowPaste(false)} footer={<><button className="btn" type="button" onClick={() => setShowPaste(false)}>Cancel</button><button className="btn btn-primary" type="button" disabled={!pasted.trim()} onClick={importConnection}>Use connection</button></>}>
      <p>Paste the JSON copied with <strong>Copy connection details</strong> on the hosting computer. This fills in the address and public sign-in ID. You will sign in yourself afterward.</p>
      <label className="remote-connection-paste">Connection details<textarea aria-label="Received Ax connection details" value={pasted} onChange={e => setPasted(e.target.value)} rows={7} spellCheck={false} /></label>
      {pasteError && <p role="alert">{pasteError}</p>}
    </ModalShell>}
    <p>Or enter the two values supplied by the hosting computer:</p>
    <div className="settings-row">
      <label htmlFor="remote-mcp-url" className="settings-row-label">Project MCP URL</label>
      <input id="remote-mcp-url" type="url" value={url} onChange={e => setUrl(e.target.value)} placeholder="https://ax.example.com/projects/my-project/mcp" disabled={busy} />
    </div>
    <div className="settings-row">
      <label htmlFor="remote-mcp-client" className="settings-row-label">OAuth client ID</label>
      <input id="remote-mcp-client" value={clientId} onChange={e => setClientId(e.target.value)} placeholder="Native Ax client registered with your provider" disabled={busy} />
    </div>
    <p>The OAuth client ID is the public native Ax app ID from the server owner. It is not your password or the Cursor app ID.</p>
    <div className="access-actions">
      <button className="btn" type="button" onClick={connect} disabled={busy || !url.trim() || !clientId.trim()}>{busy ? 'Waiting for browser sign-in…' : 'Connect with browser'}</button>
      {attempt && <button className="btn" type="button" onClick={cancel}>Cancel sign-in</button>}
    </div>
    {authorizationUrl && <p><a href={authorizationUrl} target="_blank" rel="noopener noreferrer">Open sign-in if your browser did not open</a>. Return here after authorizing Ax.</p>}
    {message && <p role="status">{message}</p>}
    {error && <p role="alert">{error}</p>}
    {connections.length > 0 && <ul>{connections.map(connection => <li key={connection.url}>
      <strong>{connection.url}</strong><br />
      <span>{connection.state === 'stored' ? 'Credentials saved' : 'Access token expired; the CLI can refresh it when a refresh token is available'} · {connection.issuer}</span>{' '}
      <div className="access-actions"><button className="btn" type="button" disabled={busy} onClick={() => disconnect(connection)}>{connection.state === 'expired' ? 'Remove saved connection' : 'Disconnect'}</button></div>
    </li>)}</ul>}
  </section></>;
}
