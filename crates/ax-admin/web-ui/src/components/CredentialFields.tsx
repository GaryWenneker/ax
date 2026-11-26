import type { ConnectionInput, InputField } from '../connectionInput';

interface Props {
  /** Prefix for field ids, unique per form. */
  ids: string;
  input: ConnectionInput;
  set: (patch: Partial<ConnectionInput>) => void;
  /** Editing a saved connection: an empty key keeps the saved one. */
  editing: boolean;
  /** A saved browser sign-in: its client ID cannot be changed. */
  locked: boolean;
  /** Attributes that mark a field as the one the form's error is about. */
  invalid: (field: InputField) => { 'aria-invalid'?: boolean; 'aria-describedby'?: string };
}

/** The key field, or the fields of a browser sign-in. */
export default function CredentialFields({ ids, input, set, editing, locked, invalid }: Props) {
  return (
    <>
      {input.mode === 'api_key' ? (
        <div className="admin-field">
          <label htmlFor={`${ids}-key`}>API key</label>
          <input
            id={`${ids}-key`}
            className="settings-input settings-input--mono"
            type="password"
            autoComplete="off"
            // Not a login: ask password managers not to offer to save it.
            data-1p-ignore=""
            data-lpignore="true"
            data-bwignore="true"
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
              disabled={locked}
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
    </>
  );
}
