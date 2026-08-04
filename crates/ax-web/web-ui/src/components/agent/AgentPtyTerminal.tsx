import { useEffect, useRef } from 'react';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { AX_LOG_ICON } from '../../lib/mcpTrace';
import '@xterm/xterm/css/xterm.css';

interface Props {
  agent: string;
  profileId: string;
}

function wsUrl(agent: string, profileId: string): string {
  const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  const params = new URLSearchParams({ agent, profile: profileId });
  return `${proto}//${window.location.host}/api/agent/pty/ws?${params}`;
}

export default function AgentPtyTerminal({ agent, profileId }: Props) {
  const hostRef = useRef<HTMLDivElement>(null);
  const sessionKey = `${agent}:${profileId}`;

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    const term = new Terminal({
      cursorBlink: true,
      fontSize: 14,
      fontFamily: "'Cascadia Code', 'Consolas', 'Courier New', monospace",
      theme: {
        background: '#0d1117',
        foreground: '#e6edf3',
        cursor: '#58a6ff',
      },
      scrollback: 8000,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host);
    fit.fit();

    let ws: WebSocket | null = null;
    let disposed = false;
    let retryTimer: ReturnType<typeof setTimeout> | null = null;
    let retries = 0;
    const MAX_RETRIES = 5;

    function connect() {
      if (disposed) return;
      ws = new WebSocket(wsUrl(agent, profileId));
      ws.onopen = () => {
        retries = 0;
        term.writeln(`\x1b[36m[ax] ${AX_LOG_ICON} Connected — ${agent} interactive CLI\x1b[0m`);
        sendResize();
      };
      ws.onmessage = (ev) => {
        try {
          const msg = JSON.parse(String(ev.data)) as { t: string; d?: string; m?: string };
          if (msg.t === 'o' && msg.d) term.write(msg.d);
          if (msg.t === 'e' && msg.m) {
            const line = msg.m.startsWith('[ax]') ? msg.m : `[ax] ${AX_LOG_ICON} ${msg.m}`;
            term.writeln(`\r\n\x1b[31m${line}\x1b[0m`);
          }
        } catch {
          term.write(String(ev.data));
        }
      };
      ws.onclose = () => {
        if (disposed) return;
        if (retries < MAX_RETRIES) {
          retries += 1;
          const delay = Math.min(1000 * 2 ** (retries - 1), 10_000);
          term.writeln(
            `\r\n\x1b[33m[ax] ${AX_LOG_ICON} Disconnected — reconnecting in ${Math.round(delay / 1000)}s (${retries}/${MAX_RETRIES})\x1b[0m`,
          );
          retryTimer = setTimeout(connect, delay);
        } else {
          term.writeln(`\r\n\x1b[31m[ax] ${AX_LOG_ICON} Disconnected — reload the page to reconnect\x1b[0m`);
        }
      };
      ws.onerror = () => {
        term.writeln(`\r\n\x1b[31m[ax] ${AX_LOG_ICON} WebSocket error\x1b[0m`);
      };
    }

    const onData = term.onData((data) => {
      if (ws?.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ t: 'i', d: data }));
      }
    });

    function sendResize() {
      if (ws?.readyState !== WebSocket.OPEN) return;
      fit.fit();
      ws.send(
        JSON.stringify({
          t: 'r',
          cols: term.cols,
          rows: term.rows,
        }),
      );
    }

    const ro = new ResizeObserver(() => sendResize());
    ro.observe(host);
    window.addEventListener('resize', sendResize);
    connect();

    return () => {
      disposed = true;
      if (retryTimer) clearTimeout(retryTimer);
      onData.dispose();
      ro.disconnect();
      window.removeEventListener('resize', sendResize);
      ws?.close();
      term.dispose();
    };
  }, [sessionKey, agent, profileId]);

  return <div className="agent-pty-host" ref={hostRef} />;
}
