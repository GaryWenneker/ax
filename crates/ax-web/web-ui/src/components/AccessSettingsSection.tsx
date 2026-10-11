import { useEffect, useState } from 'react';

type Member = {
  id: string;
  display_name: string;
  email?: string | null;
  role: string;
  status: string;
  created_at: string;
  last_active_at?: string | null;
};

type Client = {
  id: string;
  member_id: string;
  display_name: string;
  client_type: string;
  scopes: string[];
  status: string;
  created_at: string;
  last_seen_at?: string | null;
};

type Credential = { id: string; client_id: string; token_prefix: string; created_at: string; last_used_at?: string | null; revoked_at?: string | null };

type Node = {
  id: string;
  display_name: string;
  role: string;
  endpoint?: string | null;
  trust_state: string;
  last_seen_at?: string | null;
  last_sync_at?: string | null;
};

let accessCsrf: string | null = null;

async function json<T>(url: string, init?: RequestInit): Promise<T> {
  if (init?.method && init.method !== 'GET' && !accessCsrf) {
    const session = await fetch('/api/access/session', { cache: 'no-store' });
    if (!session.ok) throw new Error('Local owner session required');
    accessCsrf = (await session.json()).csrf;
  }
  const response = await fetch(url, {
    ...init,
    cache: 'no-store',
    headers: { 'Content-Type': 'application/json', ...(accessCsrf ? { 'x-ax-access-csrf': accessCsrf } : {}), ...(init?.headers ?? {}) },
  });
  const body = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(body.error ?? `HTTP ${response.status}`);
  return body as T;
}

const DEFAULT_SCOPES = ['memory:read', 'memory:write', 'preflight:read', 'context:read'];
const ALL_SCOPES = [...DEFAULT_SCOPES, 'share:read', 'share:write', 'admin:clients', 'admin:team', 'admin:nodes'];

