import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react-swc';

// The Command Center's stylesheet, theme tokens and page components are used
// as they are, so both interfaces look and behave the same.
const commandCenter = fileURLToPath(new URL('../../ax-web/web-ui/src', import.meta.url));

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: { '@cc': commandCenter },
    // Command Center sources live outside this package; their React must be ours.
    dedupe: ['react', 'react-dom'],
  },
  build: {
    outDir: 'dist',
    assetsDir: 'assets',
    sourcemap: false,
  },
  server: {
    fs: { allow: ['.', commandCenter] },
    proxy: {
      '/api': { target: 'http://127.0.0.1:7080', changeOrigin: false },
    },
  },
});
