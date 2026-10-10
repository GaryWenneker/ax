import { useEffect, useState } from 'react';

type Connection = { url: string; client_id: string; issuer: string; state: 'stored' | 'expired'; expires_at: number };
async function request(path: string, body?: unknown, csrf?: string) {
  const response = await fetch(`/api/remote-mcp${path}`, {
    method: body ? 'POST' : 'GET',
    headers: body ? { 'Content-Type': 'application/json', 'X-Ax-Csrf': csrf ?? '' } : {},
    body: body ? JSON.stringify(body) : undefined,
  });
  const value = await response.json();
  if (!response.ok) throw new Error(value.error ?? 'Remote connection request failed');
  return value;
}

export default function RemoteMcpSettingsSection() {
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
  useEffect(() => {
    if (!attempt) return;
    let stopped = false;
    const timer = setInterval(async () => {
      try {
        const status = await request(`/login/${encodeURIComponent(attempt)}`);
        if (stopped || status.state === 'pending') return;
        setAttempt(null); setBusy(false); setAuthorizationUrl(null);
        if (status.state === 'connected') { setMessage('Connected. Ax verified access to this project.'); await load(); }
        else setError(status.message ?? 'Sign-in did not complete');
      } catch (e) {
        if (!stopped) { setError(e instanceof Error ? e.message : String(e)); setAttempt(null); setBusy(false); }
      }
    }, 1000);
    return () => { stopped = true; clearInterval(timer); };
  }, [attempt]);

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

  return <section className="remote-mcp-settings" aria-labelledby="remote-mcp-title">
    <h3 id="remote-mcp-title">Remote Ax connections</h3>
    <p>Use the context from Ax running on another computer. Sign in through your browser; credentials stay in your operating system’s secure credential store.</p>
    <div className="settings-row">
      <label htmlFor="remote-mcp-url" className="settings-row-label">Project MCP URL</label>
      <input id="remote-mcp-url" type="url" value={url} onChange={e => setUrl(e.target.value)} placeholder="https://ax.example.com/projects/my-project/mcp" disabled={busy} />
    </div>
    <div className="settings-row">
      <label htmlFor="remote-mcp-client" className="settings-row-label">OAuth client ID</label>
      <input id="remote-mcp-client" value={clientId} onChange={e => setClientId(e.target.value)} placeholder="Native Ax client registered with your provider" disabled={busy} />
    </div>
    <button type="button" onClick={connect} disabled={busy || !url.trim() || !clientId.trim()}>{busy ? 'Waiting for browser sign-in…' : 'Connect with browser'}</button>
    {attempt && <button type="button" onClick={cancel}>Cancel sign-in</button>}
    {authorizationUrl && <p><a href={authorizationUrl} target="_blank" rel="noopener noreferrer">Open sign-in if your browser did not open</a>. Return here after authorizing Ax.</p>}
    {message && <p role="status">{message}</p>}
    {error && <p role="alert">{error}</p>}
    {connections.length > 0 && <ul>{connections.map(connection => <li key={connection.url}>
      <strong>{connection.url}</strong><br />
      <span>{connection.state === 'stored' ? 'Credentials saved' : 'Access token expired; the CLI can refresh it when a refresh token is available'} · {connection.issuer}</span>{' '}
      <button type="button" disabled={busy} onClick={() => disconnect(connection)}>Disconnect</button>
    </li>)}</ul>}
  </section>;
}
