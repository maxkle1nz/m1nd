import { test as base } from '@playwright/test';
import { loadLiveAuthorizationHeader } from './live-config';

/**
 * Install optional bearer auth inside the already-created browser context,
 * immediately before the spec receives `page`. The secret never enters the
 * serializable Playwright config, metadata, argv, URL, storage-state file, or
 * test artifacts. Tracing, screenshots, and video are also disabled by config.
 */
export const test = base.extend({
  page: async ({ page }, provide) => {
    const authorizationHeader = loadLiveAuthorizationHeader();
    if (authorizationHeader) {
      await page.context().setExtraHTTPHeaders({ Authorization: authorizationHeader });
    }
    await provide(page);
  },
});