export default function AccessSettingsSection({ section }: { section: string }) {
  const active = ['team', 'clients', 'nodes'].includes(section);
  useEffect(() => setIssuedToken(null), [section]);
  const [members, setMembers] = useState<Member[]>([]);
  const [clients, setClients] = useState<Client[]>([]);
  const [credentials, setCredentials] = useState<Credential[]>([]);
  const [scopeDrafts, setScopeDrafts] = useState<Record<string, string[]>>({});
  const [localNode, setLocalNode] = useState<{ node_id: string; role: string; primary_url?: string; last_sync_at?: string; sync_state: string } | null>(null);
  const [nodes, setNodes] = useState<Node[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [issuedToken, setIssuedToken] = useState<string | null>(null);
  const [memberName, setMemberName] = useState('');
  const [memberRole, setMemberRole] = useState('member');
  const [clientName, setClientName] = useState('');
  const [clientType, setClientType] = useState('custom');
  const [clientMember, setClientMember] = useState('');

  async function refresh() {
    try {
      const [team, clientData, nodeData] = await Promise.all([
        json<{ members: Member[] }>('/api/access/team'),
        json<{ clients: Client[]; credentials: Credential[] }>('/api/access/clients'),
        json<{ nodes: Node[]; local?: typeof localNode }>('/api/access/nodes'),
      ]);
      setMembers(team.members);
      setClients(clientData.clients);
      setCredentials(clientData.credentials ?? []);
      setNodes(nodeData.nodes);
      setLocalNode(nodeData.local ?? null);
      if (team.members.length) setClientMember((current) => current || team.members[0].id);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    void refresh();
    const interval = window.setInterval(() => void refresh(), 30000);
    return () => window.clearInterval(interval);
  }, []);

  async function addMember() {
    if (!memberName.trim()) return;
    try {
      await json('/api/access/team', {
        method: 'POST',
        body: JSON.stringify({ display_name: memberName.trim(), role: memberRole }),
      });
      setMemberName('');
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function addClient() {
    if (!clientName.trim() || !clientMember) return;
    try {
      await json('/api/access/clients', {
        method: 'POST',
        body: JSON.stringify({
          member_id: clientMember,
          display_name: clientName.trim(),
          client_type: clientType,
          scopes: members.find((m) => m.id === clientMember)?.role === 'readonly' ? DEFAULT_SCOPES.filter((scope) => !scope.endsWith(':write')) : DEFAULT_SCOPES,
        }),
      });
      setClientName('');
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function issueClientToken(id: string, rotate = false) {
    try {
      const data = await json<{ credential: { token: string } }>(`/api/access/clients/${id}/${rotate ? 'rotate' : 'token'}`, {
        method: 'POST',
      });
      setIssuedToken(data.credential.token);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function revokeClient(id: string) {
    try {
      await json(`/api/access/clients/${id}/revoke`, { method: 'POST' });
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function revokeNode(id: string) {
    try {
      await json(`/api/access/nodes/${id}/revoke`, { method: 'POST' });
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function mutate(url: string, method = 'POST', body?: unknown) {
    setIssuedToken(null);
    try {
      await json(url, { method, ...(body ? { body: JSON.stringify(body) } : {}) });
      await refresh();
    } catch (e) { setError(String(e)); }
  }

  async function saveScopes(client: Client) {
    await mutate(`/api/access/clients/${client.id}/scopes`, 'PATCH', { scopes: scopeDrafts[client.id] ?? client.scopes });
    setScopeDrafts((drafts) => { const next = { ...drafts }; delete next[client.id]; return next; });
  }

  return (
    <>
      {active && error && <div className="settings-toast settings-toast--err">{error}</div>}
      {active && issuedToken && (
        <div className="settings-toast settings-toast--ok">
          <strong>Credential shown once.</strong>{' '}
          <code style={{ userSelect: 'all' }}>{issuedToken}</code>
          <button type="button" className="btn" onClick={() => setIssuedToken(null)}>Dismiss</button>
        </div>
      )}

      <section id="settings-panel-team" className="settings-card" hidden={section !== 'team'}>
        <div className="settings-card-header">
          <h2>Team</h2>
          <p>People who may use this Ax environment. Roles limit what their clients can do.</p>
        </div>
        <div className="settings-card-body">
          <div className="settings-subsection-label">Members</div>
          {members.map((member) => (
            <div className="settings-row" key={member.id}>
              <div className="settings-row-label">
                <span className="settings-row-title">{member.display_name}</span>
                <span className="settings-row-desc">
                  {member.role} · {member.status} · joined {member.created_at}{member.last_active_at ? ` · active ${member.last_active_at}` : ''}
                </span>
              </div>
              <div className="settings-row-control access-row-control">
                <select aria-label={`Role for ${member.display_name}`} className="settings-select" value={member.role}
                  onChange={(e) => void mutate(`/api/access/team/${member.id}`, 'PATCH', { role: e.target.value, status: member.status })}>
                  <option value="owner">Owner</option><option value="admin">Admin</option><option value="member">Member</option><option value="readonly">Read-only</option>
                </select>
                <button type="button" className="btn" onClick={() => void mutate(`/api/access/team/${member.id}`, 'PATCH', { role: member.role, status: member.status === 'active' ? 'disabled' : 'active' })}>
                  {member.status === 'active' ? 'Disable' : 'Enable'}
                </button>
              </div>
            </div>
          ))}
          <div className="settings-divider" />
          <div className="settings-row">
            <div className="settings-row-label">
              <span className="settings-row-title">Add team member</span>
              <span className="settings-row-desc">Creates a human identity. Clients are added separately.</span>
            </div>
            <div className="settings-row-control access-row-control">
              <input className="settings-input" value={memberName} placeholder="Name" onChange={(e) => setMemberName(e.target.value)} />
              <select className="settings-select" value={memberRole} onChange={(e) => setMemberRole(e.target.value)}>
                <option value="admin">Admin</option>
                <option value="member">Member</option>
                <option value="readonly">Read-only</option>
              </select>
              <button type="button" className="btn primary" onClick={() => void addMember()}>Add</button>
            </div>
          </div>
        </div>
      </section>

      <section id="settings-panel-clients" className="settings-card" hidden={section !== 'clients'}>
        <div className="settings-card-header">
          <h2>Clients & tokens</h2>
          <p>Applications such as ChatGPT, Cursor, Cline, CLI, or custom integrations. Tokens belong to a client, never to the whole environment.</p>
        </div>
        <div className="settings-card-body">
          {clients.map((client) => (
            <div key={client.id}>
            <div className="settings-row">
              <div className="settings-row-label">
                <span className="settings-row-title">{client.display_name}</span>
                <span className="settings-row-desc">
                  {client.client_type} · {members.find((member) => member.id === client.member_id)?.display_name ?? client.member_id} · {client.status} · created {client.created_at} · {client.scopes.join(', ')}
                  {client.last_seen_at ? ` · last used ${client.last_seen_at}` : ''}
                </span>
              </div>
              <div className="settings-row-control access-row-control">
                {client.status === 'active' && (
                  <>
                    <button type="button" className="btn" onClick={() => void issueClientToken(client.id)}>Issue token</button>
                    <button type="button" className="btn" onClick={() => void issueClientToken(client.id, true)}>Rotate tokens</button>
                    <button type="button" className="btn" onClick={() => void revokeClient(client.id)}>Revoke client</button>
                  </>
                )}
              </div>
            </div>
            {client.status === 'active' && <div className="settings-row">
              <div className="settings-row-label"><span className="settings-row-title">Scopes</span><span className="settings-row-desc">Permissions remain limited by the member role.</span></div>
              <div className="settings-row-control access-row-control" style={{ flexWrap: 'wrap' }}>
                {ALL_SCOPES.map((scope) => <label key={scope} style={{ marginRight: 8 }}>
                  <input type="checkbox" checked={(scopeDrafts[client.id] ?? client.scopes).includes(scope)}
                    onChange={(e) => setScopeDrafts((drafts) => ({ ...drafts, [client.id]: e.target.checked ? [...(drafts[client.id] ?? client.scopes), scope] : (drafts[client.id] ?? client.scopes).filter((value) => value !== scope) }))} /> {scope}
                </label>)}
                <button type="button" className="btn" onClick={() => void saveScopes(client)}>Save scopes</button>
              </div>
            </div>}
            {credentials.filter((credential) => credential.client_id === client.id).map((credential) => <div className="settings-row" key={credential.id}>
              <div className="settings-row-label"><span className="settings-row-title">{credential.token_prefix}…</span><span className="settings-row-desc">{credential.revoked_at ? 'Revoked' : 'Active'} · created {credential.created_at}{credential.last_used_at ? ` · used ${credential.last_used_at}` : ''}</span></div>
              {!credential.revoked_at && <div className="settings-row-control access-row-control"><button type="button" className="btn" onClick={() => void mutate(`/api/access/credentials/${credential.id}/revoke`)}>Revoke token</button></div>}
            </div>)}
            </div>
          ))}
          <div className="settings-divider" />
          <div className="settings-row">
            <div className="settings-row-label">
              <span className="settings-row-title">Add client</span>
              <span className="settings-row-desc">The default scope set can be changed above; it can never exceed the member role.</span>
            </div>
            <div className="settings-row-control access-row-control">
              <input className="settings-input" value={clientName} placeholder="Client name" onChange={(e) => setClientName(e.target.value)} />
              <select className="settings-select" value={clientType} onChange={(e) => setClientType(e.target.value)}>
                <option value="chatgpt">ChatGPT</option>
                <option value="cursor">Cursor</option>
                <option value="cline">Cline</option>
                <option value="cli">CLI</option>
                <option value="custom">Custom</option>
              </select>
              <select className="settings-select" value={clientMember} onChange={(e) => setClientMember(e.target.value)}>
                {members.filter((member) => member.status === 'active').map((member) => (
                  <option key={member.id} value={member.id}>{member.display_name}</option>
                ))}
              </select>
              <button type="button" className="btn primary" onClick={() => void addClient()}>Add</button>
            </div>
          </div>
        </div>
      </section>

      <section id="settings-panel-nodes" className="settings-card" hidden={section !== 'nodes'}>
        <div className="settings-card-header">
          <h2>Replication</h2>
          <p>Primary/Replica nodes are infrastructure identities, separate from people and application clients.</p>
        </div>
        <div className="settings-card-body">
          <div className="settings-row"><div className="settings-row-label">
            <span className="settings-row-title">{localNode ? `Local ${localNode.role === 'primary' ? 'Primary' : 'Replica'}` : 'Local standalone node not configured'}</span>
            <span className="settings-row-desc">{localNode ? `${localNode.node_id} · ${localNode.sync_state}${localNode.primary_url ? ` · Primary ${localNode.primary_url}` : ''}${localNode.last_sync_at ? ` · synced ${localNode.last_sync_at}` : ''}` : 'Start ax server with its node configuration to show identity and connection state here.'}</span>
          </div></div>
          {nodes.length === 0 && (
            <div className="settings-row">
              <div className="settings-row-label">
                <span className="settings-row-title">No replicas discovered</span>
                <span className="settings-row-desc">A correctly configured Replica appears here automatically after authenticated registration.</span>
              </div>
            </div>
          )}
          {nodes.map((node) => (
            <div className="settings-row" key={node.id}>
              <div className="settings-row-label">
                <span className="settings-row-title">{node.display_name}</span>
                <span className="settings-row-desc">
                  {node.role} · {node.trust_state} · {node.last_seen_at && Date.now() - Date.parse(node.last_seen_at.replace(' ', 'T') + (node.last_seen_at.endsWith('Z') ? '' : 'Z')) < 120000 ? 'Online' : 'Not seen recently'}
                  {node.endpoint ? ` · ${node.endpoint}` : ''}
                  {node.last_seen_at ? ` · seen ${node.last_seen_at}` : ''}
                  {node.last_sync_at ? ` · synced ${node.last_sync_at}` : ''}
                </span>
              </div>
              <div className="settings-row-control access-row-control">
                {node.trust_state === 'revoked' && <button type="button" className="btn" onClick={() => void mutate(`/api/access/nodes/${node.id}/reenroll`)}>Allow re-enrollment</button>}
                {node.trust_state === 'trusted' && (
                  <button type="button" className="btn" onClick={() => void revokeNode(node.id)}>Revoke</button>
                )}
              </div>
            </div>
          ))}
        </div>
      </section>
    </>
  );
}
