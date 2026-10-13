import { useEffect, useState, type FormEvent } from 'react';
import { registerPasskey } from '../lib/passkeys';
import { instantMs, memoryRowAge } from '../memoryHeadline';
import { useNow } from '../lib/useNow';

type Passkey = { id: string; label: string; created_at: string; revoked: boolean };
type Status = { enabled: boolean; passkeys: boolean; origin?: string };
type WebSession = { id: string; current: boolean; created_at: number; last_seen_at: number; expires_at: number };
async function request(url: string, init?: RequestInit) {
  const result = await fetch(url, { cache: 'no-store', credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json' }, ...init });
  const data = await result.json();
  if (!result.ok) throw new Error(data.error || 'Request failed');
  return data;
}
export default function SecuritySettingsSection() {
  const now = useNow();
  const [status, setStatus] = useState<Status | null>(null);
  const [keys, setKeys] = useState<Passkey[]>([]);
  const [sessions, setSessions] = useState<WebSession[]>([]);
  const [recoveryCodes, setRecoveryCodes] = useState<string[] | null>(null);
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const reload = async () => {
    const value = await request('/api/web-auth/status') as Status;
    setStatus(value);
    const response = await fetch('/api/web-auth/passkeys', { cache: 'no-store' });
    if (response.ok) setKeys(((await response.json()) as { passkeys: Passkey[] }).passkeys);
    if (value.enabled) {
      const sessionsResponse = await fetch('/api/web-auth/sessions', { cache: 'no-store' });
      if (sessionsResponse.ok) setSessions(((await sessionsResponse.json()) as { sessions: WebSession[] }).sessions);
    } else setSessions([]);
  };
  useEffect(() => { void reload().catch(e => setError(String(e))); }, []);
  async function execute(fn: () => Promise<unknown>, message: string) {
    setBusy(true); setError(null); setNotice(null);
    try {
      await fn(); await reload(); setNotice(message);
    } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setBusy(false); }
  }
  async function setup(event: FormEvent) {
    event.preventDefault();
    await execute(() => request('/api/web-auth/setup', { method: 'POST', body: JSON.stringify({ password }) }), 'Owner password configured.');
    setPassword('');
  }
  async function toggle() {
    if (!status) return;
    if (status.enabled && !window.confirm('Disable sign-in for local Command Center? Current sessions will be ended.')) return;
    await execute(() => request('/api/web-auth/config', { method: 'POST',
      body: JSON.stringify({ enabled: !status.enabled }) }), status.enabled ? 'Sign-in disabled.' : 'Sign-in enabled.');
    window.dispatchEvent(new Event('ax-web-auth-changed'));
  }
  async function generateCodes() {
    if (!window.confirm('Regenerate recovery codes? Your previous codes will stop working.')) return;
    await execute(async () => {
      const response = await request('/api/web-auth/recovery/rotate', { method: 'POST' }) as { codes: string[] };
      setRecoveryCodes(response.codes);
    }, 'Recovery codes generated. Store them safely.');
  }
  async function revokeSession(id: string) {
    if (!window.confirm('End this browser session?')) return;
    await execute(() => request(`/api/web-auth/sessions/${id}`, { method: 'DELETE' }), 'Session ended.');
    const current = sessions.find(session => session.id === id)?.current;
    if (current) window.dispatchEvent(new Event('ax-web-auth-changed'));
  }
  async function signout() {
    await execute(() => request('/api/web-auth/logout', { method: 'POST' }), 'Signed out.');
    window.dispatchEvent(new Event('ax-web-auth-changed'));
  }
  return <div id="settings-panel-security">
    <section className="settings-card">
      <div className="settings-card-header"><h2>Web authentication</h2><p>Optional login for Command Center users. Machine clients and replication have separate credentials.</p></div>
      <div className="settings-card-body">
        {error && <p role="alert" className="settings-toast settings-toast--err">{error}</p>}
        {notice && <p role="status" className="settings-toast settings-toast--ok">{notice}</p>}
        <div className="settings-row"><div className="settings-row-label">
          <strong>Require sign-in</strong><p>When disabled, this address works without a password, including through a reverse proxy. Turn sign-in on to require a password or a passkey.</p>
        </div><div className="settings-row-control">
          <button className="btn" disabled={busy || !status} type="button" onClick={() => void toggle()}>
            {status?.enabled ? 'Disable sign-in' : 'Enable sign-in'}
          </button>
        </div></div>
        {!status?.enabled && <form className="access-form" onSubmit={setup}>
          <h3>Set initial Owner password</h3>
          <p>Choose at least 12 characters. Configure a password or a passkey before enabling sign-in.</p>
          <label>Password<input className="settings-input" type="password" value={password}
            autoComplete="new-password" minLength={12} required onChange={event => setPassword(event.target.value)} /></label>
          <button type="submit" className="btn" disabled={busy || password.length < 12}>Configure password</button>
        </form>}
        {status?.enabled && <button type="button" className="btn" disabled={busy} onClick={() => void signout()}>Sign out</button>}
      </div>
    </section>
    <section className="settings-card">
      <div className="settings-card-header"><h2>Recovery codes</h2><p>Keep a private, one-time recovery method in case all your passkeys are unavailable.</p></div>
      <div className="settings-card-body">
        {recoveryCodes ? <div role="region" aria-label="New recovery codes" className="access-token">
          <p>These codes are displayed only now. Store them offline. Each code works once.</p>
          <ul>{recoveryCodes.map(code => <li key={code}><code>{code}</code></li>)}</ul>
          <button className="btn" type="button" onClick={() => {
            void navigator.clipboard.writeText(recoveryCodes.join('\n'));
          }}>Copy codes</button>
          <button className="btn" type="button" onClick={() => setRecoveryCodes(null)}>I've saved these codes</button>
        </div> : <button className="btn" type="button" disabled={busy} onClick={() => void generateCodes()}>Generate recovery codes</button>}
      </div>
    </section>
    {status?.enabled && <section className="settings-card">
      <div className="settings-card-header"><h2>Active sessions</h2><p>End browser sessions you no longer recognize.</p></div>
      <div className="settings-card-body">
        {sessions.map(session => <div className="settings-row" key={session.id}>
          <div className="settings-row-label"><strong>{session.current ? 'This browser' : 'Other browser'}</strong>
            <p>Last active {memoryRowAge(session.last_seen_at * 1000, now)}</p>
          </div>
          <button className="btn danger" type="button" disabled={busy} onClick={() => void revokeSession(session.id)}>End session</button>
        </div>)}
        {!sessions.length && <p>No active sessions found.</p>}
      </div>
    </section>}
    <section className="settings-card">
      <div className="settings-card-header"><h2>My passkeys</h2><p>Register Touch ID, Face ID, Windows Hello or a hardware security key. You can add more than one.</p></div>
      <div className="settings-card-body">
        {status?.origin && status.origin !== window.location.origin &&
          <p className="access-note">Passkeys are tied to {status.origin}. Open that address to register or use them.</p>}
        <button className="btn primary" type="button"
          disabled={busy || !status?.passkeys || Boolean(status?.origin && status.origin !== window.location.origin)}
          onClick={() => void execute(registerPasskey, 'Passkey registered.')}>Add passkey</button>
        <div className="access-client-list">{keys.filter(key => !key.revoked).map(key =>
          <div key={key.id} className="settings-row">
            <div className="settings-row-label"><strong>{key.label}</strong><small>Created {(() => { const ms = instantMs(key.created_at); return ms == null ? key.created_at : memoryRowAge(ms, now); })()}</small></div>
            <div className="settings-row-control access-actions">
              <button type="button" className="btn" disabled={busy} onClick={() => {
                const label = window.prompt('Passkey name', key.label);
                if (label?.trim()) void execute(() => request(`/api/web-auth/passkeys/${key.id}`, {
                  method: 'PATCH', body: JSON.stringify({ label: label.trim() }),
                }), 'Passkey renamed.');
              }}>Rename</button>
              <button type="button" className="btn danger" disabled={busy}
                onClick={() => {
                  if (window.confirm('Revoke this passkey? This device will no longer sign in.')) {
                    void execute(() => request(`/api/web-auth/passkeys/${key.id}`, { method: 'DELETE' }), 'Passkey revoked.');
                  }
                }}>Revoke</button>
            </div>
          </div>)}</div>
        {!keys.some(key => !key.revoked) && <p>No passkeys registered yet.</p>}
      </div>
    </section>
  </div>;
}
