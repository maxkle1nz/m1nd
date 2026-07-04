import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// The served owner is loopback :1337 by default; a second local owner (e.g. a
// dogfood serve) can bind another port. `M1ND_API` overrides the dev proxy target
// so `--dev` iteration can point at whichever owner is live (default unchanged).
const API_TARGET = process.env.M1ND_API ?? 'http://localhost:1337';

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: API_TARGET,
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
