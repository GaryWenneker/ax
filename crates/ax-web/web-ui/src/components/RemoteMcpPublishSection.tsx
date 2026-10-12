import { useCallback, useEffect, useRef, useState } from 'react';
import { Publication, PublicationList, publicationRequest, isRemoteClient, ConnectionExport, RemoteClient, RemoteExport, RemoteMcpError, RemoteTransport } from '../remoteMcpApi';
import { WORKSPACE_SWITCHED } from '../workspaceEvents';
import './RemoteMcpPublishSection.css';
import ModalShell from './ModalShell';
import LocalMcpConfigSection from './LocalMcpConfigSection';
import RemoteHostSetup from './RemoteHostSetup';
import ServiceStatus from './ServiceStatus';

const clients: { id: RemoteClient; label: string }[] = [
  { id: 'cursor', label: 'Cursor' }, { id: 'vscode', label: 'VS Code' },
  { id: 'cline', label: 'Cline' }, { id: 'claudeCode', label: 'Claude Code' },
  { id: 'chatgpt', label: 'ChatGPT' }, { id: 'axProxy', label: 'Other / Ax proxy' },
];
const guide = 'https://getax.wenneker.io/guides/remote-mcp/';
export default function RemoteMcpPublishSection() {
  const [data, setData] = useState<PublicationList | null>(null);
  const [selected, setSelected] = useState('');
  const [client, setClient] = useState<RemoteClient>('cursor');
  const [transport, setTransport] = useState<RemoteTransport>('direct');
  const [output, setOutput] = useState<RemoteExport | null>(null);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [message, setMessage] = useState('');
  const [checking, setChecking] = useState('');
  const [connectionText, setConnectionText] = useState('');
  const transferText = useRef<HTMLTextAreaElement>(null);
  const [showImport, setShowImport] = useState(false);
  const [file, setFile] = useState<File | null>(null);
  const [name, setName] = useState('');
  const [nativeId, setNativeId] = useState('');
  const [cursorId, setCursorId] = useState('');
  const [vscodeId, setVscodeId] = useState('');
  const [clock, setClock] = useState(Date.now());
  const generation = useRef(0);
  const controller = useRef(new AbortController());
  const text = useRef<HTMLTextAreaElement>(null);
  const publication = data?.publications.find(p => p.id === selected);
  const effectiveTransport = client === 'axProxy' ? 'proxy' : ['chatgpt', 'claudeCode'].includes(client) ? 'direct' : transport;

  const reset = useCallback(() => {
    generation.current++;
    controller.current.abort(); controller.current = new AbortController();
    setData(null); setSelected(''); setOutput(null); setMessage(''); setError(''); setChecking('');
    setConnectionText(''); setBusy(false); setLoading(true); setFile(null); setName(''); setNativeId(''); setCursorId(''); setVscodeId(''); setShowImport(false);
  }, []);
  const load = useCallback(async () => {
    reset();
    const epoch = generation.current;
    try {
      const value = await publicationRequest<PublicationList>('/', { signal: controller.current.signal });
      if (epoch !== generation.current) return;
      setData(value);
      setSelected(value.publications.length === 1 ? value.publications[0].id : '');
    } catch (e) { if (epoch === generation.current && !(e instanceof DOMException && e.name === 'AbortError')) setError(e instanceof Error ? e.message : String(e)); }
    finally { if (epoch === generation.current) setLoading(false); }
  }, [reset]);
  useEffect(() => {
    void load();
    const changed = () => { void load(); };
    window.addEventListener(WORKSPACE_SWITCHED, changed);
    const timer = setInterval(() => setClock(Date.now()), 15000);
    return () => { generation.current++; controller.current.abort(); window.removeEventListener(WORKSPACE_SWITCHED, changed); clearInterval(timer); };
  }, [load]);

  const handleError = useCallback((e: unknown, epoch: number) => {
    if (epoch !== generation.current || (e instanceof DOMException && e.name === 'AbortError')) return;
    setError(e instanceof Error ? e.message : String(e));
    if (e instanceof RemoteMcpError && [404, 409].includes(e.status)) { setOutput(null); setSelected(''); setData(null); }
  }, []);
  function identity(p: Publication) { return { workspaceId: data!.workspaceId, configFingerprint: p.configFingerprint }; }
  const generate = useCallback(async (p: Publication, exportClient = client): Promise<RemoteExport | null> => {
    const epoch = generation.current;
    const value = await publicationRequest<RemoteExport>(`/${encodeURIComponent(p.id)}/export`, {
      method: 'POST', csrf: data!.csrf, signal: controller.current.signal,
      body: { workspaceId: data!.workspaceId, configFingerprint: p.configFingerprint, client: exportClient, transport: exportClient === 'chatgpt' ? 'direct' : effectiveTransport },
    });
    if (epoch !== generation.current || value.workspaceId !== data!.workspaceId || value.publicationId !== p.id || value.configFingerprint !== p.configFingerprint) return null;
    return value;
  }, [client, data, effectiveTransport]);
  useEffect(() => {
    setOutput(null); setConnectionText(''); setMessage(''); setError(''); setChecking('');
    if (!publication || !data) return;
    let cancelled = false;
    const epoch = generation.current;
    setBusy(true);
    generate(publication).then(value => { if (!cancelled && value) setOutput(value); })
      .catch(e => { if (!cancelled) handleError(e, epoch); })
      .finally(() => { if (!cancelled && epoch === generation.current) setBusy(false); });
    return () => { cancelled = true; };
  }, [publication, data, generate, handleError]);

  async function act(action: (p: Publication, epoch: number) => Promise<void>) {
    if (!publication || !data) return;
    const epoch = generation.current;
    setBusy(true); setError(''); setMessage('');
    try { await action(publication, epoch); }
    catch (e) { handleError(e, epoch); }
    finally { if (epoch === generation.current) setBusy(false); }
  }
  async function copy(kind: 'config' | 'url' | 'login') {
    await act(async (p, epoch) => {
      // Revalidate the active workspace on every copy, including URL and login commands.
      const value = await generate(p, kind === 'url' ? 'chatgpt' : client);
      if (!value || epoch !== generation.current) return;
      const content = kind === 'login' ? value.loginCommand : value.content;
      if (!content) return;
      if (kind !== 'url') setOutput(value);
      try { await navigator.clipboard.writeText(content); }
      catch {
        if (epoch !== generation.current) return;
        setOutput(kind === 'config' ? value : { ...value, content, format: 'url', fileName: null });
        setError('Clipboard access was denied. Select the text below and copy it manually.');
        requestAnimationFrame(() => { text.current?.focus(); text.current?.select(); });
        return;
      }
      if (epoch === generation.current) setMessage(kind === 'url' ? 'MCP URL copied.' : kind === 'login' ? 'Login command copied.' : 'Configuration copied.');
    });
  }
  async function download() {
    await act(async (p, epoch) => {
      const value = await generate(p);
      if (!value?.fileName || epoch !== generation.current) return;
      const url = URL.createObjectURL(new Blob([value.content], { type: 'application/json' }));
      const link = document.createElement('a'); link.href = url; link.download = value.fileName; link.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000); setMessage('Configuration downloaded. Merge its server entry into your existing file.');
    });
  }
  async function register() {
    if (!file || !data) return;
    const epoch = generation.current; setBusy(true); setError(''); setMessage('');
    try {
      if (file.size > 240 * 1024) throw new Error('Choose a server configuration smaller than 240 KiB.');
      const config = JSON.parse(await file.text());
      if (epoch !== generation.current) return;
      const hasClients = !!(nativeId.trim() || cursorId.trim() || vscodeId.trim());
      await publicationRequest('/import', { method: 'POST', csrf: data.csrf, signal: controller.current.signal,
        body: { workspaceId: data.workspaceId, config, name: name.trim() || null,
          clients: hasClients ? { axProxy: nativeId.trim() || null, cursor: cursorId.trim() || null, vscode: vscodeId.trim() || null } : null } });
      if (epoch !== generation.current) return;
      await load();
    } catch (e) { handleError(e, epoch); }
    finally { if (epoch === generation.current) setBusy(false); }
  }
  const refreshLocal = useCallback(() => { void load(); }, [load]);
  async function copyConnection() {
    await act(async (p, epoch) => {
      const value = await publicationRequest<ConnectionExport>(`/${p.id}/connection`, { method: 'POST', body: identity(p), csrf: data!.csrf, signal: controller.current.signal });
      if (epoch !== generation.current || value.workspaceId !== data!.workspaceId || value.publicationId !== p.id || value.configFingerprint !== p.configFingerprint) return;
      setConnectionText(value.content);
      try { await navigator.clipboard.writeText(value.content); }
      catch {
        if (epoch !== generation.current) return;
        setError('Clipboard access was denied. Select and copy the connection details below.');
        requestAnimationFrame(() => { transferText.current?.focus(); transferText.current?.select(); }); return;
      }
      if (epoch === generation.current) setMessage(value.requiresClientId ? 'Connection details copied. The server owner still needs to supply a public native Ax client ID for browser sign-in.' : 'Connection details copied. On the receiving computer, choose Paste connection details under Connect to another Ax.');
    });
  }
  const runtime = publication?.runtimeStatus === 'running' && (!publication.lastHeartbeatAt || clock / 1000 - publication.lastHeartbeatAt > 45) ? 'unknown' : publication?.runtimeStatus;
  return <section className="remote-mcp-publish" aria-labelledby="remote-publish-title">
    <LocalMcpConfigSection key={data?.workspaceId ?? 'loading'} workspace={data} onStale={refreshLocal} />
    {data && <RemoteHostSetup key={data.workspaceId} workspaceId={data.workspaceId} csrf={data.csrf} onChanged={load} />}
    <div className="remote-publish-heading"><div><h3 id="remote-publish-title">Share this project</h3><p>Let another computer access {data?.projectLabel ?? 'your active project'} through its remote HTTPS server.</p></div>
      <button type="button" className="btn" disabled={loading || busy} onClick={() => void load()} aria-label="Refresh project publications">Refresh</button></div>
    {loading && <p role="status">Loading project publications…</p>}
    {data && data.publications.length === 0 && <div className="remote-publish-empty"><p>No remote server is registered for this project.</p><p>For an app on this computer, use the copy buttons above. To offer this project to another computer, choose <strong>Set up remote MCP</strong> on the hosting computer. Ax can generate the server configuration and manage its authentication.</p><p>Receiving someone else’s connection? Use <strong>Paste connection details</strong> under <strong>Connect to another Ax</strong> below.</p></div>}
    {data && data.publications.length > 1 && <div className="settings-row"><label htmlFor="remote-publisher">Publisher</label>
      <select id="remote-publisher" value={selected} disabled={busy} onChange={e => { setOutput(null); setSelected(e.target.value); }}>
        <option value="">Choose a publisher…</option>{data.publications.map(p => <option key={p.id} value={p.id}>{p.publisherLabel} · {p.remoteProjectId}</option>)}
      </select></div>}
    {publication && <>
      <dl className="remote-publish-details"><dt>Publisher</dt><dd>{publication.publisherLabel}</dd><dt>Remote project ID</dt><dd>{publication.remoteProjectId}</dd><dt>MCP URL</dt><dd>{publication.endpointUrl}</dd></dl>
      <p className="remote-publish-state"><ServiceStatus state={runtime === 'running' ? 'running' : runtime === 'stopped' ? 'stopped' : 'warning'} label={runtime === 'running' ? 'Running' : runtime === 'stopped' ? 'Stopped' : 'Status unknown'} /> {runtime === 'running' ? 'Local server running. Public endpoint has not been verified.' : runtime === 'stopped' ? 'Server stopped. You can copy the configuration and start the server separately.' : 'Runtime status unknown. The last local heartbeat is stale.'}
        {publication.lastHeartbeatAt && <span> Last heartbeat: {new Date(publication.lastHeartbeatAt * 1000).toLocaleString()}.</span>}</p>
      <div className="remote-publish-controls"><label>Client<select aria-label="MCP client" value={client} disabled={busy} onChange={e => {
        const next = e.target.value; if (!isRemoteClient(next)) return; setOutput(null); setClient(next); setTransport(next === 'cline' || next === 'axProxy' ? 'proxy' : 'direct');
      }}>{clients.map(c => <option key={c.id} value={c.id}>{c.label}</option>)}</select></label>
      {!['chatgpt', 'claudeCode', 'axProxy'].includes(client) && <label>Transport<select aria-label="MCP transport" value={transport} disabled={busy} onChange={e => { setOutput(null); if (e.target.value === 'direct' || e.target.value === 'proxy') setTransport(e.target.value); }}>
        <option value="direct">Direct HTTP with OAuth</option><option value="proxy">Ax proxy (stdio)</option></select></label>}</div>
      {output && <>
        {output.requirements.length > 0 && <ul className="remote-publish-requirements">{output.requirements.map(r => <li key={r}>{r}</li>)}</ul>}
        <label className="remote-publish-preview">{output.format === 'url' ? 'Connection text' : 'Client configuration'}<textarea ref={text} aria-label="Generated MCP configuration" readOnly value={output.content} rows={output.format === 'url' ? 3 : 12} spellCheck={false} /></label>
        <div className="remote-publish-actions">
          {output.format === 'json' && <button className="btn btn-primary" type="button" disabled={busy} onClick={() => void copy('config')}>Copy configuration</button>}
          <button className="btn" type="button" disabled={busy} onClick={() => void copy('url')}>Copy MCP URL</button>
          {output.fileName && <button className="btn" type="button" disabled={busy} onClick={() => void download()}>Download configuration</button>}
          {effectiveTransport === 'proxy' && <button className="btn" type="button" disabled={busy || !output.loginCommand} onClick={() => void copy('login')}>Copy login command (POSIX)</button>}
        </div>
        <ol>{output.instructions.map(i => <li key={i}>{i}</li>)}</ol>
        <p>No secrets are included. Every receiving user needs their own project access and sign-in.</p>
      </>}
      <div className="remote-publish-transfer"><h4>Send this connection to another Ax computer</h4>
        <p>Copy the address and public Ax sign-in ID together. The receiving person pastes these details into Connect to another Ax and signs in with their own account.</p>
        <button type="button" className="btn" disabled={busy} onClick={() => void copyConnection()}>Copy connection details</button>
        {connectionText && <textarea ref={transferText} aria-label="Shareable Ax connection details" value={connectionText} readOnly rows={4} spellCheck={false} />}
      </div>
      <div className="remote-publish-actions"><button className="btn" type="button" disabled={busy} onClick={() => void act(async (p, epoch) => {
        const result = await publicationRequest<{ workspaceId: string; publicationId: string; configFingerprint: string; message: string }>(`/${p.id}/check`, { method: 'POST', body: identity(p), csrf: data!.csrf, signal: controller.current.signal });
        if (epoch === generation.current && result.workspaceId === data!.workspaceId && result.publicationId === p.id && result.configFingerprint === p.configFingerprint) setChecking(result.message);
      })}>Verify endpoint</button>
      <button className="btn" type="button" disabled={busy} onClick={() => void act(async (p, epoch) => {
        await publicationRequest(`/${p.id}`, { method: 'DELETE', body: identity(p), csrf: data!.csrf, signal: controller.current.signal });
        if (epoch === generation.current) await load();
      })}>Remove local registration</button></div>
      {checking && <p className="remote-publish-check" role="status">{checking}</p>}
    </>}
    {data && <details><summary>Advanced: register an externally managed server</summary><div className="remote-publish-actions"><button className="btn" type="button" disabled={busy} onClick={() => setShowImport(!showImport)}>Import existing server file</button>
      <a href={guide} target="_blank" rel="noopener noreferrer">View setup guide</a></div></details>}
    {showImport && <ModalShell title="Import existing remote server" onClose={() => setShowImport(false)} footer={<><button className="btn" type="button" onClick={() => setShowImport(false)}>Cancel</button><button className="btn btn-primary" type="button" disabled={busy || !file} onClick={() => void register()}>Import for this project</button></>}><div className="remote-publish-import"><p>Use this on the computer hosting the project. Choose the <code>remote-mcp.json</code> file used with <code>ax mcp remote serve --config</code>. Ax reads it to find this project’s HTTPS address.</p><p>This is a server setup file, rather than a Cursor or VS Code MCP settings file. If you do not have a remote server yet, <a href={guide} target="_blank" rel="noopener noreferrer">follow the remote server setup guide</a>. For local use, cancel and use <strong>Copy for this computer</strong>.</p>
      <label>Server configuration file<input aria-label="Server configuration file" type="file" accept=".json,application/json" disabled={busy} onChange={e => setFile(e.target.files?.[0] ?? null)} /></label>
      <label>Server name (optional)<input value={name} onChange={e => setName(e.target.value)} maxLength={160} disabled={busy} /></label>
      <details><summary>Advanced: public OAuth client IDs (optional)</summary><p>Enter IDs already registered with your sign-in provider. These are public app identifiers, not passwords. Leave this section empty to keep existing IDs.</p>
      <label>Native Ax public client ID (optional)<input value={nativeId} onChange={e => setNativeId(e.target.value)} disabled={busy} /></label>
      <label>Cursor public client ID (optional)<input value={cursorId} onChange={e => setCursorId(e.target.value)} disabled={busy} /></label>
      <label>VS Code public client ID (optional)<input value={vscodeId} onChange={e => setVscodeId(e.target.value)} disabled={busy} /></label>
      </details><p>This imports settings for this project only. It does not start the server or sign anyone in. Private server settings are excluded from the configurations you share.</p>
      {error && <p role="alert">{error}</p>}</div></ModalShell>}
    {busy && <p role="status">Preparing project configuration…</p>}
    {message && <p className="remote-publish-feedback" role="status">{message}</p>}{error && !showImport && <p role="alert">{error}</p>}
  </section>;
}
