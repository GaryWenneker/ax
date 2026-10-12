import { accessJson } from '../lib/accessApi';
import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react';
import Codicon from './Codicon';
import ReplicationSetup from './ReplicationSetup';
import ModalShell from './ModalShell';

type Role = 'owner' | 'admin' | 'member' | 'readonly';
type Member = { id: string; display_name: string; email?: string | null; role: Role; status: string; created_at: string; last_active_at?: string | null };
type Client = { id: string; member_id: string; display_name: string; client_type: string; scopes: string[]; status: string; created_at: string; last_seen_at?: string | null };
type Credential = { id: string; client_id: string; token_prefix: string; created_at: string; last_used_at?: string | null; revoked_at?: string | null };
type Node = { id: string; display_name: string; role: string; endpoint?: string | null; trust_state: string; last_seen_at?: string | null; last_sync_at?: string | null };
type LocalNode = { node_id: string; role: string; primary_url?: string; last_sync_at?: string; sync_state: string };
type IssuedToken = { value: string; clientName: string };
type Confirmation = { title: string; description: string; label: string; action: () => Promise<void> };

const ROLES: { id: Role; label: string; description: string }[] = [
  { id: 'owner', label: 'Owner', description: 'Full control, including ownership.' },
  { id: 'admin', label: 'Admin', description: 'Manage people, clients and nodes.' },
  { id: 'member', label: 'Member', description: 'Use authorized clients to read and write.' },
  { id: 'readonly', label: 'Read-only', description: 'Query context and memory without changing data.' },
];
const SCOPES = [
  { id: 'memory:read', label: 'Read memory', description: 'Recall project knowledge.' },
  { id: 'memory:write', label: 'Write memory', description: 'Save decisions and conventions.' },
  { id: 'preflight:read', label: 'Read preflight', description: 'Load applicable rules and skills.' },
  { id: 'context:read', label: 'Read context', description: 'Query indexed code and context.' },
  { id: 'share:read', label: 'Read shared data', description: 'Read shared project data.' },
  { id: 'share:write', label: 'Write shared data', description: 'Change shared project data.' },
  { id: 'admin:clients', label: 'Manage clients', description: 'Manage applications and credentials.' },
  { id: 'admin:team', label: 'Manage team', description: 'Manage members and their roles.' },
  { id: 'admin:nodes', label: 'Manage nodes', description: 'Manage replication access.' },
];
const DEFAULT_SCOPES = ['memory:read', 'memory:write', 'preflight:read', 'context:read'];
const CLIENT_TYPES = [{ id: 'chatgpt', label: 'ChatGPT' }, { id: 'cursor', label: 'Cursor' }, { id: 'cline', label: 'Cline' }, { id: 'cli', label: 'CLI' }, { id: 'custom', label: 'Custom' }];

function permits(role: Role | undefined, scope: string) {
  if (!role) return false;
  return (role === 'owner' || role === 'admin' || !scope.startsWith('admin:'))
    && (role !== 'readonly' || !scope.endsWith(':write'));
}

function timestamp(value?: string | null) {
  if (!value) return null;
  // SQLite uses UTC without a suffix; API ISO timestamps may already include an offset.
  const normalized = value.replace(' ', 'T');
  const date = new Date(/(?:Z|[+-]\d{2}:?\d{2})$/i.test(normalized) ? normalized : `${normalized}Z`);
  return Number.isNaN(date.getTime()) ? null : date;
}

function DateLabel({ value, empty = 'Never' }: { value?: string | null; empty?: string }) {
  const date = timestamp(value);
  return date ? <time dateTime={date.toISOString()}>{date.toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })}</time> : <>{empty}</>;
}

function Status({ label, good = false }: { label: string; good?: boolean }) {
  return <span className={`access-status${good ? ' access-status--good' : ''}`}><span aria-hidden="true" />{label}</span>;
}


