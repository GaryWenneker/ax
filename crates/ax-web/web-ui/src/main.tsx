import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import '@vscode/codicons/dist/codicon.css';
import '@uiw/react-md-editor/markdown-editor.css';
import '@uiw/react-markdown-preview/markdown.css';
import './index.css';
import './agent-terminal.css';
import './portal.css';
import App from './App';
import './controls.css';
import './mobile.css';
import './auth.css';
import { installContextMenuGuard } from './lib/contextMenuGuard';
import { installBrowserChunkReload } from './lib/chunkReload';
import { PWA_OPTIN_KEY, resolvePwaOptIn } from './lib/pwaOptIn';

installContextMenuGuard();
installBrowserChunkReload();

const root = document.getElementById('root');
if (root) {
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}

/**
 * Service workers caused empty Command Center pages (stale HTML shell / broken
 * intercept of navigations). Default: unregister + clear caches so /api/* always
 * hits the live server. Opt-in PWA only with `?pwa=1`, the stored opt-in, or an installed standalone
 * window; `?pwa=0` opts out again (see lib/pwaOptIn.ts).
 */
if ('serviceWorker' in navigator) {
  window.addEventListener('load', () => {
    const params = new URLSearchParams(window.location.search);
    let optInStored = false;
    try {
      optInStored = localStorage.getItem(PWA_OPTIN_KEY) === '1';
    } catch {
      /* ignore */
    }
    const { enable: enablePwa, clearStored } = resolvePwaOptIn(
      params.get('pwa'),
      optInStored,
      window.matchMedia('(display-mode: standalone)').matches,
    );
    if (clearStored) {
      try {
        localStorage.removeItem(PWA_OPTIN_KEY);
      } catch {
        /* ignore */
      }
    }

    void (async () => {
      try {
        const regs = await navigator.serviceWorker.getRegistrations();
        if (!enablePwa) {
          await Promise.all(regs.map((r) => r.unregister()));
          if ('caches' in window) {
            const keys = await caches.keys();
            await Promise.all(keys.map((k) => caches.delete(k)));
          }
          return;
        }
        await navigator.serviceWorker.register('/sw.js', { updateViaCache: 'none' });
      } catch {
        /* optional */
      }
    })();
  });
}
