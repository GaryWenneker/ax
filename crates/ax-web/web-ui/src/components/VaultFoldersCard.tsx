import { useEffect, useId, useState } from 'react';
import ModalShell from './ModalShell';
import {
  fetchVaultFolders,
  folderNameFromPath,
  folderSyncSummary,
  pickFolderPath,
  removeVaultFolder,
  saveVaultFolders,
  syncVaultFolder,
  validMountName,
  type VaultFolder,
} from '../vaultMount';

type Entry = Pick<VaultFolder, 'name' | 'path' | 'index'>;

const entries = (list: VaultFolder[]): Entry[] => list.map(({ name, path, index }) => ({ name, path, index }));

/** Settings → Vault connection: extra directories under `folders/` in the vault drive. */
export default function VaultFoldersCard() {
  const [folders, setFolders] = useState<VaultFolder[]>([]);
  const [readonly, setReadonly] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);

  useEffect(() => {
    fetchVaultFolders()
      .then((r) => {
        setFolders(r.folders);
        setReadonly(r.readonly);
      })
      .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)));
  }, []);

  async function run(key: string, action: () => Promise<VaultFolder[] | void>) {
    setBusy(key);
    setError(null);
    try {
      const next = await action();
      if (next) setFolders(next);
      else setFolders((await fetchVaultFolders()).folders);
      return true;
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      return false;
    } finally {
      setBusy(null);
    }
  }

  const save = (list: Entry[]) => saveVaultFolders(list).then((r) => r.folders);

  return (
    <>
      <div className="settings-row">
        <div className="settings-row-label">
          <span className="settings-row-title">Folders</span>
          <span className="settings-row-desc">
            Extra directories shown in the drive under <span className="mono">folders/</span>. With indexing on, their
            Markdown and text files are imported as memories you can find with recall.
          </span>
        </div>
        <div className="settings-row-control settings-row-control--actions">
          <button type="button" className="btn btn-subtle" disabled={readonly || !!busy} onClick={() => setAdding(true)}>
            Add folder
          </button>
        </div>
      </div>
      {error && !adding && (
        <p className="settings-row-desc vault-folders-error" role="alert">
          {error}
        </p>
      )}
      {folders.length > 0 && (
        <ul className="vault-folders-list" aria-label="Vault folders">
          {folders.map((f) => (
            <li key={f.name} className="vault-folder-row" data-name={f.name}>
              <div className="vault-folder-text">
                <span className="settings-row-title">{f.name}</span>
                <span className="settings-row-desc mono" title={f.path}>
                  {f.path}
                </span>
                <span className="settings-row-desc">{folderSyncSummary(f)}</span>
              </div>
              <div className="vault-folder-actions">
                <label className="vault-folder-index">
                  <span className="muted">Index into memory</span>
                  <button
                    type="button"
                    role="switch"
                    aria-checked={f.index}
                    aria-label={`Index ${f.name} into memory`}
                    className={`settings-toggle${f.index ? ' on' : ''}`}
                    disabled={readonly || !!busy}
                    onClick={() =>
                      void run(f.name, () =>
                        save(entries(folders).map((e) => (e.name === f.name ? { ...e, index: !e.index } : e))),
                      )
                    }
                  >
                    <span className="settings-toggle-thumb" />
                  </button>
                </label>
                <button
                  type="button"
                  className="btn btn-subtle"
                  disabled={readonly || !f.index || !!busy}
                  onClick={() => void run(f.name, () => syncVaultFolder(f.name).then(() => undefined))}
                >
                  {busy === f.name ? 'Syncing…' : 'Sync now'}
                </button>
                <button
                  type="button"
                  className="btn btn-subtle"
                  aria-label={`Remove ${f.name}`}
                  disabled={readonly || !!busy}
                  onClick={() => void run(f.name, () => removeVaultFolder(f.name).then((r) => r.folders))}
                >
                  Remove
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}
      {adding && (
        <AddFolderModal
          taken={folders.map((f) => f.name.toLowerCase())}
          onClose={() => setAdding(false)}
          onAdd={async (entry) => {
            const ok = await run('add', () => save([...entries(folders), entry]));
            if (ok) setAdding(false);
          }}
          busy={busy === 'add'}
          error={busy === null && adding ? error : null}
        />
      )}
    </>
  );
}

function AddFolderModal({
  taken,
  onClose,
  onAdd,
  busy,
  error,
}: {
  taken: string[];
  onClose: () => void;
  onAdd: (entry: Entry) => Promise<void>;
  busy: boolean;
  error: string | null;
}) {
  const [name, setName] = useState('');
  const [path, setPath] = useState('');
  const [index, setIndex] = useState(true);
  const ids = useId();
  const nameHint = `${ids}-name`;
  const pathHint = `${ids}-path`;
  const errorId = `${ids}-error`;
  const [picking, setPicking] = useState(false);
  const [pickNote, setPickNote] = useState<string | null>(null);
  const pickNoteId = `${ids}-pick`;
  const shown = error ?? pickNote;
  const describe = (hint: string) => (shown ? `${hint} ${error ? errorId : pickNoteId}` : hint);

  async function choose() {
    setPicking(true);
    setPickNote(null);
    try {
      const picked = await pickFolderPath();
      if (picked === undefined) {
        setPickNote('No folder dialog on this system — type the path.');
      } else if (picked) {
        setPath(picked);
        if (!name.trim()) setName(folderNameFromPath(picked));
      }
    } catch (e) {
      setPickNote(e instanceof Error ? e.message : String(e));
    } finally {
      setPicking(false);
    }
  }
  const nameOk = validMountName(name.trim()) && !taken.includes(name.trim().toLowerCase());
  const pathOk = path.trim().startsWith('/') || /^[A-Za-z]:[\\/]/.test(path.trim());

  return (
    <ModalShell
      title="Add folder"
      subtitle="Shown in the vault drive under folders/"
      ariaLabel="Add vault folder"
      onClose={onClose}
      footer={
        <>
          <button type="button" className="btn btn-subtle" onClick={onClose} disabled={busy}>
            Cancel
          </button>
          <button
            type="button"
            className="btn primary"
            disabled={busy || !nameOk || !pathOk}
            onClick={() => void onAdd({ name: name.trim(), path: path.trim(), index })}
          >
            {busy ? 'Adding…' : 'Add'}
          </button>
        </>
      }
    >
      <div className="vault-folder-form">
        <label className="settings-row-label">
          <span className="settings-row-title">Name</span>
          <input
            className="settings-input"
            value={name}
            aria-label="Folder name"
            aria-invalid={name !== '' && !nameOk}
            aria-describedby={describe(nameHint)}
            placeholder="notes"
            onChange={(e) => setName(e.target.value)}
          />
          <span id={nameHint} className="settings-row-desc">1–32 letters, digits, spaces, "_" or "-". Must be unique.</span>
        </label>
        <label className="settings-row-label">
          <span className="settings-row-title">Path</span>
          <span className="vault-folder-path-row">
            <input
              className="settings-input mono"
              value={path}
              aria-label="Folder path"
              aria-invalid={path !== '' && !pathOk}
              aria-describedby={describe(pathHint)}
              placeholder="/Users/me/notes"
              onChange={(e) => setPath(e.target.value)}
            />
            <button type="button" className="btn btn-subtle" disabled={busy || picking} onClick={() => void choose()}>
              {picking ? 'Choosing…' : 'Choose…'}
            </button>
          </span>
          <span id={pathHint} className="settings-row-desc">An absolute path to an existing directory.</span>
        </label>
        <label className="vault-folder-index">
          <input type="checkbox" checked={index} onChange={(e) => setIndex(e.target.checked)} />
          <span>Index its Markdown and text files into memory</span>
        </label>
        {error && (
          <p id={errorId} className="vault-folders-error" role="alert">
            {error}
          </p>
        )}
        {!error && pickNote && (
          <p id={pickNoteId} className="vault-folders-error" role="alert">
            {pickNote}
          </p>
        )}
      </div>
    </ModalShell>
  );
}
