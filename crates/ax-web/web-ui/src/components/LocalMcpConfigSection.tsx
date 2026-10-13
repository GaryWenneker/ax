import { useCallback, useEffect, useRef, useState } from 'react';
import { LocalExport, LocalClient, PublicationList, publicationRequest, RemoteMcpError } from '../remoteMcpApi';
import SettingsSelect from './ui/SettingsSelect';

type Props = { workspace: PublicationList | null; onStale: () => void };
const localClients: { id: LocalClient; label: string }[] = [
  { id: 'cursor', label: 'Cursor' }, { id: 'vscode', label: 'VS Code' }, { id: 'cline', label: 'Cline' },
  { id: 'claudeCode', label: 'Claude Code' }, { id: 'other', label: 'Other app with local MCP' },
];
export default function LocalMcpConfigSection({ workspace, onStale }: Props) {
  const [client, setClient] = useState<LocalClient>('cursor');
  const [output, setOutput] = useState<LocalExport | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [message, setMessage] = useState('');
  const [preview, setPreview] = useState(false);
  const current = useRef(0);
  const text = useRef<HTMLTextAreaElement>(null);
  const controller = useRef(new AbortController());
  const identity = workspace?.workspaceId;
  const csrf = workspace?.csrf;
  const shareable = client === 'cursor' || client === 'vscode';
  const generate = useCallback(async (target: 'thisComputer' | 'projectFolder', signal?: AbortSignal) => {
    if (!identity || !csrf) return null;
    const value = await publicationRequest<LocalExport>('/local/export', { method: 'POST', signal,
      csrf, body: { workspaceId: identity, client, target } });
    if (value.workspaceId !== identity || value.client !== client || value.target !== target) throw new Error('The project changed. Refresh Settings before copying.');
    return value;
  }, [client, identity, csrf]);
  const fail = useCallback((e: unknown, epoch: number) => {
    if (epoch !== current.current || (e instanceof DOMException && e.name === 'AbortError')) return;
    setError(e instanceof Error ? e.message : String(e));
    if (e instanceof RemoteMcpError && [404, 409].includes(e.status)) {
      setOutput(null);
      // A missing endpoint must stay visible, rather than repeatedly remounting this card.
      if (e.status === 409) onStale();
    }
  }, [onStale]);
  useEffect(() => {
    current.current++;
    controller.current.abort(); controller.current = new AbortController();
    const epoch = current.current;
    setOutput(null); setError(''); setMessage(''); setBusy(!!identity); setPreview(false);
    if (identity) generate('thisComputer', controller.current.signal).then(value => { if (epoch === current.current) setOutput(value); })
      .catch(e => fail(e, epoch)).finally(() => { if (epoch === current.current) setBusy(false); });
    return () => { current.current++; controller.current.abort(); };
  }, [identity, generate, fail]);
  async function useConfiguration(target: 'thisComputer' | 'projectFolder', action: 'copy' | 'download') {
    const epoch = current.current; setBusy(true); setError(''); setMessage('');
    try {
      const value = await generate(target, controller.current.signal);
      if (!value || epoch !== current.current) return;
      setOutput(value);
      if (action === 'download') {
        const url = URL.createObjectURL(new Blob([value.content], { type: 'application/json' }));
        const link = document.createElement('a'); link.href = url; link.download = value.fileName; link.click(); setTimeout(() => URL.revokeObjectURL(url), 1000);
        setMessage('Configuration file downloaded. Add its ax entry to the app settings shown below.');
      } else {
        try { await navigator.clipboard.writeText(value.content); }
        catch {
          if (epoch !== current.current) return;
          setPreview(true); setError('Clipboard access was denied. Select and copy the configuration below.');
          requestAnimationFrame(() => { text.current?.focus(); text.current?.select(); }); return;
        }
        if (epoch === current.current) setMessage(target === 'projectFolder' ? 'Shareable project configuration copied.' : 'Configuration for this computer copied.');
      }
    } catch (e) { fail(e, epoch); }
    finally { if (epoch === current.current) setBusy(false); }
  }
  return <section className="local-mcp-config" aria-labelledby="local-mcp-title">
    <h3 id="local-mcp-title">Use on this computer</h3>
    <p>Connect an app to <strong>{workspace?.projectLabel ?? 'your active project'}</strong> here. Ax prepares the settings; no server file or sign-in is required.</p>
    <label>App<SettingsSelect aria-label="Local MCP app" value={client} disabled={!workspace || busy} onChange={e => {
      const next = localClients.find(c => c.id === e.target.value); if (next) { setOutput(null); setClient(next.id); }
    }}>{localClients.map(c => <option key={c.id} value={c.id}>{c.label}</option>)}</SettingsSelect></label>
    <div className="remote-publish-actions">
      <button type="button" className="btn btn-primary" disabled={!output || busy} onClick={() => void useConfiguration('thisComputer', 'copy')}>Copy for this computer</button>
      {shareable && <button type="button" className="btn" disabled={!output || busy} onClick={() => void useConfiguration('projectFolder', 'copy')}>Copy to share with project</button>}
      <button type="button" className="btn" disabled={!output || busy} onClick={() => void useConfiguration(output?.target ?? 'thisComputer', 'download')}>Download local configuration</button>
    </div>
    {output && <>
      <p>Paste the <code>ax</code> entry into <strong>{output.destination}</strong>, then reload your app.</p>
      <p className="remote-publish-state">{output.portable ? 'Share this file with the project. Each person needs Ax installed and their own local copy of the project.' : 'This configuration includes this computer’s executable and project paths. Use the project sharing button for another checkout, or the remote card below to access this computer’s hosted project.'}</p>
      <details open={preview} onToggle={e => setPreview(e.currentTarget.open)}><summary>View configuration and instructions</summary>
        <textarea ref={text} aria-label="Generated local MCP configuration" readOnly value={output.content} rows={10} spellCheck={false} />
        <ol>{output.instructions.map(i => <li key={i}>{i}</li>)}</ol>
      </details>
    </>}
    <p>Want Ax to configure your installed app for you? <a href={`/settings?section=agents${new URLSearchParams(window.location.search).get('embed') === '1' ? '&embed=1' : ''}`}>Open IDEs &amp; agents</a>.</p>
    {busy && <p role="status">Preparing local configuration…</p>}{message && <p role="status">{message}</p>}{error && <p role="alert">{error}</p>}
  </section>;
}
