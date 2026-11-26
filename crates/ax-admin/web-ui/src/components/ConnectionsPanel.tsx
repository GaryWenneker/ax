import { useState } from 'react';
import ModalShell from '@cc/components/ModalShell';
import Codicon from '@cc/components/Codicon';
import { activateConnection, removeConnection, testConnection, type Connection } from '../api';
import Badge from './Badge';
import ConfirmDialog from './ConfirmDialog';
import ConnectionForm from './ConnectionForm';
import OptionList from './OptionList';

interface Props {
  connections: Connection[];
  onClose: () => void;
  /** Called after any change, so the application reloads the list and the pages. */
  onChanged: () => Promise<void>;
}

type View = { kind: 'list' } | { kind: 'add' } | { kind: 'edit'; connection: Connection } | { kind: 'remove'; connection: Connection };

/** Saved environments: switch between them, add, edit, test and remove. */
export default function ConnectionsPanel({ connections, onClose, onChanged }: Props) {
  const [view, setView] = useState<View>({ kind: 'list' });
  const [selected, setSelected] = useState<string | null>(connections.find((c) => c.active)?.id ?? connections[0]?.id ?? null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<{ kind: 'ok' | 'err'; text: string } | null>(null);

  const current = connections.find((c) => c.id === selected) ?? null;

  const act = async (work: () => Promise<string>) => {
    if (busy) return;
    setBusy(true);
    setNote(null);
    try {
      setNote({ kind: 'ok', text: await work() });
    } catch (e) {
      setNote({ kind: 'err', text: e instanceof Error ? e.message : 'Something went wrong' });
    } finally {
      setBusy(false);
    }
  };

  const switchTo = (connection: Connection) => act(async () => {
    await activateConnection(connection.id);
    await onChanged();
    return `Switched to ${connection.name}.`;
  });

  if (view.kind === 'add' || view.kind === 'edit') {
    return (
      <ModalShell
        title={view.kind === 'add' ? 'Add connection' : `Edit ${view.connection.name}`}
        subtitle="The address and credentials are checked before anything is saved."
        onClose={() => setView({ kind: 'list' })}
        size="lg"
      >
        <ConnectionForm
          {...(view.kind === 'edit' ? { editing: view.connection } : {})}
          submitLabel={view.kind === 'add' ? 'Save and connect' : 'Save'}
          onCancel={() => setView({ kind: 'list' })}
          onSaved={(connection) => {
            setSelected(connection.id);
            setView({ kind: 'list' });
            setNote({ kind: 'ok', text: `${connection.name} is saved.` });
            void onChanged();
          }}
        />
      </ModalShell>
    );
  }

  if (view.kind === 'remove') {
    const target = view.connection;
    return (
      <ConfirmDialog
        title={`Remove ${target.name}?`}
        message="Its saved credential is deleted from this computer. The environment itself is not changed."
        confirmLabel="Remove"
        onCancel={() => setView({ kind: 'list' })}
        onConfirm={() => {
          setView({ kind: 'list' });
          void act(async () => {
            await removeConnection(target.id);
            setSelected(null);
            await onChanged();
            return `${target.name} is removed.`;
          });
        }}
      />
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
          <button type="button" className="btn btn-subtle" onClick={onClose}>Done</button>
        </div>
      }
    >
      {note && (
        <div className={`settings-toast settings-toast--${note.kind}`} role={note.kind === 'err' ? 'alert' : 'status'}>{note.text}</div>
      )}
      <OptionList
        label="Saved connections"
        items={connections}
        keyOf={(connection) => connection.id}
        selected={selected}
        onSelect={(id) => { setSelected(id); setNote(null); }}
        onActivate={(id) => {
          const connection = connections.find((c) => c.id === id);
          if (connection && !connection.active) void switchTo(connection);
        }}
      >
        {(connection) => (
          <>
            <Codicon name="server-environment" />
            <span className="admin-row-main">
              <span className="admin-row-title">{connection.name}</span>
              <span className="admin-row-sub">{connection.url}</span>
            </span>
            <span className="admin-row-badges">
              {connection.active && <Badge icon="play" label="active" tone="ok" />}
              <Badge icon={connection.auth === 'oauth' ? 'person' : 'key'} label={connection.auth === 'oauth' ? 'sign-in' : 'API key'} />
            </span>
          </>
        )}
      </OptionList>
      {current && (
        <div className="admin-form">
          <div className="admin-actions">
            <button type="button" className="btn" disabled={busy || current.active} onClick={() => void switchTo(current)}>
              {current.active ? 'In use' : 'Switch to this'}
            </button>
            <button
              type="button"
              className="btn btn-secondary"
              disabled={busy}
              onClick={() => void act(async () => {
                const info = await testConnection({ id: current.id });
                return `${current.name} answers: ${info.serverName || 'Ax'} ${info.serverVersion}, ${info.tools.length} tools.`;
              })}
            >
              Test
            </button>
            <button type="button" className="btn btn-secondary" disabled={busy} onClick={() => setView({ kind: 'edit', connection: current })}>Edit</button>
            <button type="button" className="btn btn-subtle btn-subtle--remove" disabled={busy} onClick={() => setView({ kind: 'remove', connection: current })}>Remove</button>
          </div>
        </div>
      )}
    </ModalShell>
  );
}
