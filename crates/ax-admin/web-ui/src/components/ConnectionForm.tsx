import { useEffect, useId, useRef, useState, type FormEvent } from 'react';
import { parseConnectionDetails } from '@cc/connectionTransfer';
import {
  ApiError,
  cancelLogin,
  createConnection,
  loginStatus,
  startLogin,
  testConnection,
  updateConnection,
  type Connection,
} from '../api';
import { EMPTY_INPUT, inputError, suggestedName, type ConnectionInput } from '../connectionInput';

interface Props {
  /** A saved connection to edit; absent for a new one. */
  editing?: Connection;
  submitLabel: string;
  onSaved: (connection: Connection) => void;
  onCancel?: () => void;
}

interface PendingLogin {
  id: string;
  authorizationUrl: string;
}

function message(e: unknown): string {
  return e instanceof ApiError || e instanceof Error ? e.message : 'Something went wrong';
}

/** Address and credentials for one Ax environment. Nothing is saved until the server accepts them. */
export default function ConnectionForm({ editing, submitLabel, onSaved, onCancel }: Props) {
  const ids = useId();
  const [input, setInput] = useState<ConnectionInput>(() =>
    editing
      ? { ...EMPTY_INPUT, name: editing.name, url: editing.url, mode: editing.auth, clientId: editing.clientId ?? '', write: editing.write }
      : EMPTY_INPUT,
  );
  const [busy, setBusy] = useState<'test' | 'save' | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [ok, setOk] = useState<string | null>(null);
  const [pasting, setPasting] = useState(false);
  const [pasted, setPasted] = useState('');
  const [login, setLogin] = useState<PendingLogin | null>(null);
  const saved = useRef(onSaved);
  saved.current = onSaved;

  const set = (patch: Partial<ConnectionInput>) => {
    setInput((current) => ({ ...current, ...patch }));
    setError(null);
    setOk(null);
  };

  // While the browser sign-in is open, ask once a second whether it finished.
  useEffect(() => {
    if (!login) return;
    let stopped = false;
    const timer = window.setInterval(async () => {
      try {
        const status = await loginStatus(login.id);
        if (stopped || status.state === 'pending') return;
        setLogin(null);
        if (status.state === 'connected' && status.connection) saved.current(status.connection);
        else setError(status.message ?? 'Sign-in failed.');
      } catch (e) {
        if (stopped) return;
        setLogin(null);
        setError(message(e));
      }
    }, 1000);
    return () => {
      stopped = true;
      window.clearInterval(timer);
    };
  }, [login]);

  const keyAuth = () => (input.key.trim() ? { type: 'api_key' as const, key: input.key.trim() } : undefined);
  const oauthEdit = editing?.auth === 'oauth';

  const check = (): boolean => {
    const problem = oauthEdit ? null : inputError(input, Boolean(editing));
    setError(problem);
    setOk(null);
    return problem === null;
  };

  const test = async () => {
    if (!check()) return;
    if (input.mode === 'oauth' && !editing) {
      setOk('The address looks right. Access is checked when you sign in.');
      return;
    }
    setBusy('test');
    try {
      const auth = keyAuth();
      const { info } = await testConnection(
        editing && !auth ? { id: editing.id, url: input.url.trim() } : { name: input.name, url: input.url.trim(), auth },
      );
      setOk(`Connected to ${info.serverName || 'Ax'} ${info.serverVersion} with ${info.tools.length} tools.`);
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(null);
    }
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!check()) return;
    setBusy('save');
    try {
      if (editing) {
        const patch = oauthEdit
          ? { name: input.name }
          : { name: input.name, url: input.url.trim(), auth: keyAuth() };
        const { connection } = await updateConnection(editing.id, patch);
        onSaved(connection);
      } else if (input.mode === 'api_key') {
        const { connection } = await createConnection({
          name: input.name,
          url: input.url.trim(),
          auth: { type: 'api_key', key: input.key.trim() },
        });
        onSaved(connection);
      } else {
        const started = await startLogin({
          name: input.name,
          url: input.url.trim(),
          clientId: input.clientId.trim(),
          write: input.write,
        });
        setLogin(started);
        window.open(started.authorizationUrl, '_blank', 'noopener');
      }
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(null);
    }
  };

  const usePasted = () => {
    try {
      const details = parseConnectionDetails(pasted.trim());
      set({
        url: details.projectMcpUrl,
        clientId: details.nativeClientId ?? '',
        mode: details.nativeClientId ? 'oauth' : input.mode,
      });
      setPasting(false);
      setPasted('');
    } catch (e) {
      setError(message(e));
    }
  };

  const stopLogin = async () => {
    if (!login) return;
    const id = login.id;
    setLogin(null);
    await cancelLogin(id).catch(() => undefined);
  };

  if (login) {
    return (
      <div className="admin-form" role="status" aria-live="polite">
        <p>Finish signing in in the browser tab that just opened. This page continues when the environment has granted access.</p>
        <p className="admin-note">
          No new tab? <a href={login.authorizationUrl} target="_blank" rel="noopener noreferrer">Open the sign-in page</a>.
        </p>
        <div className="admin-actions">
          <button type="button" className="btn btn-secondary" onClick={() => void stopLogin()}>Cancel sign-in</button>
        </div>
      </div>
    );
  }

  const placeholderName = suggestedName(input.url) || 'Production';
  return (
    <form className="admin-form" onSubmit={(e) => void submit(e)} noValidate aria-label={editing ? 'Edit connection' : 'New connection'}>
      {error && <div className="settings-toast settings-toast--err" role="alert">{error}</div>}
      {ok && <div className="settings-toast settings-toast--ok" role="status">{ok}</div>}

      <div className="admin-field">
        <label htmlFor={`${ids}-url`}>Address</label>
        <input
          id={`${ids}-url`}
          className="settings-input settings-input--mono"
          type="url"
          inputMode="url"
          autoComplete="off"
          spellCheck={false}
          placeholder="https://ax.example.com/projects/my-project/mcp"
          value={input.url}
          disabled={oauthEdit}
          onChange={(e) => set({ url: e.target.value })}
          required
        />
        <span className="admin-field-hint">The project MCP address of the environment. It is the only way this application talks to it.</span>
      </div>

      {!editing && (
        <div className="admin-field" role="radiogroup" aria-label="Credentials">
          <span className="admin-field-label">Credentials</span>
          <div className="admin-choice">
            <label>
              <input type="radio" name={`${ids}-mode`} checked={input.mode === 'api_key'} onChange={() => set({ mode: 'api_key' })} />
              API key
            </label>
            <label>
              <input type="radio" name={`${ids}-mode`} checked={input.mode === 'oauth'} onChange={() => set({ mode: 'oauth' })} />
              Browser sign-in
            </label>
          </div>
        </div>
      )}

      {input.mode === 'api_key' ? (
        <div className="admin-field">
          <label htmlFor={`${ids}-key`}>API key</label>
          <input
            id={`${ids}-key`}
            className="settings-input settings-input--mono"
            type="password"
            autoComplete="off"
            spellCheck={false}
            placeholder={editing ? 'Leave empty to keep the saved key' : ''}
            value={input.key}
            onChange={(e) => set({ key: e.target.value })}
            required={!editing}
          />
          <span className="admin-field-hint">Stored in this computer's credential store, never in a file. A key with the admin grant shows the management pages.</span>
        </div>
      ) : (
        <>
          <div className="admin-field">
            <label htmlFor={`${ids}-client`}>Client ID</label>
            <input
              id={`${ids}-client`}
              className="settings-input settings-input--mono"
              type="text"
              autoComplete="off"
              spellCheck={false}
              value={input.clientId}
              disabled={oauthEdit}
              onChange={(e) => set({ clientId: e.target.value })}
              required
            />
            <span className="admin-field-hint">The public client ID from the environment's connection details. It is not a password.</span>
          </div>
          {!editing && (
            <label className="admin-choice">
              <input type="checkbox" checked={input.write} onChange={(e) => set({ write: e.target.checked })} />
              Also ask for write access (adding memories and rules)
            </label>
          )}
        </>
      )}

      <div className="admin-field">
        <label htmlFor={`${ids}-name`}>Name <span className="admin-field-hint">(optional)</span></label>
        <input
          id={`${ids}-name`}
          className="settings-input"
          type="text"
          maxLength={80}
          placeholder={placeholderName}
          value={input.name}
          onChange={(e) => set({ name: e.target.value })}
        />
      </div>

      {!editing && (pasting ? (
        <div className="admin-field">
          <label htmlFor={`${ids}-paste`}>Connection details copied from Ax</label>
          <textarea id={`${ids}-paste`} className="settings-input" value={pasted} onChange={(e) => setPasted(e.target.value)} spellCheck={false} />
          <div className="admin-actions">
            <button type="button" className="btn btn-secondary" onClick={usePasted} disabled={!pasted.trim()}>Use these details</button>
            <button type="button" className="btn btn-subtle" onClick={() => { setPasting(false); setPasted(''); }}>Cancel</button>
          </div>
        </div>
      ) : (
        <div className="admin-actions">
          <button type="button" className="btn btn-subtle" onClick={() => setPasting(true)}>Paste connection details</button>
        </div>
      ))}

      <div className="admin-actions admin-actions--end">
        {onCancel && <button type="button" className="btn btn-subtle" onClick={onCancel}>Cancel</button>}
        {!oauthEdit && (
          <button type="button" className="btn btn-secondary" onClick={() => void test()} disabled={busy !== null}>
            {busy === 'test' ? 'Testing…' : 'Test'}
          </button>
        )}
        <button type="submit" className="btn" disabled={busy !== null}>
          {busy === 'save' ? 'Connecting…' : input.mode === 'oauth' && !editing ? 'Sign in with browser' : submitLabel}
        </button>
      </div>
    </form>
  );
}
