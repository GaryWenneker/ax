import { useCallback, useEffect, useState, type FormEvent, type ReactNode } from 'react';
import { signInPasskey } from '../lib/passkeys';

type Session = { authenticated: boolean; member?: { display_name: string; roles: string[] } };
async function checkedJson(url: string, init?: RequestInit) {
  const response = await fetch(url, { credentials: 'same-origin', cache: 'no-store', ...init });
  const data = await response.json();
  if (!response.ok) throw new Error(data.error || 'Authentication failed');
  return data;
}
export default function AuthGate({ children }: { children: ReactNode }) {
  const [mode, setMode] = useState<'loading' | 'open' | 'login' | 'error'>('loading');
  const [identity, setIdentity] = useState('');
  const [password, setPassword] = useState('');
  const [useRecovery, setUseRecovery] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inspect = useCallback(async () => {
    try {
      const status = await checkedJson('/api/web-auth/status') as { enabled: boolean };
      if (!status.enabled) { setMode('open'); return; }
      const session = await fetch('/api/web-auth/session', { cache: 'no-store' });
      setMode(session.ok ? 'open' : session.status === 401 ? 'login' : 'error');
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Cannot check authentication');
      setMode('error');
    }
  }, []);
  useEffect(() => { void inspect(); }, [inspect]);
  useEffect(() => {
    if (mode !== 'open') return;
    const interval = window.setInterval(() => { void inspect(); }, 30_000);
    const visible = () => { if (document.visibilityState === 'visible') void inspect(); };
    document.addEventListener('visibilitychange', visible);
    return () => { window.clearInterval(interval); document.removeEventListener('visibilitychange', visible); };
  }, [inspect, mode]);
  useEffect(() => {
    const listener = () => { void inspect(); };
    window.addEventListener('ax-web-auth-changed', listener);
    return () => window.removeEventListener('ax-web-auth-changed', listener);
  }, [inspect]);
  async function submitPassword(event: FormEvent) {
    event.preventDefault();
    setBusy(true); setError(null);
    try {
      await checkedJson(useRecovery ? '/api/web-auth/recovery/login' : '/api/web-auth/login', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(useRecovery ? { identity, code: password } : { identity, password }),
      });
      setPassword(''); await inspect();
    } catch (e) { setError(e instanceof Error ? e.message : 'Sign in failed'); }
    finally { setBusy(false); }
  }
  async function submitPasskey() {
    setBusy(true); setError(null);
    try { await signInPasskey(identity); await inspect(); }
    catch (e) { setError(e instanceof Error ? e.message : 'Passkey sign-in failed'); }
    finally { setBusy(false); }
  }
  if (mode === 'loading') return <div className="auth-loading" role="status">Checking authentication…</div>;
  if (mode === 'error') return <main className="auth-page" role="alert"><h1>Authentication unavailable</h1><p>{error || 'Unable to reach the server.'}</p><button onClick={() => void inspect()} type="button" className="btn">Retry</button></main>;
  if (mode === 'open') return <>{children}</>;
  return <main className="auth-page">
    <form className="auth-card" onSubmit={submitPassword}>
      <span className="auth-eyebrow">Ax · Command Center</span>
      <h1>Sign in</h1>
      <p>Sign in with your account or an enrolled passkey.</p>
      {error && <p className="settings-toast settings-toast--err" role="alert">{error}</p>}
      <label htmlFor="auth-identity">User name or email</label>
      <input id="auth-identity" autoComplete="username webauthn" required value={identity}
        onChange={event => setIdentity(event.target.value)} className="settings-input" />
      <label htmlFor="auth-password">{useRecovery ? 'Recovery code' : 'Password'}</label>
      <input id="auth-password" type="password" autoComplete={useRecovery ? 'off' : 'current-password'} value={password}
        onChange={event => setPassword(event.target.value)} className="settings-input" />
      <button type="submit" className="btn primary" disabled={busy || !identity.trim() || !password}>{useRecovery ? 'Recover account' : 'Sign in'}</button>
      <button type="button" className="btn auth-passkey" disabled={busy || !identity.trim()} onClick={() => void submitPasskey()}>
        <i className="codicon codicon-key" aria-hidden="true" /> Sign in with passkey
      </button>
      <button type="button" className="btn" disabled={busy} onClick={() => { setUseRecovery(!useRecovery); setPassword(''); setError(null); }}>
        {useRecovery ? 'Use password instead' : 'Use a recovery code'}
      </button>
      <small>Passkeys use Touch ID, Face ID, Windows Hello or a security key, depending on your device.</small>
    </form>
  </main>;
}
