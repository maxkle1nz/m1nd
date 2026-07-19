/*
 * G7 LIVE browser gate. This config never starts or proxies a server: it binds
 * only to the explicitly supplied isolated owner after fail-closed validation.
 * The ordinary `playwright.config.ts` remains the deterministic mocked suite.
 */
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { defineConfig } from '@playwright/test';
import { loadLiveOwnerConfig } from './e2e-live/live-config';

const live = loadLiveOwnerConfig();

export default defineConfig({
  testDir: './e2e-live',
  testMatch: 'live-owner.spec.ts',
  fullyParallel: false,
  workers: 1,
  retries: 0,
  forbidOnly: true,
  timeout: 60_000,
  expect: { timeout: 30_000 },
  outputDir: join(tmpdir(), 'm1nd-g7-live-playwright'),
  preserveOutput: 'never',
  reporter: [['list']],
  metadata: {
    gate: 'G7_LIVE_REAL_BROWSER',
    ownerOrigin: live.ownerOrigin,
    ownerPort: live.ownerPort,
    expectedUiBundleSha256: live.expectedUiBundleSha256,
  },
  use: {
    baseURL: live.ownerOrigin,
    browserName: 'chromium',
    launchOptions: { executablePath: live.browserExecutable },
    serviceWorkers: 'block',
    navigationTimeout: 30_000,
    actionTimeout: 15_000,
    screenshot: 'off',
    trace: 'off',
    video: 'off',
  },
});
