import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: 'http://localhost:1337',
        changeOrigin: true,
      },
    },
  },
  build: {
    outDir: 'dist',
    sourcemap: false,
    // Vite 8 is rolldown-based and no longer bundles esbuild; use its native
    // Oxc minifier (no separate esbuild dependency — reuse-first).
    minify: 'oxc',
  },
});
