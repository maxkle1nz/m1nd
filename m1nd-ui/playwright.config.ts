/*
 * Playwright e2e — the deterministic browser gate (the uiproof standard: the
 * runner is `npx playwright test`, seconds, no agent in the loop). The suite
 * boots ITS OWN Vite dev server on a private port (5199, strictPort) and mocks
 * every /api/* route inside the page context — no request ever reaches a live
 * owner (the maintainer's served owner is out of bounds for tests, AGENTS.md).
 */
import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  // One shared dev server + stateful route mocks per test — serial keeps the
  // runs deterministic (the specs are few and fast).
  fullyParallel: false,
  workers: 1,
  timeout: 60_000,
  use: {
    baseURL: 'http://localhost:5199',
    screenshot: 'only-on-failure',
  },
  webServer: {
    command: 'npm run dev -- --port 5199 --strictPort',
    url: 'http://localhost:5199',
    reuseExistingServer: false,
    timeout: 30_000,
  },
  reporter: [['list']],
});
