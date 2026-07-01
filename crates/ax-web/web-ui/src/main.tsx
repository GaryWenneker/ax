import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import '@uiw/react-md-editor/markdown-editor.css';
import './index.css';
import App from './App';

const root = document.getElementById('root');
if (root) {
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}
