import { useEffect, useState } from 'react';

type Member = {
  id: string;
  display_name: string;
  email?: string | null;
  role: string;
  status: string;
  last_active_at?: string | null;
};

type Client = {
  id: string;
  member_id: string;
  display_name: string;
  client_type: string;
  scopes: string[];
  status: string;
  last_seen_at?: string | null;
};

type Node = {
  id: string;
  display_name: string;
  role: string;
  endpoint?: string | null;
  trust_state: string;
  last_seen_at?: string | null;
  last_sync_at?: string | null;
};

async function json<T>(url: string, init?: RequestInit): Promise<T> {
  const response = await fetch(url, {
    ...init,
    headers: { 'Content-Type': 'application/json', ...(init?.headers ?? {}) },
  });
  const body = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(body.error ?? `HTTP ${response.status}`);
  return body as T;
}

const DEFAULT_SCOPES = ['memory:read', 'memory:write', 'preflight:read', 'context:read'];

export default function AccessSettingsSection() {
  const [members, setMembers] = useState<Member[]>([]);
  const [clients, setClients] = useState<Client[]>([]);
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
        json<{ clients: Client[] }>('/api/access/clients'),
        json<{ nodes: Node[] }>('/api/access/nodes'),
      ]);
      setMembers(team.members);
      setClients(clientData.clients);
      setNodes(nodeData.nodes);
      if (!clientMember && team.members.length) setClientMember(team.members[0].id);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    void refresh();
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
          scopes: DEFAULT_SCOPES,
        }),
      });
      setClientName('');
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  async function issueClientToken(id: string) {
    try {
      const data = await json<{ credential: { token: string } }>(`/api/access/clients/${id}/token`, {
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

  return (
    <>
      {error && <div className="settings-toast settings-toast--err">{error}</div>}
      {issuedToken && (
        <div className="settings-toast settings-toast--ok">
          <strong>Credential shown once.</strong>{' '}
          <code style={{ userSelect: 'all' }}>{issuedToken}</code>
          <button type="button" className="btn" onClick={() => setIssuedToken(null)}>Dismiss</button>
        </div>
      )}

      <section className="settings-card">
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
                  {member.role} · {member.status}{member.last_active_at ? ` · active ${member.last_active_at}` : ''}
                </span>
              </div>
            </div>
          ))}
          <div className="settings-divider" />
          <div className="settings-row">
            <div className="settings-row-label">
              <span className="settings-row-title">Add team member</span>
              <span className="settings-row-desc">Creates a human identity. Clients are added separately.</span>
            </div>
            <div className="settings-row-control">
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

      <section className="settings-card">
        <div className="settings-card-header">
          <h2>Clients & tokens</h2>
          <p>Applications such as ChatGPT, Cursor, Cline, CLI, or custom integrations. Tokens belong to a client, never to the whole environment.</p>
        </div>
        <div className="settings-card-body">
          {clients.map((client) => (
            <div className="settings-row" key={client.id}>
              <div className="settings-row-label">
                <span className="settings-row-title">{client.display_name}</span>
                <span className="settings-row-desc">
                  {client.client_type} · {client.status} · {client.scopes.join(', ')}
                  {client.last_seen_at ? ` · last used ${client.last_seen_at}` : ''}
                </span>
              </div>
              <div className="settings-row-control">
                {client.status === 'active' && (
                  <>
                    <button type="button" className="btn" onClick={() => void issueClientToken(client.id)}>Issue token</button>
                    <button type="button" className="btn" onClick={() => void revokeClient(client.id)}>Revoke</button>
                  </>
                )}
              </div>
            </div>
          ))}
          <div className="settings-divider" />
          <div className="settings-row">
            <div className="settings-row-label">
              <span className="settings-row-title">Add client</span>
              <span className="settings-row-desc">The default scope set can be narrowed later; it can never exceed the member role.</span>
            </div>
            <div className="settings-row-control">
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

      <section className="settings-card">
        <div className="settings-card-header">
          <h2>Replication</h2>
          <p>Primary/Replica nodes are infrastructure identities, separate from people and application clients.</p>
        </div>
        <div className="settings-card-body">
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
                  {node.role} · {node.trust_state}
                  {node.endpoint ? ` · ${node.endpoint}` : ''}
                  {node.last_seen_at ? ` · seen ${node.last_seen_at}` : ''}
                  {node.last_sync_at ? ` · synced ${node.last_sync_at}` : ''}
                </span>
              </div>
              <div className="settings-row-control">
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
