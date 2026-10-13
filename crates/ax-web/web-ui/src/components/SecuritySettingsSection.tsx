import { useEffect, useState, type FormEvent } from 'react';
import { registerPasskey } from '../lib/passkeys';

type Passkey = { id: string; label: string; created_at: string; revoked: boolean };
type Status = { enabled: boolean; passkeys: boolean };
async function request(url: string, init?: RequestInit) {
  const result = await fetch(url, { cache: 'no-store', credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json' }, ...init });
  const data = await result.json();
  if (!result.ok) throw new Error(data.error || 'Request failed');
  return data;
}
export default function SecuritySettingsSection() {
  const [status, setStatus] = useState<Status | null>(null);
  const [keys, setKeys] = useState<Passkey[]>([]);
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const reload = async () => {
    const value = await request('/api/web-auth/status') as Status;
    setStatus(value);
    const response = await fetch('/api/web-auth/passkeys', { cache: 'no-store' });
    if (response.ok) setKeys(((await response.json()) as { passkeys: Passkey[] }).passkeys);
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
          <strong>Require sign-in</strong><p>When disabled, local Command Center keeps the existing trusted-local behavior. Remote access remains protected.</p>
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
      <div className="settings-card-header"><h2>My passkeys</h2><p>Register Touch ID, Face ID, Windows Hello or a hardware security key. You can add more than one.</p></div>
      <div className="settings-card-body">
        <button className="btn primary" type="button" disabled={busy || !status?.passkeys}
          onClick={() => void execute(registerPasskey, 'Passkey registered.')}>Add passkey</button>
        <div className="access-client-list">{keys.filter(key => !key.revoked).map(key =>
          <div key={key.id} className="settings-row">
            <div className="settings-row-label"><strong>{key.label}</strong><small>Created {key.created_at}</small></div>
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
