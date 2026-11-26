import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import '@vscode/codicons/dist/codicon.css';
import '@cc/index.css';
import '@cc/portal.css';
import '@cc/controls.css';
import '@cc/mobile.css';
import './admin.css';
import { initTheme } from '@cc/lib/themes';
import App from './App';

initTheme();

const root = document.getElementById('root');
if (root) {
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}
