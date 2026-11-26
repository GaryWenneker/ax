import { useState } from 'react';
import ModalShell from '@cc/components/ModalShell';
import Codicon from '@cc/components/Codicon';
import { ApiError, activateConnection, removeConnection, testConnection, type Connection } from '../api';
import Badge from './Badge';
import ConnectionForm from './ConnectionForm';

interface Props {
  connections: Connection[];
  onClose: () => void;
  /** Called after any change, so the application reloads the list and the pages. */
  onChanged: () => Promise<void> | void;
}

type View = { kind: 'list' } | { kind: 'add' } | { kind: 'edit'; connection: Connection };

/** Saved environments: switch between them, add, edit, test and remove. */
export default function ConnectionsPanel({ connections, onClose, onChanged }: Props) {
  const [view, setView] = useState<View>({ kind: 'list' });
  const [selected, setSelected] = useState<string | null>(connections.find((c) => c.active)?.id ?? connections[0]?.id ?? null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<{ kind: 'ok' | 'err'; text: string } | null>(null);
  const [confirming, setConfirming] = useState(false);

  const current = connections.find((c) => c.id === selected) ?? null;

  const act = async (work: () => Promise<string | void>) => {
    setBusy(true);
    setNote(null);
    try {
      const text = await work();
      if (text) setNote({ kind: 'ok', text });
    } catch (e) {
      setNote({ kind: 'err', text: e instanceof ApiError || e instanceof Error ? e.message : 'Something went wrong' });
    } finally {
      setBusy(false);
    }
  };

  if (view.kind !== 'list') {
    return (
      <ModalShell
        title={view.kind === 'add' ? 'Add connection' : `Edit ${view.connection.name}`}
        subtitle="The address and credentials are checked before anything is saved."
        onClose={() => setView({ kind: 'list' })}
        size="lg"
      >
        <ConnectionForm
          editing={view.kind === 'edit' ? view.connection : undefined}
          submitLabel={view.kind === 'add' ? 'Save and connect' : 'Save'}
          onCancel={() => setView({ kind: 'list' })}
          onSaved={async (connection) => {
            await onChanged();
            setSelected(connection.id);
            setView({ kind: 'list' });
            setNote({ kind: 'ok', text: `${connection.name} is saved.` });
          }}
        />
      </ModalShell>
    );
  }

  return (
    <ModalShell
      title="Connections"
      subtitle="Each connection is one Ax environment, reached through its remote MCP address."
      onClose={onClose}
      size="lg"
      footer={
        <div className="admin-actions admin-actions--end">
          <button type="button" className="btn btn-secondary" onClick={() => setView({ kind: 'add' })}>
            <Codicon name="add" /> Add connection
          </button>
          <button type="button" className="btn btn-subtle" onClick={onClose}>Close</button>
        </div>
      }
    >
      {note && (
        <div className={`settings-toast settings-toast--${note.kind}`} role={note.kind === 'err' ? 'alert' : 'status'}>{note.text}</div>
      )}
      <div className="admin-list" role="listbox" aria-label="Saved connections">
        {connections.map((connection) => (
          <button
            key={connection.id}
            type="button"
            role="option"
            aria-selected={connection.id === selected}
            className={`admin-row${connection.id === selected ? ' admin-row--selected' : ''}`}
            onClick={() => { setSelected(connection.id); setConfirming(false); setNote(null); }}
            onDoubleClick={() => void act(async () => { await activateConnection(connection.id); await onChanged(); })}
          >
            <Codicon name="server-environment" />
            <span className="admin-row-main">
              <span className="admin-row-title">{connection.name}</span>
              <span className="admin-row-sub">{connection.url}</span>
            </span>
            <span className="admin-row-badges">
              {connection.active && <Badge icon="play" label="active" tone="ok" />}
              <Badge icon={connection.auth === 'oauth' ? 'person' : 'key'} label={connection.auth === 'oauth' ? 'sign-in' : 'API key'} />
            </span>
          </button>
        ))}
      </div>
      {current && (
        <div className="admin-form">
          {confirming ? (
            <div className="admin-actions" role="alertdialog" aria-label="Remove connection">
              <span>Remove {current.name}? Its saved credential is deleted from this computer. The environment itself is not changed.</span>
              <button
                type="button"
                className="btn btn-danger-subtle"
                disabled={busy}
                onClick={() => void act(async () => {
                  await removeConnection(current.id);
                  setConfirming(false);
                  setSelected(null);
                  await onChanged();
                  return `${current.name} is removed.`;
                })}
              >
                Remove
              </button>
              <button type="button" className="btn btn-subtle" onClick={() => setConfirming(false)}>Keep</button>
            </div>
          ) : (
            <div className="admin-actions">
              <button
                type="button"
                className="btn"
                disabled={busy || current.active}
                onClick={() => void act(async () => {
                  await activateConnection(current.id);
                  await onChanged();
                  return `Switched to ${current.name}.`;
                })}
              >
                {current.active ? 'In use' : 'Switch to this'}
              </button>
              <button
                type="button"
                className="btn btn-secondary"
                disabled={busy}
                onClick={() => void act(async () => {
                  const { info } = await testConnection({ id: current.id });
                  return `${current.name} answers: ${info.serverName || 'Ax'} ${info.serverVersion}, ${info.tools.length} tools.`;
                })}
              >
                Test
              </button>
              <button type="button" className="btn btn-secondary" disabled={busy} onClick={() => setView({ kind: 'edit', connection: current })}>Edit</button>
              <button type="button" className="btn btn-subtle btn-subtle--remove" disabled={busy} onClick={() => setConfirming(true)}>Remove</button>
            </div>
          )}
        </div>
      )}
    </ModalShell>
  );
}