export default function AccessSettingsSection({ section }: { section: string }) {
  const active = ['team', 'clients', 'nodes'].includes(section);
  const [members, setMembers] = useState<Member[]>([]);
  const [clients, setClients] = useState<Client[]>([]);
  const [credentials, setCredentials] = useState<Credential[]>([]);
  const [nodes, setNodes] = useState<Node[]>([]);
  const [scopeDrafts, setScopeDrafts] = useState<Record<string, string[]>>({});
  const [loading, setLoading] = useState(true);
  const [readonly, setReadonly] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [issuedToken, setIssuedToken] = useState<IssuedToken | null>(null);
  const [copied, setCopied] = useState(false);
  const [form, setForm] = useState<'member' | 'client' | null>(null);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const [memberName, setMemberName] = useState('');
  const [memberEmail, setMemberEmail] = useState('');
  const [memberRole, setMemberRole] = useState<Role>('member');
  const [clientName, setClientName] = useState('');
  const [clientType, setClientType] = useState('custom');
  const [clientMember, setClientMember] = useState('');
  const requestVersion = useRef(0);
  const mutationRunning = useRef(false);
  const currentSection = useRef(section);
  currentSection.current = section;

  const refresh = useCallback(async (target: string, initial = false) => {
    const version = ++requestVersion.current;
    if (initial) setLoading(true);
    try {
      const status = await accessJson<{ readonly: boolean }>('/api/share/status');
      if (version !== requestVersion.current) return;
      setReadonly(status.readonly !== false);
      if (target === 'team' || target === 'clients') {
        const team = await accessJson<{ members: Member[] }>('/api/access/team');
        if (version !== requestVersion.current) return;
        setMembers(team.members);
        setClientMember(current => team.members.some(member => member.id === current && member.status === 'active')
          ? current : team.members.find(member => member.status === 'active')?.id ?? '');
      }
      if (target === 'clients') {
        const data = await accessJson<{ clients: Client[]; credentials: Credential[] }>('/api/access/clients');
        if (version !== requestVersion.current) return;
        setClients(data.clients);
        setCredentials(data.credentials ?? []);
      }
      if (target === 'nodes') {
        const data = await accessJson<{ nodes: Node[]; local?: LocalNode }>('/api/access/nodes');
        if (version !== requestVersion.current) return;
        setNodes(data.nodes);
      }
      setError(null);
    } catch (e) {
      if (version === requestVersion.current) setError(e instanceof Error ? e.message : String(e));
    } finally {
      if (version === requestVersion.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    setIssuedToken(null);
    setCopied(false);
    setForm(null);
    setConfirmation(null);
    setMessage(null);
    setError(null);
    if (!active) return;
    void refresh(section, true);
    const interval = window.setInterval(() => {
      if (!mutationRunning.current) void refresh(section);
    }, section === 'nodes' ? 5000 : 30000);
    return () => { window.clearInterval(interval); requestVersion.current++; };
  }, [active, section, refresh]);

  async function mutate(url: string, method: string, body?: unknown, success?: string, tokenClient?: string) {
    if (mutationRunning.current || readonly || loading) return false;
    mutationRunning.current = true;
    requestVersion.current++;
    const target = section;
    setBusy(true);
    setError(null);
    setMessage(null);
    setIssuedToken(null);
    setCopied(false);
    try {
      const data = await accessJson<{ credential?: { token: string } }>(url, { method, ...(body ? { body: JSON.stringify(body) } : {}) });
      if (target === currentSection.current) {
        if (data.credential && tokenClient) setIssuedToken({ value: data.credential.token, clientName: tokenClient });
        if (success) setMessage(success);
        await refresh(target);
      }
      return true;
    } catch (e) {
      if (target === currentSection.current) setError(e instanceof Error ? e.message : String(e));
      return false;
    } finally {
      mutationRunning.current = false;
      setBusy(false);
    }
  }

  async function addMember(event: FormEvent) {
    event.preventDefault();
    if (!memberName.trim()) return;
    if (await mutate('/api/access/team', 'POST', { display_name: memberName.trim(), email: memberEmail.trim() || null, role: memberRole }, 'Team member added.')) {
      setMemberName(''); setMemberEmail(''); setForm(null);
    }
  }

  async function addClient(event: FormEvent) {
    event.preventDefault();
    const member = members.find(item => item.id === clientMember && item.status === 'active');
    if (!clientName.trim() || !member) return;
    if (await mutate('/api/access/clients', 'POST', {
      member_id: member.id, display_name: clientName.trim(), client_type: clientType,
      scopes: DEFAULT_SCOPES.filter(scope => permits(member.role, scope)),
    }, 'Client added. Issue a token to connect it.')) { setClientName(''); setForm(null); }
  }

  async function saveScopes(client: Client, role?: Role) {
    const scopes = (scopeDrafts[client.id] ?? client.scopes).filter(scope => permits(role, scope));
    if (await mutate(`/api/access/clients/${client.id}/scopes`, 'PATCH', { scopes }, `Permissions saved for ${client.display_name}.`)) {
      setScopeDrafts(drafts => { const next = { ...drafts }; delete next[client.id]; return next; });
    }
  }

  const locked = busy || readonly || loading;
  const activeOwners = members.filter(member => member.role === 'owner' && member.status === 'active').length;
  const activeMembers = members.filter(member => member.status === 'active');
  const title = section === 'team' ? 'Team' : section === 'clients' ? 'Clients & tokens' : 'Replication';
  const description = section === 'team' ? 'People and roles for this Ax environment.'
    : section === 'clients' ? 'Applications, their permissions and independently revocable credentials.'
    : 'Your local node, its configured Primary and connected replicas.';

  if (!active) return null;

  return <>
    <section id={`settings-panel-${section}`} className="settings-card access-settings">
      <div className="settings-card-header access-header">
        <div><h2>{title}</h2><p>{description}</p></div>
        <div className="access-actions">
          <button className="btn" type="button" disabled={busy || loading} onClick={() => void refresh(section, true)}><Codicon name="refresh" />Refresh</button>
          {section !== 'nodes' && <button className="btn primary" type="button" disabled={locked || (section === 'clients' && !activeMembers.length)} onClick={() => setForm(section === 'team' ? 'member' : 'client')}>
            <Codicon name="add" />{section === 'team' ? 'Add member' : 'Add client'}
          </button>}
        </div>
      </div>
      <div className="settings-card-body">
        {error && <div role="alert" className="settings-toast settings-toast--err">{error}</div>}
        {message && <div role="status" className="settings-toast settings-toast--ok">{message}</div>}
        {loading && <p role="status" className="access-note">Loading {title.toLowerCase()}…</p>}
        {!loading && readonly && <p className="access-note">This session is read-only. Open the local Command Center to manage access.</p>}
        {section === 'clients' && issuedToken && <div className="access-token" role="status">
          <div><strong>New token for {issuedToken.clientName}</strong><p>Copy it now. Ax cannot display this credential again.</p></div>
          <code>{issuedToken.value}</code>
          <div className="access-actions">
            <button type="button" className="btn" onClick={async () => {
              try { await navigator.clipboard.writeText(issuedToken.value); setCopied(true); }
              catch { setError('Copy failed. Select the token and copy it manually.'); }
            }}><Codicon name="copy" />{copied ? 'Copied' : 'Copy token'}</button>
            <button type="button" className="btn" onClick={() => setIssuedToken(null)}>Dismiss</button>
          </div>
        </div>}

        {!loading && section === 'team' && <>
          <p className="access-note">{activeMembers.length} active · {members.length} total. A client can never exceed its member’s role.</p>
          {!members.length && !error && <p className="access-empty">No members found. Add a member to assign application access.</p>}
          {members.map(member => {
            const lastOwner = member.role === 'owner' && member.status === 'active' && activeOwners === 1;
            return <div className="settings-row" key={member.id}>
              <div className="settings-row-label">
                <span className="settings-row-title">{member.display_name} <Status label={member.status === 'active' ? 'Active' : 'Disabled'} good={member.status === 'active'} /></span>
                {member.email && <span className="settings-row-desc">{member.email}</span>}
                <span className="settings-row-desc">Joined <DateLabel value={member.created_at} /> · Last active <DateLabel value={member.last_active_at} /></span>
                {lastOwner && <span className="settings-row-desc">The last active Owner must remain enabled.</span>}
              </div>
              <div className="settings-row-control access-actions">
                <select aria-label={`Role for ${member.display_name}`} className="settings-select" value={member.role} disabled={locked || lastOwner}
                  onChange={event => void mutate(`/api/access/team/${member.id}`, 'PATCH', { role: event.target.value, status: member.status }, `Role updated for ${member.display_name}.`)}>
                  {ROLES.map(role => <option key={role.id} value={role.id}>{role.label}</option>)}
                </select>
                <button type="button" className="btn" disabled={locked || lastOwner} onClick={() => {
                  if (member.status !== 'active') { void mutate(`/api/access/team/${member.id}`, 'PATCH', { role: member.role, status: 'active' }, `${member.display_name} enabled.`); return; }
                  setConfirmation({ title: `Disable ${member.display_name}?`, description: 'Their clients will lose access until this member is enabled again.', label: 'Disable member', action: async () => { await mutate(`/api/access/team/${member.id}`, 'PATCH', { role: member.role, status: 'disabled' }, `${member.display_name} disabled.`); } });
                }}>{member.status === 'active' ? 'Disable' : 'Enable'}</button>
              </div>
            </div>;
          })}
          <dl className="access-role-guide">{ROLES.map(role => <div key={role.id}><dt>{role.label}</dt><dd>{role.description}</dd></div>)}</dl>
        </>}

        {!loading && section === 'clients' && <>
          <p className="access-note">{clients.filter(client => client.status === 'active').length} active clients. Tokens authenticate an application; the member’s role limits its permissions.</p>
          {!clients.length && !error && <div className="access-empty"><strong>No clients yet</strong><p>Add an application, then issue its first token.</p></div>}
          {clients.map(client => {
            const member = members.find(item => item.id === client.member_id);
            const usable = client.status === 'active' && member?.status === 'active';
            const draft = scopeDrafts[client.id] ?? client.scopes;
            const effective = draft.filter(scope => permits(member?.role, scope));
            const tokens = credentials.filter(credential => credential.client_id === client.id);
            return <article className="access-client" key={client.id} aria-label={client.display_name}>
              <div className="access-client-header">
                <div><h3>{client.display_name}</h3><p>{client.client_type} · {member?.display_name ?? client.member_id}</p></div>
                <Status label={client.status === 'revoked' ? 'Revoked' : usable ? 'Active' : 'Member disabled'} good={!!usable} />
              </div>
              <dl className="access-metadata"><div><dt>Created</dt><dd><DateLabel value={client.created_at} /></dd></div><div><dt>Last used</dt><dd><DateLabel value={client.last_seen_at} /></dd></div><div><dt>Active tokens</dt><dd>{tokens.filter(token => !token.revoked_at).length}</dd></div></dl>
              {client.status === 'active' && <>
                <div className="access-subheader"><h4>Permissions</h4><span>{effective.length} allowed by role</span></div>
                <div className="access-scopes">{SCOPES.map(scope => <label key={scope.id} className="access-scope">
                  <input type="checkbox" aria-label={scope.id} checked={effective.includes(scope.id)} disabled={locked || !usable || !permits(member?.role, scope.id)}
                    onChange={event => setScopeDrafts(drafts => ({ ...drafts, [client.id]: event.target.checked ? [...effective, scope.id] : effective.filter(value => value !== scope.id) }))} />
                  <span><strong>{scope.label}</strong><small>{scope.description}</small><code>{scope.id}</code></span>
                </label>)}</div>
                <div className="access-actions"><button type="button" className="btn" disabled={locked || !usable || (!scopeDrafts[client.id] && effective.length === client.scopes.length)} onClick={() => void saveScopes(client, member?.role)}>Save scopes</button>
                  {scopeDrafts[client.id] && <button type="button" className="btn" disabled={busy} onClick={() => setScopeDrafts(drafts => { const next = { ...drafts }; delete next[client.id]; return next; })}>Discard changes</button>}
                </div>
              </>}
              <div className="access-subheader"><h4>Tokens</h4><span>Only prefixes are stored in this view</span></div>
              {!tokens.length && <p className="access-note">No tokens issued.</p>}
              {tokens.map(token => <div className="access-credential" key={token.id}>
                <div><code>{token.token_prefix}…</code> <Status label={token.revoked_at ? 'Revoked' : 'Active'} good={!token.revoked_at} />
                  <p>Created <DateLabel value={token.created_at} /> · Last used <DateLabel value={token.last_used_at} /></p>
                </div>
                {!token.revoked_at && <button type="button" className="btn" disabled={locked} onClick={() => setConfirmation({ title: 'Revoke this token?', description: `Applications using ${token.token_prefix}… will immediately lose access. Other tokens remain active.`, label: 'Revoke token', action: async () => { await mutate(`/api/access/credentials/${token.id}/revoke`, 'POST', undefined, 'Token revoked.'); } })}>Revoke token</button>}
              </div>)}
              {client.status === 'active' && <div className="access-actions access-client-footer">
                <button type="button" className="btn" disabled={locked || !usable} onClick={() => void mutate(`/api/access/clients/${client.id}/token`, 'POST', undefined, undefined, client.display_name)}>Issue token</button>
                <button type="button" className="btn" disabled={locked || !usable || !tokens.some(token => !token.revoked_at)} onClick={() => setConfirmation({ title: `Rotate tokens for ${client.display_name}?`, description: 'All existing tokens for this client stop working immediately. Copy the new token and update the application.', label: 'Rotate tokens', action: async () => { await mutate(`/api/access/clients/${client.id}/rotate`, 'POST', undefined, undefined, client.display_name); } })}>Rotate tokens</button>
                <button type="button" className="btn danger" disabled={locked} onClick={() => setConfirmation({ title: `Revoke ${client.display_name}?`, description: 'This client and all its tokens will lose access. The member’s other clients remain active.', label: 'Revoke client', action: async () => { await mutate(`/api/access/clients/${client.id}/revoke`, 'POST', undefined, 'Client revoked.'); } })}>Revoke client</button>
              </div>}
            </article>;
          })}
        </>}

        {!loading && section === 'nodes' && <>
          <ReplicationSetup locked={locked} onChanged={() => refresh('nodes')} />
          <p className="access-note">An authenticated Replica registers with its configured Primary automatically. Replication is separate from policy sharing and export.</p>
          {!nodes.length && !error && <div className="access-empty"><strong>No registered nodes</strong><p>Configure a Replica’s Primary URL and project credential to connect it.</p></div>}
          {nodes.map(node => {
            const seen = timestamp(node.last_seen_at);
            const online = node.trust_state === 'trusted' && !!seen && Date.now() - seen.getTime() < 120000;
            return <article className="access-client" key={node.id}>
              <div className="access-client-header"><div><h3>{node.display_name}</h3><p>{node.role === 'primary' ? 'Primary' : 'Replica'} · {node.trust_state}</p></div><Status label={node.trust_state === 'revoked' ? 'Blocked' : online ? 'Online' : 'Not seen recently'} good={online} /></div>
              <dl className="access-metadata"><div><dt>Node identity</dt><dd><code>{node.id}</code></dd></div><div><dt>Endpoint</dt><dd>{node.endpoint ?? 'Not reported'}</dd></div><div><dt>Last seen</dt><dd><DateLabel value={node.last_seen_at} /></dd></div><div><dt>Last successful sync</dt><dd><DateLabel value={node.last_sync_at} /></dd></div></dl>
              <div className="access-actions">
                {node.trust_state === 'revoked' && <button type="button" className="btn" disabled={locked} onClick={() => void mutate(`/api/access/nodes/${node.id}/reenroll`, 'POST', undefined, 'Re-enrollment allowed. The node must authenticate again.')}>Allow re-enrollment</button>}
                {node.trust_state === 'trusted' && <button type="button" className="btn danger" disabled={locked} onClick={() => setConfirmation({ title: `Block ${node.display_name}?`, description: 'This node will lose replication access. Automatic registration cannot undo the block.', label: 'Block node', action: async () => { await mutate(`/api/access/nodes/${node.id}/revoke`, 'POST', undefined, 'Node blocked.'); } })}>Block node</button>}
              </div>
            </article>;
          })}
        </>}
      </div>
    </section>

    {form && <ModalShell title={form === 'member' ? 'Add team member' : 'Add client'} onClose={() => !busy && setForm(null)}>
      <form className="access-form" onSubmit={form === 'member' ? addMember : addClient}>
        {error && <p role="alert" className="settings-toast settings-toast--err">{error}</p>}
        {form === 'member' ? <>
          <label>Name<input autoFocus required className="settings-input" value={memberName} onChange={event => setMemberName(event.target.value)} disabled={locked} /></label>
          <label>Email (optional)<input type="email" className="settings-input" value={memberEmail} onChange={event => setMemberEmail(event.target.value)} disabled={locked} /></label>
          <label>Role<select aria-label="Role" className="settings-select" value={memberRole} onChange={event => setMemberRole(event.target.value as Role)} disabled={locked}>{ROLES.map(role => <option key={role.id} value={role.id}>{role.label}</option>)}</select></label>
          <p className="access-note">{ROLES.find(role => role.id === memberRole)?.description} Clients are added separately.</p>
        </> : <>
          <label>Client name<input autoFocus required className="settings-input" value={clientName} onChange={event => setClientName(event.target.value)} disabled={locked} /></label>
          <label>Application type<select aria-label="Application type" className="settings-select" value={clientType} onChange={event => setClientType(event.target.value)} disabled={locked}>{CLIENT_TYPES.map(type => <option key={type.id} value={type.id}>{type.label}</option>)}</select></label>
          <label>Team member<select aria-label="Team member" required className="settings-select" value={clientMember} onChange={event => setClientMember(event.target.value)} disabled={locked}>{activeMembers.map(member => <option key={member.id} value={member.id}>{member.display_name}</option>)}</select></label>
          <p className="access-note">Starts with context, preflight and memory access allowed by this member’s role. Edit permissions and issue a token after creation.</p>
        </>}
        <div className="access-actions"><button type="button" className="btn" disabled={busy} onClick={() => setForm(null)}>Cancel</button><button type="submit" className="btn primary" disabled={locked}>{busy ? 'Saving…' : form === 'member' ? 'Add member' : 'Add client'}</button></div>
      </form>
    </ModalShell>}
    {confirmation && <ModalShell title={confirmation.title} onClose={() => !busy && setConfirmation(null)}>
      <p>{confirmation.description}</p><div className="access-actions access-confirm-actions"><button type="button" className="btn" disabled={busy} onClick={() => setConfirmation(null)}>Cancel</button><button type="button" className="btn danger" disabled={locked} onClick={async () => { await confirmation.action(); setConfirmation(null); }}>{busy ? 'Saving…' : confirmation.label}</button></div>
    </ModalShell>}
  </>;
}
