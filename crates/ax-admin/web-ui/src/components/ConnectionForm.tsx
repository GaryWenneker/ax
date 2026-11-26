import { useId, useState, type FormEvent } from 'react';
import { parseConnectionDetails } from '@cc/connectionTransfer';
import { createConnection, startLogin, testConnection, updateConnection, type Connection, type StartedSignIn } from '../api';
import { EMPTY_INPUT, inputProblem, suggestedName, type ConnectionInput, type InputField } from '../connectionInput';
import SignInWait from './SignInWait';

interface Props {
  /** A saved connection to edit; absent for a new one. */
  editing?: Connection;
  submitLabel: string;
  onSaved: (connection: Connection) => void;
  onCancel?: () => void;
}

type Note = { kind: 'ok'; text: string } | { kind: 'err'; text: string; field: InputField | null };

const describe = (e: unknown): string => (e instanceof Error ? e.message : 'Something went wrong');

/** Opens the sign-in page. A browser may refuse a tab that is not opened by the click itself. */
function openTab(): Window | null {
  return window.open('about:blank', '_blank');
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
  const [note, setNote] = useState<Note | null>(null);
  const [pasted, setPasted] = useState<string | null>(null);
  const [signIn, setSignIn] = useState<{ started: StartedSignIn; blocked: boolean } | null>(null);

  const set = (patch: Partial<ConnectionInput>) => {
    setInput((current) => ({ ...current, ...patch }));
    setNote(null);
  };
  const fail = (e: unknown) => setNote({ kind: 'err', text: describe(e), field: null });

  const oauthEdit = editing?.auth === 'oauth';
  const key = input.key.trim();
  const addressChanged = editing !== undefined && input.url.trim() !== editing.url;

  const check = (): boolean => {
    let problem = oauthEdit ? null : inputProblem(input, Boolean(editing));
    // A saved key belongs to its address; another address needs its own key.
    if (!problem && addressChanged && !key) problem = { field: 'key', message: 'Enter the key again to use another address.' };
    setNote(problem ? { kind: 'err', text: problem.message, field: problem.field } : null);
    return problem === null;
  };

  const test = async () => {
    if (!check()) return;
    if (input.mode === 'oauth') {
      setNote({ kind: 'ok', text: 'The address looks right. Access is checked when you sign in.' });
      return;
    }
    setBusy('test');
    try {
      const info = editing && !key
        ? await testConnection({ id: editing.id })
        : await testConnection({ name: input.name, url: input.url.trim(), auth: { type: 'api_key', key } });
      setNote({ kind: 'ok', text: `Connected to ${info.serverName || 'Ax'} ${info.serverVersion} with ${info.tools.length} tools.` });
    } catch (e) {
      fail(e);
    } finally {
      setBusy(null);
    }
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!check()) return;
    // Opened in the click itself; pointed at the sign-in page once it is known.
    const tab = !editing && input.mode === 'oauth' ? openTab() : null;
    setBusy('save');
    try {
      if (editing) {
        const patch = oauthEdit
          ? { name: input.name }
          : { name: input.name, ...(addressChanged ? { url: input.url.trim() } : {}), ...(key ? { auth: { type: 'api_key' as const, key } } : {}) };
        onSaved(await updateConnection(editing.id, patch));
      } else if (input.mode === 'api_key') {
        onSaved(await createConnection({ name: input.name, url: input.url.trim(), auth: { type: 'api_key', key } }));
      } else {
        const started = await startLogin({
          name: input.name,
          url: input.url.trim(),
          clientId: input.clientId.trim(),
          write: input.write,
          admin: input.admin,
        });
        if (tab) {
          tab.opener = null;
          tab.location.replace(started.authorizationUrl);
        }
        setSignIn({ started, blocked: tab === null });
      }
    } catch (e) {
      tab?.close();
      fail(e);
    } finally {
      setBusy(null);
    }
  };

  const usePasted = () => {
    try {
      const details = parseConnectionDetails((pasted ?? '').trim());
      set({
        url: details.projectMcpUrl,
        clientId: details.nativeClientId ?? '',
        ...(details.nativeClientId ? { mode: 'oauth' as const, key: '' } : {}),
      });
      setPasted(null);
    } catch (e) {
      fail(e);
    }
  };

  if (signIn) {
    return (
      <SignInWait
        signIn={signIn.started}
        blocked={signIn.blocked}
        onConnected={onSaved}
        onFailed={(message) => { setSignIn(null); setNote({ kind: 'err', text: message, field: null }); }}
        onCancelled={() => setSignIn(null)}
      />
    );
  }

  const noteId = `${ids}-note`;
  const invalid = (field: InputField) =>
    note?.kind === 'err' && note.field === field ? { 'aria-invalid': true, 'aria-describedby': noteId } : {};
  const placeholderName = suggestedName(input.url) || 'Production';

  return (
    <form className="admin-form" onSubmit={(e) => void submit(e)} noValidate aria-label={editing ? 'Edit connection' : 'New connection'}>
      {note?.kind === 'err' && <div id={noteId} className="settings-toast settings-toast--err" role="alert">{note.text}</div>}
      {note?.kind === 'ok' && <div className="settings-toast settings-toast--ok" role="status">{note.text}</div>}

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
          {...invalid('url')}
        />
        <span className="admin-field-hint">The project MCP address of the environment. It is the only way this application talks to it.</span>
      </div>

      {!editing && (
        <fieldset className="admin-field admin-fieldset">
          <legend className="admin-field-label">Credentials</legend>
          <div className="admin-choice">
            <label>
              <input type="radio" name={`${ids}-mode`} checked={input.mode === 'api_key'} onChange={() => set({ mode: 'api_key' })} />
              API key
            </label>
            <label>
              {/* The key is dropped when it is no longer the chosen credential. */}
              <input type="radio" name={`${ids}-mode`} checked={input.mode === 'oauth'} onChange={() => set({ mode: 'oauth', key: '' })} />
              Browser sign-in
            </label>
          </div>
        </fieldset>
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
            {...invalid('key')}
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
              {...invalid('clientId')}
            />
            <span className="admin-field-hint">The public client ID from the environment's connection details. It is not a password.</span>
          </div>
          {!editing && (
            <>
              <label className="admin-choice">
                <input type="checkbox" checked={input.write || input.admin} disabled={input.admin} onChange={(e) => set({ write: e.target.checked })} />
                Also ask for write access (adding memories and rules)
              </label>
              <label className="admin-choice">
                <input type="checkbox" checked={input.admin} onChange={(e) => set({ admin: e.target.checked })} />
                Also ask for admin access (the management pages; your account needs the admin grant)
              </label>
            </>
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
          {...invalid('name')}
        />
      </div>

      {!editing && (pasted !== null ? (
        <div className="admin-field">
          <label htmlFor={`${ids}-paste`}>Connection details copied from Ax</label>
          <textarea id={`${ids}-paste`} className="settings-input" value={pasted} onChange={(e) => setPasted(e.target.value)} spellCheck={false} autoFocus />
          <div className="admin-actions">
            <button type="button" className="btn btn-secondary" onClick={usePasted} disabled={!pasted.trim()}>Use these details</button>
            <button type="button" className="btn btn-subtle" onClick={() => setPasted(null)}>Cancel</button>
          </div>
        </div>
      ) : (
        <div className="admin-actions">
          <button type="button" className="btn btn-subtle" onClick={() => setPasted('')}>Paste connection details</button>
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
