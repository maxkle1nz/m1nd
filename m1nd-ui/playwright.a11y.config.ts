/*
 * The ACCESSIBILITY lane — a SEPARATE proof, on purpose.
 *
 * G7 (docs/benchmarks/G7-LIVE-CEREMONY.md §5) counts "UI unit · accessibility ·
 * browser fixture · browser LIVE" as four separate proofs, so the accessibility
 * smoke gets its own directory, its own runner and its own reported result —
 * exactly as the LIVE gate does in `playwright.live.config.ts`. The ordinary
 * `playwright.config.ts` (testDir `./e2e`) never picks these specs up.
 *
 * It boots its own Vite dev server on a private port — 5198, one below the
 * fixture suite's 5199 — so the two lanes can run back to back (or side by side)
 * without fighting over a socket. No live owner is contacted: the specs mock
 * every /api/* route in-page.
 */
import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './e2e-a11y',
  fullyParallel: false,
  workers: 1,
  timeout: 60_000,
  metadata: { gate: 'G7_ACCESSIBILITY_SMOKE' },
  use: {
    baseURL: 'http://localhost:5198',
    screenshot: 'only-on-failure',
  },
  webServer: {
    command: 'npm run dev -- --port 5198 --strictPort',
    url: 'http://localhost:5198',
    reuseExistingServer: false,
    timeout: 30_000,
  },
  reporter: [['list']],
});
